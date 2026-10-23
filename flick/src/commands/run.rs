//! `flick run` and `flick dev`. Each build of the app goes into its own
//! generation `.flick/run/<n>/`, never written again once it runs, so a late
//! `import()` of a running generation never sees the next build. A
//! generation is locked while its process runs and removed after it ends;
//! `flick clean` keeps locked generations. `run` and `dev` always build
//! JavaScript, whatever `emit` the project chooses.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitCode};
use std::sync::atomic::AtomicUsize;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::{self, Project, cache};
use flick_core::source::SourceMap;

use super::build::{BuildCommand, write};
use crate::args::{BuildArguments, EmitArgument, GlobalArguments, RunArguments};
use crate::child::{self, Running};
use crate::command::Command;
use crate::sources::FsHost;
use crate::{report, toolchain};

/// The generations of a project.
pub fn generations(root: &Path) -> PathBuf {
    root.join(cache::DIR).join("run")
}

/// Whether the process that locked `generation` still runs.
pub fn locked(generation: &Path) -> bool {
    let Ok(pid) = fs::read_to_string(generation.join(".lock")) else {
        return false;
    };
    let pid = pid.trim();
    // without /proc (not Linux) a lock is assumed held
    !Path::new("/proc/self").exists() || Path::new("/proc").join(pid).exists()
}

/// The project, or the diagnostics that stop the command.
pub fn find_project(global: &GlobalArguments) -> Result<Project, (SourceMap, Vec<Diagnostic>)> {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let explicit = global.project.as_deref().map(Path::new);
    match project::discover(&host, &mut sources, &host.cwd, explicit) {
        Ok(Some(p)) => Ok(p),
        Ok(None) => {
            let message = format!(
                "no `{}` in the working directory or any parent",
                project::CONFIG_FILE
            );
            Err((sources, vec![project::no_project(message)]))
        }
        Err(d) => Err((sources, vec![d])),
    }
}

/// Builds the app into a new generation; `Err` once diagnostics are shown.
pub fn build_generation(
    project: &Project,
    options: &RunArguments,
    global: &GlobalArguments,
) -> Result<PathBuf, u8> {
    if options.entry.is_none() && project.config.entry.is_none() {
        let d = Diagnostic::new(
            codes::P007,
            Severity::Error,
            "a library has no `main` to run: give `--entry App.Main`",
        );
        report::render(global, &SourceMap::default(), &[d]);
        return Err(2);
    }
    let dir = generations(&project.root);
    let next = fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| e.file_name().to_str()?.parse::<u64>().ok())
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
        + 1;
    let generation = dir.join(next.to_string());
    let build = BuildCommand {
        options: BuildArguments {
            paths: Vec::new(),
            out_dir: Some(generation.to_string_lossy().into_owned()),
            emit: Some(EmitArgument::Js),
            emit_dts: false,
            promise_facade: false,
            schema_aot: false,
            profile: options.profile,
            watch: false,
        },
        project: Some(project.root.to_string_lossy().into_owned()),
        entry: options.entry.clone(),
        generation: true,
    };
    let outcome = build.run();
    report::render(global, &outcome.sources, &outcome.diagnostics);
    let code = report::code(&outcome.diagnostics);
    let failed = code != 0 || (global.warnings_as_errors && !outcome.diagnostics.is_empty());
    if failed {
        let _ = fs::remove_dir_all(&generation);
        return Err(code.max(1));
    }
    Ok(generation)
}

/// Starts the runner of `generation` and locks it.
pub fn launch(generation: &Path, args: &[String], global: &GlobalArguments) -> Option<Running> {
    let mut command = Process::new("node");
    command
        .arg("--enable-source-maps")
        .arg(generation.join("main.js"))
        .args(args);
    match child::spawn(&mut command, global, "application-stderr") {
        Ok(running) => {
            let _ = write(&generation.join(".lock"), &running.child.id().to_string());
            Some(running)
        }
        Err(err) => {
            let d = Diagnostic::new(
                codes::P015,
                Severity::Error,
                format!("cannot run `node` (Node.js >= {}): {err}", toolchain::NODE),
            );
            report::render(global, &SourceMap::default(), &[d]);
            None
        }
    }
}

pub fn run(options: &RunArguments, global: &GlobalArguments) -> ExitCode {
    let signals = child::signals();
    let project = match find_project(global) {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            report::render(global, &sources, &diagnostics);
            return report::exit_code(&diagnostics);
        }
    };
    let generation = match build_generation(&project, options, global) {
        Ok(g) => g,
        Err(code) => return ExitCode::from(code),
    };
    let Some(running) = launch(&generation, &options.args, global) else {
        let _ = fs::remove_dir_all(&generation);
        return ExitCode::from(2);
    };
    let code = finish(running, &signals);
    let _ = fs::remove_dir_all(&generation);
    ExitCode::from(code)
}

/// Waits for the app, forwarding signals; its exit code.
fn finish(running: Running, signals: &AtomicUsize) -> u8 {
    match running.wait(signals) {
        Ok(status) => child::code(status),
        Err(_) => 1,
    }
}
