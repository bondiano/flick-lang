//! `flick test`: builds the project with its test root into `.flick/test/`,
//! writes a runner that imports the `tests` of each test module, and runs it
//! on Node. With `--watch`, a change of a source file stops the run (closing
//! the runner's stdin: every running test closes its scope) and starts the
//! next one after it.

use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime};
use std::{fs, io, thread};

use flick_core::diagnostics::{Diagnostic, Severity};
use flick_core::project::{self, Project};
use flick_core::source::SourceMap;

use super::build::{Output, emit, release, write};
use super::check::select;
use crate::args::{GlobalArguments, Reporter, TestArguments};
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
    let mut command = runner_command(&runner, options, false);
    match child::spawn(&mut command, global, "application-stderr").and_then(|r| r.wait(&signals)) {
        Ok(status) => ExitCode::from(child::code(status)),
        Err(err) => {
            eprintln!("error: cannot run node: {err}");
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
    if let Some(problem) = toolchain::check_runtime(&project.root) {
        return fail(&sources, vec![problem]);
    }
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
    let output = Output {
        out_dir: out.clone(),
        entries: Vec::new(),
        typescript: false,
        extras: Extras::default(),
        release: release(options.profile, Some(&project.config.build)),
    };
    let (problems, _) = emit(&host, &sources, Some(&project), &compilation, &output);
    if !problems.is_empty() {
        return fail(&sources, problems);
    }
    let suites: Vec<(String, String)> = compilation
        .modules
        .iter()
        .filter(|m| m.test && m.interface.exports_value("tests"))
        .filter(|m| wanted.is_empty() || wanted.contains(&m.path))
        .map(|m| (m.name.clone(), format!("./{}.js", m.name.replace('.', "/"))))
        .collect();
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

fn start(runner: &Path, options: &TestArguments, watched: bool) -> io::Result<Child> {
    runner_command(runner, options, watched).spawn()
}

fn runner_command(runner: &Path, options: &TestArguments, watched: bool) -> Command {
    let mut command = Command::new("node");
    command.arg("--enable-source-maps").arg(runner);
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

/// The latest modification time under `paths`.
pub fn stamp(paths: &[PathBuf]) -> Option<SystemTime> {
    fn newest(path: &Path, out: &mut Option<SystemTime>) {
        let Ok(meta) = fs::metadata(path) else {
            return;
        };
        if let Ok(time) = meta.modified() {
            *out = (*out).max(Some(time));
        }
        if meta.is_dir()
            && let Ok(entries) = fs::read_dir(path)
        {
            for entry in entries.flatten() {
                newest(&entry.path(), out);
            }
        }
    }
    let mut out = None;
    for p in paths {
        newest(p, &mut out);
    }
    out
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
        let before = stamp(&paths);
        let mut child = prepare(options, global)
            .ok()
            .and_then(|runner| start(&runner, options, true).ok());
        while stamp(&paths) == before {
            thread::sleep(Duration::from_millis(200));
        }
        if let Some(child) = &mut child {
            stop(child);
        }
        println!("\n— a file changed: running the tests again —\n");
    }
}
