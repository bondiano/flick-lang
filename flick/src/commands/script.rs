//! `flick script`: the scripts of `[scripts]`, shell commands run in order
//! in the directory of their `flick.toml`, with this `flick` first on `PATH`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};
use std::{env, fs};

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::{self, Host, config};
use flick_core::source::SourceMap;

use crate::args::{GlobalArguments, ScriptArguments};
use crate::command::{Outcome, finish};
use crate::report;
use crate::sources::FsHost;

/// The steps of each script of a `flick.toml`, by name, and its directory.
struct Scripts {
    dir: PathBuf,
    scripts: BTreeMap<String, Vec<String>>,
}

pub fn script(options: &ScriptArguments, global: &GlobalArguments) -> ExitCode {
    let mut sources = SourceMap::default();
    let found = match manifests(&mut sources, global) {
        Ok(found) => found,
        Err(diagnostic) => return failed(sources, diagnostic, global),
    };
    let Some(name) = &options.name else {
        return list(&found);
    };
    let Some(owner) = found.iter().find(|m| m.scripts.contains_key(name)) else {
        let message = format!("no script `{name}` in `[scripts]` of this project or its workspace");
        return failed(
            sources,
            Diagnostic::new(codes::P005, Severity::Error, message),
            global,
        );
    };
    match run(owner, name, &options.args, &mut Vec::new(), global) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Step(code)) => code,
        Err(Failure::Cycle(message)) => failed(
            sources,
            Diagnostic::new(codes::P005, Severity::Error, message),
            global,
        ),
    }
}

/// The manifests with scripts from the project's directory upwards, the
/// nearest first.
fn manifests(
    sources: &mut SourceMap,
    global: &GlobalArguments,
) -> Result<Vec<Scripts>, Diagnostic> {
    let host = FsHost::new();
    let start = match &global.project {
        Some(path) => {
            let path = project::normalize(&host.cwd.join(path));
            if path.extension().is_some_and(|e| e == "toml") {
                path.parent().map(Path::to_path_buf).unwrap_or_default()
            } else {
                path
            }
        }
        None => host.cwd.clone(),
    };
    let mut out = Vec::new();
    for dir in start.ancestors() {
        let path = dir.join(project::CONFIG_FILE);
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let file = sources.add(host.display(&path), text.clone());
        let manifest = config::parse_manifest(&text, file)?;
        if !manifest.scripts.is_empty() {
            let scripts = manifest
                .scripts
                .into_iter()
                .map(|s| (s.name, s.steps))
                .collect();
            out.push(Scripts {
                dir: dir.to_path_buf(),
                scripts,
            });
        }
    }
    Ok(out)
}

/// Prints each script with its steps, by manifest.
fn list(found: &[Scripts]) -> ExitCode {
    let mut out = String::new();
    for manifest in found {
        let _ = writeln!(
            out,
            "{}:",
            manifest.dir.join(project::CONFIG_FILE).display()
        );
        for (name, steps) in &manifest.scripts {
            let _ = writeln!(out, "  {name}: {}", steps.join(" && "));
        }
    }
    let _ = io::stdout().lock().write_all(out.as_bytes());
    ExitCode::SUCCESS
}

enum Failure {
    /// A step failed with this exit code.
    Step(ExitCode),
    Cycle(String),
}

/// Runs the steps of script `name`; `running` are the scripts that run it
/// through `@`.
fn run(
    owner: &Scripts,
    name: &str,
    args: &[String],
    running: &mut Vec<String>,
    global: &GlobalArguments,
) -> Result<(), Failure> {
    if running.iter().any(|r| r == name) {
        running.push(name.to_owned());
        return Err(Failure::Cycle(format!(
            "the scripts run each other: {}",
            running
                .iter()
                .map(|r| format!("`{r}`"))
                .collect::<Vec<_>>()
                .join(" → ")
        )));
    }
    running.push(name.to_owned());
    for step in &owner.scripts[name] {
        if let Some(other) = step.strip_prefix('@') {
            run(owner, other, args, running, global)?;
            continue;
        }
        report::log(global, &format!("$ {step}"));
        let status = shell(step, args)
            .current_dir(&owner.dir)
            .env("PATH", search_path())
            .status();
        let code = match status {
            Ok(status) if status.success() => continue,
            Ok(status) => status
                .code()
                .and_then(|c| u8::try_from(c).ok())
                .unwrap_or(1),
            Err(err) => {
                report::log(global, &format!("cannot run `{step}`: {err}"));
                1
            }
        };
        return Err(Failure::Step(ExitCode::from(code.max(1))));
    }
    running.pop();
    Ok(())
}

/// `step` in the shell; `args` are its positional parameters, `"$@"`.
fn shell(step: &str, args: &[String]) -> process::Command {
    if cfg!(windows) {
        let mut command = process::Command::new("cmd");
        command.arg("/C").arg(step).args(args);
        command
    } else {
        let mut command = process::Command::new("sh");
        command.arg("-c").arg(step).arg("flick-script").args(args);
        command
    }
}

/// `PATH` with the directory of this `flick` first.
fn search_path() -> OsString {
    let own = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let rest = env::var_os("PATH").unwrap_or_default();
    env::join_paths(own.into_iter().chain(env::split_paths(&rest))).unwrap_or(rest)
}

fn failed(sources: SourceMap, diagnostic: Diagnostic, global: &GlobalArguments) -> ExitCode {
    finish(
        Outcome {
            sources,
            diagnostics: vec![diagnostic],
        },
        global,
    )
}
