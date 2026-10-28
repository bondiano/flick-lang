//! `flick run` and `flick dev`. Each build of the app goes into its own
//! generation `.flick/run/<pid>-<n>/`, never written again once it runs, so
//! a late `import()` never sees the next build. Its CLI owns the directory
//! from reservation through cleanup; `clean` also keeps an orphaned live
//! child. `run` and `dev` always build
//! JavaScript, whatever `emit` the project chooses.

use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitCode};
use std::sync::atomic::AtomicUsize;
use std::{fs, io, process};

use flick_core::build::runner_file;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::MAIN_ENTRY;
use flick_core::project::{self, Project, cache};
use flick_core::source::SourceMap;
#[cfg(unix)]
use rustix::io::Errno;
#[cfg(unix)]
use rustix::process::{Pid, test_kill_process};

use super::build::BuildCommand;
use crate::args::{BuildArguments, EmitArgument, GlobalArguments, RunArguments};
use crate::child::{self, Running};
use crate::command::Command;
use crate::sources::FsHost;
use crate::{report, toolchain};

/// The generations of a project.
pub fn generations(root: &Path) -> PathBuf {
    root.join(cache::DIR).join("run")
}

/// A directory atomically reserved by this CLI until all cleanup ends.
pub struct Generation {
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
                Ok(()) => return Ok(Self { path }),
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
    build_into_generation(project, options, global).map(|generation| (generation, name))
}

/// Builds the app into a new generation; `Err` once diagnostics are shown.
pub fn build_into_generation(
    project: &Project,
    options: &RunArguments,
    global: &GlobalArguments,
) -> Result<Generation, u8> {
    let generation = match Generation::reserve(&project.root) {
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
            out_dir: Some(generation.path().to_string_lossy().into_owned()),
            emit: Some(EmitArgument::Js),
            emit_dts: false,
            promise_facade: false,
            schema_aot: false,
            profile: options.profile,
            labels: None,
            watch: false,
            inputs: false,
        },
        project: Some(project.root.to_string_lossy().into_owned()),
        entry: options.entry.clone(),
        generation: true,
        built: RefCell::default(),
    };
    let outcome = build.run();
    report::render(global, &outcome.sources, &outcome.diagnostics);
    let code = report::code(&outcome.diagnostics);
    let failed = code != 0 || (global.warnings_as_errors && !outcome.diagnostics.is_empty());
    if failed {
        return Err(code.max(1));
    }
    Ok(generation)
}

/// Starts the runner and records its child PID for cleanup after an orphaned CLI.
pub fn launch(
    generation: &Generation,
    name: &str,
    options: &RunArguments,
    global: &GlobalArguments,
) -> Option<Running> {
    let mut command = Process::new("node");
    command.arg("--enable-source-maps");
    if let Some(url) = &options.devtools {
        // the runtime's module that hands `runMain` the DevTools layer
        command
            .args(["--import", "@flick-lang/runtime/runner/devtools"])
            .env("FLICK_DEVTOOLS", url);
    }
    command
        .arg(generation.path().join(runner_file(name, false)))
        .args(&options.args);
    match child::spawn(&mut command, global, "application-stderr") {
        Ok(running) => {
            let _ = append_pid(&generation.path().join(".lock"), running.child.id());
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

#[cfg(test)]
mod tests {
    use std::env;
    use std::sync::{Barrier, mpsc};
    use std::thread;

    use crate::commands::clean::clean_generations;

    use super::*;

    #[test]
    fn locks_keep_live_or_unknown_processes() {
        assert!(process_alive(&format!("\n{}\n", process::id())));
        #[cfg(unix)]
        assert!(!process_alive("999999999"));
        for pid in [
            "",
            " \n",
            "0",
            "-1",
            "-999999999",
            "2147483648",
            "1 2",
            "../1",
            "/proc/1",
            "1; exit 1",
        ] {
            assert!(process_alive(pid), "{pid:?}");
        }
    }

    #[test]
    fn concurrent_reservations_survive_clean_before_launch_and_other_cleanup() {
        let root = env::temp_dir().join(format!("flick-reservations-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        let gate = Barrier::new(2);
        thread::scope(|scope| {
            let (reserved, paths) = mpsc::channel();
            let (release, released) = mpsc::channel();
            let root = &root;
            let gate = &gate;
            let first = scope.spawn(move || {
                gate.wait();
                let generation = Generation::reserve(root).expect("first reservation");
                fs::write(generation.path().join("Main.js"), "first").expect("first artifact");
                reserved
                    .send(generation.path().to_path_buf())
                    .expect("path");
                released.recv().expect("release first");
            });
            gate.wait();
            let second = Generation::reserve(root).expect("second reservation");
            fs::write(second.path().join("Main.js"), "second").expect("second artifact");
            let first_path = paths.recv().expect("reserved path");
            assert_ne!(first_path, second.path());
            assert_eq!(clean_generations(root), 2, "construction owns both paths");
            assert!(first_path.join("Main.js").exists());
            assert!(second.path().join("Main.js").exists());
            release.send(()).expect("finish first");
            first.join().expect("first cleanup");
            assert!(!first_path.exists());
            assert_eq!(clean_generations(root), 1);
            assert_eq!(
                fs::read_to_string(second.path().join("Main.js")).expect("surviving artifact"),
                "second"
            );
            let second_path = second.path().to_path_buf();
            drop(second);
            assert!(!second_path.exists());
        });
        fs::remove_dir_all(root).expect("fixture cleanup");
    }
}
