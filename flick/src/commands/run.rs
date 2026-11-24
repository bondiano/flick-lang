//! `flick run` and `flick dev`. Each build of the app goes into its own
//! generation `.flick/run/<pid>-<n>/`, never written again once it runs, so
//! a running process never sees the next build. Its CLI owns the directory
//! from reservation through cleanup; `clean` also keeps an orphaned live
//! child.

use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::atomic::AtomicUsize;
use std::{fs, io, process};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::MAIN_ENTRY;
use flick_core::project::{self, ModuleCache, Project, cache};
use flick_core::source::SourceMap;
#[cfg(unix)]
use rustix::io::Errno;
#[cfg(unix)]
use rustix::process::{Pid, test_kill_process};

use super::build::BuildCommand;
#[cfg(feature = "wasm-backends")]
use super::contributed;
use crate::args::{BuildArguments, GlobalArguments, RunArguments};
use crate::artifact::resolve_root;
use crate::backends;
use crate::child::{self, Running};
use crate::command::Command;
use crate::report;
use crate::sources::FsHost;

/// The generations of a project.
pub fn generations(root: &Path) -> PathBuf {
    resolve_root(root, root).join(cache::DIR).join("run")
}

/// A directory atomically reserved by this CLI until all cleanup ends.
pub struct Generation {
    #[cfg(feature = "wasm-backends")]
    backend: Option<flick_backend::BackendPin>,
    path: PathBuf,
}

impl Generation {
    fn reserve(root: &Path) -> io::Result<Self> {
        let dir = generations(root);
        fs::create_dir_all(&dir)?;
        let owner = process::id();
        for number in 1..=u64::MAX {
            // The owner is in the name, so clean cannot race lock-file creation.
            let path = dir.join(format!("{owner}-{number}"));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        #[cfg(feature = "wasm-backends")]
                        backend: None,
                    });
                }
                Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
                Err(err) => return Err(err),
            }
        }
        Err(io::Error::other("run generation identifiers exhausted"))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Generation {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Whether a generation's lock is held, conservatively keeping unknown states.
pub fn locked(generation: &Path) -> bool {
    if let Some((owner, _)) = generation
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('-'))
        && process_alive(owner)
    {
        return true;
    }
    let lock = generation.join(".lock");
    match fs::symlink_metadata(&lock) {
        Ok(metadata) if metadata.is_file() => {}
        Err(err) if err.kind() == ErrorKind::NotFound => return false,
        _ => return true,
    }
    let Ok(pids) = fs::read_to_string(lock) else {
        return true;
    };
    // one line per process of the generation: `flick dev` starts each server entry
    let mut pids = pids.lines().filter(|p| !p.trim().is_empty()).peekable();
    pids.peek().is_none() || pids.any(process_alive)
}

fn process_alive(pid: &str) -> bool {
    let Ok(pid) = pid.trim().parse::<i32>() else {
        return true;
    };
    if pid <= 0 {
        return true;
    }
    #[cfg(unix)]
    if let Some(pid) = Pid::from_raw(pid) {
        return test_kill_process(pid) != Err(Errno::SRCH);
    }
    true
}

/// The project, or the diagnostics that stop the command.
pub fn find_project(global: &GlobalArguments) -> Result<Project, (SourceMap, Vec<Diagnostic>)> {
    find_project_at(global.project.as_deref().map(Path::new))
}

pub(crate) fn find_project_at(
    explicit: Option<&Path>,
) -> Result<Project, (SourceMap, Vec<Diagnostic>)> {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
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

/// The entry `run` starts: `--entry` is `main`, else the one named, or the
/// only one the project has.
fn chosen_entry(project: &Project, options: &RunArguments) -> Result<String, String> {
    if options.entry.is_some() {
        return Ok(MAIN_ENTRY.to_owned());
    }
    let names: Vec<&str> = project
        .config
        .entries
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    let listed = || {
        names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    match (&options.name, names.as_slice()) {
        (_, []) => Err("a library has no `main` to run: give `--entry App.Main`".to_owned()),
        (Some(name), _) if names.contains(&name.as_str()) => Ok(name.clone()),
        (Some(name), _) => Err(format!(
            "the project has no entry `{name}`; its entries: {}",
            listed()
        )),
        (None, [only]) => Ok((*only).to_owned()),
        (None, _) => Err(format!(
            "the project has several entries; name the one to run: {}",
            listed()
        )),
    }
}

/// The entries `flick dev` starts: the one `run` would, or, when a
/// project with several is not given one, each that is not a client's,
/// which a bundler serves instead.
pub fn dev_entries(project: &Project, options: &RunArguments) -> Result<Vec<String>, String> {
    let entries = &project.config.entries;
    if options.entry.is_some() || options.name.is_some() || entries.len() <= 1 {
        return chosen_entry(project, options).map(|name| vec![name]);
    }
    Ok(entries
        .iter()
        .filter(|e| !e.client)
        .map(|e| e.name.clone())
        .collect())
}

/// Builds the app into a new generation, with the name of the entry to
/// start; `Err` once diagnostics are shown.
pub fn build_generation(
    project: &Project,
    options: &RunArguments,
    global: &GlobalArguments,
) -> Result<(Generation, String), u8> {
    let name = match chosen_entry(project, options) {
        Ok(name) => name,
        Err(message) => {
            let d = Diagnostic::new(codes::P007, Severity::Error, message);
            report::render(global, &SourceMap::default(), &[d]);
            return Err(2);
        }
    };
    build_into_generation(project, options, global, None).map(|generation| (generation, name))
}

/// Builds the app into a new generation; `Err` once diagnostics are shown.
pub fn build_into_generation(
    project: &Project,
    options: &RunArguments,
    global: &GlobalArguments,
    modules: Option<Rc<RefCell<ModuleCache>>>,
) -> Result<Generation, u8> {
    build_into_generation_for(project, options, global, modules, false)
}

/// Builds the app into a new generation for `run` (`dev = false`) or `dev` (`dev = true`).
pub fn build_into_generation_for(
    project: &Project,
    options: &RunArguments,
    global: &GlobalArguments,
    modules: Option<Rc<RefCell<ModuleCache>>>,
    dev: bool,
) -> Result<Generation, u8> {
    let current = find_project_at(Some(&project.root)).map_err(|(sources, diagnostics)| {
        let code = report::code(&diagnostics).max(1);
        report::render(global, &sources, &diagnostics);
        code
    })?;
    let project = &current;
    #[allow(unused_mut, reason = "mutated only with wasm-backends")]
    let mut generation = match Generation::reserve(&project.root) {
        Ok(generation) => generation,
        Err(err) => {
            let d = Diagnostic::new(
                codes::P013,
                Severity::Error,
                format!("cannot reserve a run generation: {err}"),
            );
            report::render(global, &SourceMap::default(), &[d]);
            return Err(2);
        }
    };
    let build = BuildCommand {
        options: BuildArguments {
            paths: Vec::new(),
            target: options.target.clone(),
            out_dir: Some(generation.path().to_string_lossy().into_owned()),
            emit_ir: None,
            profile: options.profile,
            labels: None,
            watch: false,
            inputs: false,
            no_native_overrides: options.no_native_overrides,
            backend_options: options.backend_options.clone(),
        },
        project: Some(project.root.to_string_lossy().into_owned()),
        display: None,
        entry: options.entry.clone(),
        generation: true,
        dev,
        built: RefCell::default(),
        modules,
    };
    let outcome = build.run();
    report::render(global, &outcome.sources, &outcome.diagnostics);
    let code = report::code(&outcome.diagnostics);
    let failed = code != 0
        || (global.warnings_as_errors
            && outcome
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Warning));
    if failed {
        return Err(code.max(1));
    }
    let Some(completed) = build.built.borrow_mut().take() else {
        return Err(1);
    };
    #[cfg(feature = "wasm-backends")]
    {
        generation.backend = completed.backend;
    }
    #[cfg(not(feature = "wasm-backends"))]
    let _ = completed;
    Ok(generation)
}

/// Starts the runner and records its child PID for cleanup after an orphaned CLI.
pub fn launch(
    generation: &Generation,
    name: &str,
    options: &RunArguments,
    global: &GlobalArguments,
) -> Option<Running> {
    #[cfg(feature = "wasm-backends")]
    let command = generation.backend.as_ref().map(|pin| {
        backends::launch::command(generation.path(), pin, flick_backend::LaunchKind::Run, name)
    });
    #[cfg(not(feature = "wasm-backends"))]
    let command: Option<Result<std::process::Command, String>> = None;
    let mut command = match command {
        Some(Ok(command)) => command,
        Some(Err(why)) => {
            report::render(global, &SourceMap::default(), &[backends::failure(why)]);
            return None;
        }
        None => {
            report::render(
                global,
                &SourceMap::default(),
                &[backends::failure(
                    "running requires WebAssembly backend support",
                )],
            );
            return None;
        }
    };
    command.args(&options.args);
    match child::spawn(&mut command, global, "application-stderr") {
        Ok(running) => {
            let _ = append_pid(&generation.path().join(".lock"), running.child.id());
            Some(running)
        }
        Err(err) => {
            let message = format!("cannot run entry `{name}`: {err}");
            let d = Diagnostic::new(codes::P015, Severity::Error, message);
            report::render(global, &SourceMap::default(), &[d]);
            None
        }
    }
}

/// Delegates a client entry to the selected backend's `run-client` verb.
fn run_client(
    project: &Project,
    name: &str,
    options: &RunArguments,
    signals: &AtomicUsize,
    global: &GlobalArguments,
) -> ExitCode {
    #[cfg(feature = "wasm-backends")]
    {
        contributed::run_client(
            project,
            options.target.as_deref(),
            Some(name),
            &options.args,
            signals,
            global,
        )
    }
    #[cfg(not(feature = "wasm-backends"))]
    {
        let _ = (project, name, options, signals);
        let d = Diagnostic::new(
            codes::P067,
            Severity::Error,
            "client entries require WebAssembly backend support",
        );
        report::render(global, &SourceMap::default(), &[d]);
        ExitCode::from(2)
    }
}

pub fn run(options: &RunArguments, global: &GlobalArguments) -> ExitCode {
    let signals = child::signals();
    let found = find_project(global);
    // A name that is not an entry of the project names a script.
    if options.entry.is_none()
        && let Some(name) = &options.name
        && !found
            .as_ref()
            .is_ok_and(|p| p.config.entries.iter().any(|e| &e.name == name))
        && let Some(code) = super::script(name, &options.args, global)
    {
        return code;
    }
    let project = match found {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            report::render(global, &sources, &diagnostics);
            return report::exit_code(&diagnostics);
        }
    };
    // A client entry runs through the selected backend's `run-client` verb.
    if options.entry.is_none()
        && let Ok(name) = chosen_entry(&project, options)
        && project
            .config
            .entries
            .iter()
            .any(|entry| entry.name == name && entry.client)
    {
        return run_client(&project, &name, options, &signals, global);
    }
    let (generation, name) = match build_generation(&project, options, global) {
        Ok(g) => g,
        Err(code) => return ExitCode::from(code),
    };
    let Some(running) = launch(&generation, &name, options, global) else {
        return ExitCode::from(2);
    };
    let code = finish(running, &signals);
    ExitCode::from(code)
}

/// Waits for the app, forwarding signals; its exit code.
fn finish(running: Running, signals: &AtomicUsize) -> u8 {
    match running.wait(signals) {
        Ok(status) => child::code(status),
        Err(_) => 1,
    }
}

/// Adds `pid` to the processes the lock of a generation records.
fn append_pid(lock: &Path, pid: u32) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(lock)?;
    writeln!(file, "{pid}")
}

#[cfg(all(test, feature = "wasm-backends"))]
#[path = "run/backend_tests.rs"]
mod backend_tests;

#[cfg(test)]
#[path = "run/tests.rs"]
mod tests;
