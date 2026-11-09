//! `install`, `add`, `remove`, `update` and `deps`. `install` and `update`
//! fetch the Flick dependencies `flick.lock` pins first; the npm packages of
//! `package.json` go through pnpm, and every command that writes checks
//! first that the pnpm on `PATH` is the version `package.json` pins, so a
//! wrong pnpm changes nothing.

use std::fs;
use std::path::Path;
use std::process::Command as Process;

use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::project::pnpm::{self, Command};
use flick_core::project::{self, js};
use flick_core::source::SourceMap;

use super::libraries;
use super::run::find_project;
use crate::args::GlobalArguments;
use crate::child;
use crate::command::Outcome;
use crate::fetch;
use crate::sources::FsHost;
use crate::stdlib;
use crate::toolchain;
use flick_core::project::Project;
use tracing::info;

fn failure(code: Code, message: String) -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
    }
}

/// The pnpm version the project pins, else the one this compiler pins.
fn pinned(root: &Path) -> String {
    fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|t| pnpm::pinned(&t))
        .unwrap_or_else(|| toolchain::PNPM.to_owned())
}

// Flick owns the version gate; even `pnpm --version` must not bootstrap or pin pnpm.
fn pnpm_process(root: &Path) -> Process {
    let mut process = Process::new("pnpm");
    process
        .current_dir(root)
        .env("npm_config_manage_package_manager_versions", "false")
        .env("PNPM_CONFIG_PM_ON_FAIL", "ignore")
        .env("COREPACK_ENABLE_PROJECT_SPEC", "0")
        .env("COREPACK_ENABLE_AUTO_PIN", "0")
        .env("COREPACK_ENABLE_NETWORK", "0")
        .env("COREPACK_DEFAULT_TO_LATEST", "0");
    process
}

/// `P016` unless `pnpm --version` is `wanted`, with how to get it: the
/// project's own `packageManager` pin wins, and only that version writes.
fn check_pnpm(root: &Path, wanted: &str) -> Option<Outcome> {
    let found = pnpm_process(root)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let instruction = format!(
        "run `corepack prepare pnpm@{wanted} --activate`, or `npm install --global pnpm@{wanted}`"
    );
    let next = "nothing was changed (`flick doctor` lists the toolchain versions)";
    match found {
        Some(v) if v == wanted => None,
        Some(v) => Some(failure(
            codes::P016,
            format!(
                "the project pins pnpm {wanted} in `packageManager`, but `pnpm` is {v}: {instruction}; {next}"
            ),
        )),
        None => Some(failure(
            codes::P016,
            format!(
                "`pnpm` is not on PATH and the project pins pnpm {wanted} in `packageManager`: {instruction}; {next}"
            ),
        )),
    }
}

fn done() -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: Vec::new(),
    }
}

/// Fetches the Flick dependencies for `install` and `update`; the failure.
fn fetch(project: &Project, command: &Command) -> Option<Outcome> {
    let (update, options) = match command {
        Command::Install { locked, offline } => (
            Vec::new(),
            fetch::Options {
                locked: *locked,
                offline: *offline,
            },
        ),
        Command::Update { packages } => (packages.clone(), fetch::Options::default()),
        _ => return None,
    };
    let dependencies = stdlib::with_implicit(&project.config.dependencies);
    match fetch::install(&project.root, &dependencies, &update, options) {
        Ok(fetched) => {
            for name in fetched {
                info!("fetched {name}");
            }
            None
        }
        Err(message) => Some(failure(codes::P034, message)),
    }
}

/// For `install` and `update`, adds what the dependency closure wants of npm
/// to `package.json`, which pnpm then installs; the failure.
fn add_js_dependencies(command: &Command, global: &GlobalArguments) -> Option<Outcome> {
    let locked = match command {
        Command::Install { locked, .. } => *locked,
        Command::Update { .. } => false,
        _ => return None,
    };
    if let Err(message) = stdlib::prepare(global.project.as_deref()) {
        return Some(failure(codes::P034, message));
    }
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let explicit = global.project.as_deref().map(Path::new);
    let project = project::discover(&host, &mut sources, &host.cwd, explicit)
        .ok()
        .flatten()?;
    let mut diagnostics = Vec::new();
    let wanted = js::wanted(&host, &mut sources, &project, &mut diagnostics);
    diagnostics.extend(js::conflicts(&host, &project.root, &wanted));
    if !diagnostics.is_empty() {
        return Some(Outcome {
            sources,
            diagnostics,
        });
    }
    let path = project.root.join("package.json");
    let text = fs::read_to_string(&path).ok();
    match js::merged_manifest(text.as_deref(), &wanted) {
        Ok(None) => None,
        Ok(Some(_)) if locked => Some(failure(
            codes::P034,
            "`package.json` lacks npm packages that `[js.dependencies]` name: run `flick install` without `--locked`".to_owned(),
        )),
        Ok(Some(merged)) => fs::write(&path, merged).err().map(|e| {
            failure(codes::P013, format!("cannot write `{}`: {e}", path.display()))
        }),
        Err(message) => Some(failure(codes::P005, message)),
    }
}

pub fn deps(command: &Command, global: &GlobalArguments) -> Outcome {
    let project = match find_project(global) {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            return Outcome {
                sources,
                diagnostics,
            };
        }
    };
    let root = &project.root;
    if let Some(failed) = fetch(&project, command).or_else(|| add_js_dependencies(command, global))
    {
        return failed;
    }
    let npm_remove;
    let command = match command {
        Command::Remove { packages } => match libraries::remove(&project, packages) {
            Ok(rest) if rest.is_empty() => return done(),
            Ok(rest) => {
                npm_remove = Command::Remove { packages: rest };
                &npm_remove
            }
            Err(failed) => return failed,
        },
        Command::List => {
            libraries::print_locked(&project);
            command
        }
        command => command,
    };
    // a project without npm packages needs no pnpm
    if !root.join("package.json").exists()
        && matches!(
            command,
            Command::Install { .. } | Command::Update { .. } | Command::List
        )
    {
        return done();
    }
    if pnpm::writes(command) {
        if let Some(problem) = check_pnpm(root, &pinned(root)) {
            return problem;
        }
        if matches!(command, Command::Install { locked: true, .. })
            && !root.join("pnpm-lock.yaml").exists()
        {
            return failure(
                codes::P017,
                "`install --locked` needs `pnpm-lock.yaml`; run `flick install` to create it"
                    .to_owned(),
            );
        }
    }
    let mut process = pnpm_process(root);
    process.args(pnpm::args(command));
    let status = if pnpm::writes(command) {
        child::spawn_tool(&mut process, global, "package-manager-output")
    } else {
        child::spawn(&mut process, global, "package-manager-output")
    }
    .and_then(|running| running.wait(&child::signals()));
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            return failure(
                codes::P017,
                format!("pnpm failed with exit code {}", child::code(s)),
            );
        }
        Err(err) => return failure(codes::P016, format!("cannot run pnpm: {err}")),
    }
    let diagnostics = if pnpm::writes(command) {
        toolchain::check_runtime(root, false).into_iter().collect()
    } else {
        Vec::new()
    };
    Outcome {
        sources: SourceMap::default(),
        diagnostics,
    }
}
