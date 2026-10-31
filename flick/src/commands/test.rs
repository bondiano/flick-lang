//! `flick test`: builds the project with its test root into `.flick/test/`,
//! writes a runner that imports the `tests` of each test module, and runs it
//! on Node; a project that emits go, or `--emit go`, gets one executable,
//! `flick-test`, that runs them. With `--watch`, a change of a source file stops the run (closing
//! the runner's stdin: every running test closes its scope) and starts the
//! next one after it.

use std::fmt::Write;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime};
use std::{env, fs, io, thread};

use flick_core::build::go;
use flick_core::diagnostics::{Diagnostic, Severity};
use flick_core::project::config::Emit;
use flick_core::project::{self, Project};
use flick_core::source::SourceMap;

use super::build::{BuildPlan, emit, release, write};
use super::check::select;
use crate::args::{Color, GlobalArguments, Reporter, TestArguments, TestEmitArgument};
use crate::sources::FsHost;
use crate::{child, report, toolchain};
use flick_core::build::Extras;

/// Where the test build goes, inside the project.
const OUT_DIR: &str = ".flick/test";

pub fn test(options: &TestArguments, global: &GlobalArguments) -> ExitCode {
    if options.watch {
        return watch(options, global);
    }
    let Ok(runner) = prepare(options, global) else {
        return ExitCode::from(1);
    };
    let signals = child::signals();
    let mut command = runner_command(&runner, options, global, false);
    match child::spawn(&mut command, global, "application-stderr").and_then(|r| r.wait(&signals)) {
        Ok(status) => ExitCode::from(child::code(status)),
        Err(err) => {
            eprintln!("error: cannot run `{}`: {err}", runner.display());
            ExitCode::from(2)
        }
    }
}

/// Builds the test run and writes its runner.
fn prepare(options: &TestArguments, global: &GlobalArguments) -> Result<PathBuf, ()> {
    let mut sources = SourceMap::default();
    let host = FsHost::new();
    let fail = |sources: &SourceMap, diagnostics: Vec<Diagnostic>| {
        report::render(global, sources, &diagnostics);
        Err(())
    };
    let (project, roots) = match select(&host, &mut sources, global.project.as_deref(), &[]) {
        Ok(selected) => selected,
        Err(diagnostic) => return fail(&sources, vec![diagnostic]),
    };
    let Some(project) = project else {
        return Err(());
    };
    let wanted: Vec<PathBuf> = options
        .paths
        .iter()
        .map(|p| project::normalize(&host.cwd.join(p)))
        .collect();
    let (diagnostics, compilation) = project::compile(&host, &mut sources, Some(&project), &roots);
    let errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
    if !diagnostics.is_empty() {
        report::render(global, &sources, &diagnostics);
    }
    if errors {
        return Err(());
    }
    let out = project.root.join(OUT_DIR);
    let _ = fs::remove_dir_all(&out);
    let suites: Vec<(String, String)> = compilation
        .modules
        .iter()
        .filter(|m| m.test && m.interface.exports_value("tests"))
        .filter(|m| wanted.is_empty() || wanted.contains(&m.path))
        .map(|m| (m.name.clone(), format!("./{}.js", m.name.replace('.', "/"))))
        .collect();
    let go = match options.emit {
        Some(target) => target == TestEmitArgument::Go,
        None => project.config.build.emit == Emit::Go,
    };
    // with no suite, the go target has no program: the node runner reports the empty run
    let go = go && !suites.is_empty();
    if !go && let Some(problem) = toolchain::check_runtime(&project.root) {
        return fail(&sources, vec![problem]);
    }
    let output = BuildPlan {
        out_dir: out.clone(),
        entries: Vec::new(),
        typescript: false,
        go,
        suites: if go {
            suites.iter().map(|(name, _)| name.clone()).collect()
        } else {
            Vec::new()
        },
        extras: Extras::default(),
        release: release(options.profile, Some(&project.config.build)),
    };
    let emitted = emit(&host, &sources, Some(&project), &compilation, &output);
    if !emitted.diagnostics.is_empty() {
        return fail(&sources, emitted.diagnostics);
    }
    if go {
        return Ok(out.join(go::executable(go::TEST)));
    }
    let runner = out.join("test.js");
    if let Err(err) = write(&runner, &runner_source(&suites)) {
        eprintln!("error: cannot write `{}`: {err}", runner.display());
        return Err(());
    }
    Ok(runner)
}

/// The runner: imports each suite's `tests` and runs them all.
fn runner_source(suites: &[(String, String)]) -> String {
    let mut out = String::from("import { runTests } from \"@flick-lang/runtime\";\n");
    for (i, (_, path)) in suites.iter().enumerate() {
        let _ = writeln!(out, "import * as $s{i} from {path:?};");
    }
    let list: Vec<String> = suites
        .iter()
        .enumerate()
        .map(|(i, (name, _))| format!("[{name:?}, $s{i}.tests]"))
        .collect();
    let _ = write!(out, "\nawait runTests([{}]);\n", list.join(", "));
    out
}

fn start(
    runner: &Path,
    options: &TestArguments,
    global: &GlobalArguments,
    watched: bool,
) -> io::Result<Child> {
    runner_command(runner, options, global, watched).spawn()
}

fn runner_command(
    runner: &Path,
    options: &TestArguments,
    global: &GlobalArguments,
    watched: bool,
) -> Command {
    // the runner is the test executable of the go target, or a script for node
    let mut command = if runner.extension().is_some_and(|e| e == "js") {
        let mut command = Command::new("node");
        command.arg("--enable-source-maps").arg(runner);
        command
    } else {
        Command::new(runner)
    };
    if watched {
        command.arg("--watched").stdin(Stdio::piped());
    }
    if let Some(filter) = &options.filter {
        command.args(["--filter", filter]);
    }
    command.args(["--jobs", &options.jobs.to_string()]);
    if let Some(seed) = options.seed {
        command.args(["--seed", &seed.to_string()]);
    }
    let reporter = match options.reporter {
        Reporter::Human => "human",
        Reporter::Json => "json",
    };
    command.args(["--reporter", reporter]);
    let color = matches!(options.reporter, Reporter::Human)
        && match global.color {
            Color::Always => true,
            Color::Never => false,
            Color::Auto => {
                io::stdout().is_terminal()
                    && env::var_os("NO_COLOR").is_none()
                    && env::var_os("TERM").is_none_or(|term| term != "dumb")
            }
        };
    command.args(["--color", if color { "always" } else { "never" }]);
    command
}

/// The source and test roots and the configuration of the project.
pub fn watched(global: &GlobalArguments) -> Vec<PathBuf> {
    let host = FsHost::new();
    let explicit = global.project.as_deref().map(Path::new);
    match project::discover(&host, &mut SourceMap::default(), &host.cwd, explicit) {
        Ok(Some(project)) => watched_roots(&project),
        _ => vec![host.cwd],
    }
}

/// What a watch of `project` follows.
pub fn watched_roots(project: &Project) -> Vec<PathBuf> {
    vec![
        project.source_root(),
        project.test_root(),
        project.root.join(project::CONFIG_FILE),
    ]
}

/// Per-path filesystem state, independent of the newest timestamp in the tree.
#[derive(Debug, PartialEq, Eq)]
pub struct WatchSnapshot(Vec<WatchedPath>);

#[derive(Debug, PartialEq, Eq)]
struct WatchedPath {
    path: PathBuf,
    modified: Option<SystemTime>,
    size: u64,
    directory: bool,
}

/// Captures changes, additions and removals even when another file is future-dated.
pub fn watch_snapshot(paths: &[PathBuf]) -> WatchSnapshot {
    fn capture(path: &Path, out: &mut Vec<WatchedPath>) {
        let Ok(meta) = fs::metadata(path) else {
            return;
        };
        out.push(WatchedPath {
            path: path.to_path_buf(),
            modified: meta.modified().ok(),
            size: meta.len(),
            directory: meta.is_dir(),
        });
        if meta.is_dir()
            && let Ok(entries) = fs::read_dir(path)
        {
            for entry in entries.flatten() {
                capture(&entry.path(), out);
            }
        }
    }
    let mut out = Vec::new();
    for path in paths {
        capture(path, &mut out);
    }
    out.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    WatchSnapshot(out)
}

/// Stops a run: closing its stdin lets every running test close its scope,
/// and the wait returns only once the runner has exited.
fn stop(child: &mut Child) {
    drop(child.stdin.take());
    let _ = child.wait();
}

fn watch(options: &TestArguments, global: &GlobalArguments) -> ExitCode {
    let paths = watched(global);
    loop {
        let before = watch_snapshot(&paths);
        let mut child = prepare(options, global)
            .ok()
            .and_then(|runner| start(&runner, options, global, true).ok());
        while watch_snapshot(&paths) == before {
            thread::sleep(Duration::from_millis(200));
        }
        if let Some(child) = &mut child {
            stop(child);
        }
        println!("\n— a file changed: running the tests again —\n");
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs::{File, FileTimes};
    use std::process;
    use std::time::UNIX_EPOCH;

    use super::*;

    #[test]
    fn snapshots_follow_each_path_despite_future_timestamps() {
        let root = env::temp_dir().join(format!("flick-watch-snapshot-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("fixture");
        let future = root.join("Future.fl");
        let ordinary = root.join("Main.fl");
        fs::write(&future, "future").expect("future");
        fs::write(&ordinary, "1").expect("main");
        let set_time = |path: &Path, seconds| {
            File::open(path)
                .expect("file")
                .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
                .expect("timestamp");
        };
        set_time(&future, 4_102_444_800);
        set_time(&ordinary, 1_000);
        let paths = [root.clone()];
        let before = watch_snapshot(&paths);
        assert_eq!(before, watch_snapshot(&paths), "stable order and state");
        fs::write(&ordinary, "2").expect("change");
        set_time(&ordinary, 2_000);
        let changed = watch_snapshot(&paths);
        assert_ne!(before, changed, "the future file cannot mask another file");
        let created = root.join("New.fl");
        fs::write(&created, "new").expect("create");
        set_time(&created, 1_000);
        let added = watch_snapshot(&paths);
        assert_ne!(changed, added, "creation changes path identity");
        fs::remove_file(created).expect("delete");
        assert_ne!(
            added,
            watch_snapshot(&paths),
            "deletion changes path identity"
        );
        fs::remove_dir_all(&root).expect("delete root");
        assert_eq!(watch_snapshot(&paths), WatchSnapshot(Vec::new()));
    }
}
