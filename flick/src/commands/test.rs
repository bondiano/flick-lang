//! `flick test`: builds the project with its test root into `.flick/test/`,
//! writes a runner that imports the `tests` of each test module, and runs it
//! on Node; a project that emits go, or `--emit go`, gets one executable,
//! `flick-test`, that runs them, and `--rt` runs them on Flick's own runtime.
//! With `--watch`, a change of a source file stops the run (closing the
//! runner's stdin: every running test closes its scope) and starts the next
//! one after it.

use std::fmt::Write;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime};
use std::{env, fs, io, thread};

use flick_core::build::{Artifact, go};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::{Backend, Emit};
use flick_core::project::{self, ModuleCache, Project};
use flick_core::source::SourceMap;
use tracing::debug;

use super::build::{BuildPlan, Emission, artifacts, release, write, write_artifacts};
use super::check::select;
use crate::args::{Color, GlobalArguments, Reporter, TestArguments, TestEmitArgument};
use crate::package_cache;
use crate::progress::{Mode, Progress};
use crate::sources::FsHost;
use crate::{builtins, cache, child, report, timings, toolchain};
use flick_core::build::Extras;

/// Where the test build goes, inside the project.
const OUT_DIR: &str = ".flick/test";

pub fn test(options: &TestArguments, global: &GlobalArguments) -> ExitCode {
    if options.watch {
        return watch(options, global);
    }
    let Ok(runner) = prepare(options, global, None) else {
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
#[allow(clippy::too_many_lines)]
fn prepare(
    options: &TestArguments,
    global: &GlobalArguments,
    modules: Option<&mut ModuleCache>,
) -> Result<PathBuf, ()> {
    let progress = Progress::new(if matches!(options.reporter, Reporter::Json) {
        Mode::Hidden
    } else {
        Mode::for_cli(global)
    });
    let mut sources = SourceMap::default();
    let host = FsHost::new();
    let fail = |sources: &SourceMap, diagnostics: Vec<Diagnostic>| {
        report::render(global, sources, &diagnostics);
        Err(())
    };
    progress.stage("Resolving project");
    let (project, roots) = match select(&host, &mut sources, global.project.as_deref(), &[]) {
        Ok(selected) => selected,
        Err(diagnostic) => {
            drop(progress);
            return fail(&sources, vec![diagnostic]);
        }
    };
    let Some(project) = project else {
        return Err(());
    };
    let wanted: Vec<PathBuf> = options
        .paths
        .iter()
        .map(|p| project::normalize(&host.cwd.join(p)))
        .collect();
    let mut selected = wanted.clone();
    selected.sort();
    selected.dedup();
    let go_requested = match options.emit {
        Some(target) => target == TestEmitArgument::Go,
        None => project.config.build.emit == Emit::Go,
    };
    let rt = options.rt || (!go_requested && project.config.build.effect == Backend::Rt);
    let settings = format!(
        "test {:?} {:?} {:?} {rt}",
        selected,
        match options.emit {
            Some(TestEmitArgument::Go) => "go",
            Some(TestEmitArgument::Js) => "js",
            None => "default",
        },
        release(options.profile, Some(&project.config.build)),
    );
    let key = cache::key(&project, &roots, &settings, !go_requested && !rt);
    let out = project.root.join(OUT_DIR);
    progress.stage("Checking test cache");
    if let Some(runner) = cached_prepare(&project, &key, &out, &progress) {
        return Ok(runner);
    }
    progress.stage("Preparing compiler");
    if let Err(diagnostic) = builtins::prepare_in(project::stages_by_class(Some(&project), rt)) {
        drop(progress);
        return fail(&sources, vec![diagnostic]);
    }
    progress.stage("Compiling Flick modules");
    let (mut diagnostics, mut compilation) = package_cache::with_saved(|saved| {
        let (diagnostics, compilation, fresh) = project::compile_with(
            &host,
            &mut sources,
            Some(&project),
            &roots,
            saved,
            rt,
            modules,
        );
        ((diagnostics, compilation), fresh)
    });
    super::build::hooks::ir(Some(&project), &mut compilation, &mut diagnostics);
    let errors = diagnostics.iter().any(|d| d.severity == Severity::Error);
    let progress = if diagnostics.is_empty() {
        progress
    } else {
        drop(progress);
        report::render(global, &sources, &diagnostics);
        if errors {
            return Err(());
        }
        Progress::new(if matches!(options.reporter, Reporter::Json) {
            Mode::Hidden
        } else {
            Mode::for_cli(global)
        })
    };
    let _ = fs::remove_dir_all(&out);
    let suites: Vec<(String, String)> = compilation
        .modules
        .iter()
        .filter(|m| m.test && m.interface.exports_value("tests"))
        .filter(|m| wanted.is_empty() || wanted.contains(&m.path))
        .map(|m| (m.name.clone(), format!("./{}.js", m.name.replace('.', "/"))))
        .collect();
    if go_requested && rt {
        drop(progress);
        let message = "Flick's own runtime has only JavaScript output yet";
        return fail(
            &sources,
            vec![Diagnostic::new(codes::F004, Severity::Error, message)],
        );
    }
    // with no suite, the go target has no program: the node runner reports the empty run
    let go = go_requested && !suites.is_empty();
    if !go && let Some(problem) = toolchain::check_runtime(&project.root, rt) {
        drop(progress);
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
        extras: Extras {
            rt,
            ..Extras::default()
        },
        release: release(options.profile, Some(&project.config.build)),
    };
    progress.stage(if go {
        "Emitting Go"
    } else {
        "Emitting JavaScript"
    });
    let mut emitted = artifacts(&host, &sources, Some(&project), &compilation, &output);
    write_artifacts(&output, &mut emitted, &progress);
    if !emitted.diagnostics.is_empty() {
        drop(progress);
        return fail(&sources, emitted.diagnostics);
    }
    if go {
        if diagnostics.is_empty()
            && cache::key(&project, &roots, &settings, !go_requested && !rt) == key
            && cache::read_unchanged(&project, &roots, &host.cwd, &sources)
        {
            cache::store(&project.root, &key, &emitted.artifacts);
            cache::mark_go(&project.root, &key, &out, &go::executable(go::TEST));
        }
        drop(progress);
        return Ok(out.join(go::executable(go::TEST)));
    }
    progress.stage("Writing test runner");
    let runner = out.join("test.js");
    if let Err(err) = write(&runner, &runner_source(&suites, rt)) {
        drop(progress);
        eprintln!("error: cannot write `{}`: {err}", runner.display());
        return Err(());
    }
    if diagnostics.is_empty()
        && cache::key(&project, &roots, &settings, !go_requested && !rt) == key
        && cache::read_unchanged(&project, &roots, &host.cwd, &sources)
    {
        emitted.artifacts.push(Artifact {
            path: "test.js".to_owned(),
            contents: runner_source(&suites, rt),
        });
        cache::store(&project.root, &key, &emitted.artifacts);
    }
    drop(progress);
    Ok(runner)
}

fn cached_prepare(
    project: &Project,
    key: &cache::Signed,
    out: &Path,
    progress: &Progress,
) -> Option<PathBuf> {
    let Some(artifacts) = cache::load(&project.root, key) else {
        debug!(miss = %cache::missed(&project.root, key));
        return None;
    };
    let go = artifacts.iter().any(|a| a.path == "go/test/go.mod");
    let runner = out.join(if go {
        go::executable(go::TEST)
    } else {
        "test.js".to_owned()
    });
    if cache::artifacts_current(out, &artifacts)
        && (!go || cache::go_current(&project.root, key, out, &go::executable(go::TEST)))
    {
        progress.stage("Reusing test build");
        return Some(runner);
    }
    progress.stage("Restoring cached test artifacts");
    let output = BuildPlan {
        out_dir: out.to_path_buf(),
        entries: Vec::new(),
        typescript: false,
        go,
        suites: if go {
            vec!["cached".to_owned()]
        } else {
            Vec::new()
        },
        extras: Extras::default(),
        release: false,
    };
    let mut emitted = Emission {
        artifacts,
        diagnostics: Vec::new(),
    };
    write_artifacts(&output, &mut emitted, progress);
    if emitted.diagnostics.is_empty() {
        if go {
            cache::mark_go(&project.root, key, out, &go::executable(go::TEST));
        }
        Some(runner)
    } else {
        None
    }
}

/// The runner: imports each suite's `tests` and runs them all, on Flick's own
/// runtime with `rt`.
fn runner_source(suites: &[(String, String)], rt: bool) -> String {
    let runtime = if rt {
        "@flick-lang/runtime/rt"
    } else {
        "@flick-lang/effect-ts"
    };
    let mut out = format!("import {{ runTests }} from \"{runtime}\";\n");
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
    if matches!(options.reporter, Reporter::Human)
        && matches!(Mode::for_cli(global), Mode::Live { .. })
        && io::stdout().is_terminal()
    {
        command.args(["--progress", "live"]);
    }
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

/// What a watch of `project` follows: its roots, its configuration and the
/// libraries it depends on by path.
pub fn watched_roots(project: &Project) -> Vec<PathBuf> {
    // shortcut: only direct path dependencies are watched, add theirs when a workspace nests libraries
    let libraries = project
        .config
        .dependencies
        .iter()
        .filter_map(|d| Some(project::normalize(&project.root.join(d.path()?))));
    [
        project.source_root(),
        project.test_root(),
        project.root.join(project::CONFIG_FILE),
    ]
    .into_iter()
    .chain(libraries)
    .collect()
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
    // what the runs of this session compiled: a change compiles what it reaches
    let mut modules = ModuleCache::default();
    loop {
        let before = watch_snapshot(&paths);
        let prepared = prepare(options, global, Some(&mut modules));
        // each run reports its build, before its tests start
        timings::report(global);
        let mut child = prepared
            .ok()
            .and_then(|runner| start(&runner, options, global, true).ok());
        while watch_snapshot(&paths) == before {
            thread::sleep(Duration::from_millis(200));
        }
        if let Some(child) = &mut child {
            stop(child);
        }
        timings::discard();
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
