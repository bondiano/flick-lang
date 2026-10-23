//! `install`, `add`, `remove`, `update` and `deps` through pnpm. Every
//! command that writes checks first that the pnpm on `PATH` is the version
//! `package.json` pins, so a wrong pnpm changes nothing. Flick keeps no
//! lockfile of its own.

use std::fs;
use std::path::Path;
use std::process::Command as Process;

use flick_core::diagnostics::{Code, Diagnostic, Severity, codes};
use flick_core::project::pnpm::{self, Command};
use flick_core::source::SourceMap;

use super::run::find_project;
use crate::args::GlobalArguments;
use crate::cache::files_under;
use crate::child;
use crate::command::Outcome;
use crate::toolchain;

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

/// `P016` unless `pnpm --version` is `wanted`.
fn check_pnpm(wanted: &str) -> Option<Outcome> {
    let found = Process::new("pnpm")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    let instruction = format!("install it with `corepack prepare pnpm@{wanted} --activate`");
    match found {
        Some(v) if v == wanted => None,
        Some(v) => Some(failure(
            codes::P016,
            format!(
                "the project pins pnpm {wanted}, but `pnpm` is {v}: {instruction}; nothing was changed"
            ),
        )),
        None => Some(failure(
            codes::P016,
            format!("pnpm {wanted} is not available: {instruction}; nothing was changed"),
        )),
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
    if pnpm::writes(command) {
        if let Some(problem) = check_pnpm(&pinned(root)) {
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
    let mut process = Process::new("pnpm");
    process.args(pnpm::args(command)).current_dir(root);
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
    if matches!(command, Command::List) {
        interfaces(root);
    }
    let diagnostics = if pnpm::writes(command) {
        toolchain::check_runtime(root).into_iter().collect()
    } else {
        Vec::new()
    };
    Outcome {
        sources: SourceMap::default(),
        diagnostics,
    }
}

/// The Flick interfaces each direct dependency provides.
fn interfaces(root: &Path) {
    let Ok(text) = fs::read_to_string(root.join("package.json")) else {
        return;
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    println!("\nFlick interfaces:");
    for field in ["dependencies", "devDependencies"] {
        let Some(deps) = manifest.get(field).and_then(|d| d.as_object()) else {
            continue;
        };
        for name in deps.keys() {
            let dir = root.join("node_modules").join(name);
            let package: Option<serde_json::Value> = fs::read_to_string(dir.join("package.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok());
            let iface = package.as_ref().and_then(|p| {
                p.get("flick")?
                    .get("iface-dir")?
                    .as_str()
                    .map(str::to_owned)
            });
            let line = match (&package, iface) {
                (None, _) => "not installed".to_owned(),
                (Some(_), None) => "JavaScript only: bind it with `flick ffi import`".to_owned(),
                (Some(_), Some(dir_name)) => {
                    let mut files = Vec::new();
                    files_under(&dir.join(&dir_name), &mut files);
                    let modules: Vec<String> = files
                        .iter()
                        .filter(|f| f.extension().is_some_and(|e| e == "iface"))
                        .filter_map(|f| f.strip_prefix(dir.join(&dir_name)).ok())
                        .map(|f| f.with_extension("").to_string_lossy().replace('/', "."))
                        .collect();
                    format!("Flick modules {}", modules.join(", "))
                }
            };
            println!("  {name}: {line}");
        }
    }
}
