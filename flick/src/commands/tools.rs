//! `flick explain` and `flick doctor`.

use std::fs;
use std::path::Path;
use std::process::Command as Process;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::{self, store};
use flick_core::source::SourceMap;

#[cfg(feature = "wasm-backends")]
use super::contributed;
use super::plugins;
use crate::args::{DoctorArguments, ExplainArguments, GlobalArguments};
#[cfg(feature = "wasm-backends")]
use crate::backends::Selected;
use crate::command::Outcome;
use crate::sources::FsHost;
use crate::stdlib;
use crate::toolchain::COMPILER;

pub fn explain(options: &ExplainArguments) -> Outcome {
    if let Some((package, code)) = options.code.trim().split_once('/') {
        return explain_plugin(package, code, &options.code);
    }
    let wanted = options.code.trim().to_uppercase();
    let wanted = if wanted.starts_with("FLICK-") {
        wanted
    } else {
        format!("FLICK-{wanted}")
    };
    match codes::lookup(&wanted) {
        Some(info) => {
            println!(
                "{}: {}\n\n{}",
                info.code,
                info.title,
                info.explain.trim_end()
            );
            Outcome::default()
        }
        None => Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![Diagnostic::new(
                codes::P018,
                Severity::Error,
                format!("`{}` is not a diagnostic code of Flick", options.code),
            )],
        },
    }
}

/// `flick explain <package>/<code>`: the text of a code of a package's
/// plugin, in its `explain/<code>.md`.
fn explain_plugin(package: &str, code: &str, written: &str) -> Outcome {
    let code = code.to_uppercase();
    let unknown = |why: String| Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(
            codes::P018,
            Severity::Error,
            format!("`{written}` is not a diagnostic code of a plugin: {why}"),
        )],
    };
    // the code names a file: only the shape a plugin's codes have
    if codes::plugin(package, &code).is_none() {
        return unknown("a code is capital letters and three digits".to_owned());
    }
    let dir = match stdlib::package_dir(package, None) {
        Ok(dir) => dir,
        Err(why) => return unknown(why),
    };
    match fs::read_to_string(dir.join("explain").join(format!("{code}.md"))) {
        Ok(text) => {
            println!("{package}/{code}\n\n{}", text.trim_end());
            Outcome::default()
        }
        Err(_) => unknown(format!("`{package}` explains no `{code}`")),
    }
}

/// The trimmed stdout of `program args`, if it runs and succeeds.
fn output_of(program: &str, args: &[&str]) -> Option<String> {
    let out = Process::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The version `git --version` prints, such as `2.43.0`.
fn git_version() -> Option<String> {
    output_of("git", &["--version"]).map(|v| v.trim_start_matches("git version ").to_owned())
}

#[derive(Default)]
struct DoctorReport {
    diagnostics: Vec<Diagnostic>,
}

impl DoctorReport {
    fn line(&mut self, ok: Option<bool>, what: &str, detail: &str) {
        let mark = match ok {
            Some(true) => "ok  ",
            Some(false) => "FAIL",
            None => "note",
        };
        println!("{mark} {what}: {detail}");
        if ok == Some(false) {
            self.diagnostics.push(Diagnostic::new(
                codes::P015,
                Severity::Error,
                format!("{what}: {detail}"),
            ));
        }
    }

    #[cfg(feature = "wasm-backends")]
    fn fail_with(&mut self, what: &str, diagnostic: Diagnostic) {
        println!("FAIL {what}: {}", diagnostic.message);
        self.diagnostics.push(diagnostic);
    }
}

/// Checks core compiler prerequisites (compiler, store, git, plugins) and then
/// delegates target-specific toolchain and package checks to the selected backend.
pub fn doctor(options: &DoctorArguments, global: &GlobalArguments) -> Outcome {
    let mut report = DoctorReport::default();
    report.line(Some(true), "compiler", &format!("flick {COMPILER}"));
    match store::store_dir() {
        Some(dir) => report.line(Some(true), "store", &dir.display().to_string()),
        None => report.line(
            None,
            "store",
            "unavailable (set `FLICK_STORE_DIR` or `HOME`)",
        ),
    }
    let host = FsHost::new();
    let explicit = global.project.as_deref().map(Path::new);
    let project = project::discover(&host, &mut SourceMap::default(), &host.cwd, explicit)
        .ok()
        .flatten();
    // a note, not a failure: a checkout with `FLICK_STD_PATH`, path
    // dependencies and a warm package store fetches nothing
    match git_version() {
        Some(v) => report.line(
            Some(true),
            "git",
            &format!("{v} (fetches the implicit `std` and `git` dependencies)"),
        ),
        None => report.line(
            None,
            "git",
            "not found: `flick install` fetches the implicit `std` from its release tag, and `check`, \
             `build`, `run` and `test` do too when it is not in the package store; set \
             `FLICK_STD_PATH` to a local `packages/std` to work without it",
        ),
    }
    if let Some(p) = &project {
        for (ok, what, detail) in plugins::doctor_lines(p) {
            report.line(ok, &what, &detail);
        }
    } else {
        report.line(None, "plugins", "no project here");
    }
    doctor_backend(options, &host, project.as_ref(), &mut report);
    Outcome {
        sources: SourceMap::default(),
        diagnostics: report.diagnostics,
    }
}

fn doctor_backend(
    options: &DoctorArguments,
    host: &FsHost,
    project: Option<&project::Project>,
    report: &mut DoctorReport,
) {
    #[cfg(feature = "wasm-backends")]
    {
        if project.is_some() || options.target.is_some() {
            match Selected::load_target_local(host, project, options.target.as_deref()) {
                Ok((package, _)) => {
                    let pin = package.pin();
                    report.line(
                        Some(true),
                        "backend",
                        &format!("{}@{}", pin.identity.id, pin.identity.version),
                    );
                    match contributed::doctor(&package, project, &host.cwd) {
                        Ok(Some(lines)) => {
                            for (ok, what, detail) in lines {
                                report.line(ok, &what, &detail);
                            }
                        }
                        Ok(None) => report.line(
                            None,
                            "backend checks",
                            &format!("`{}` declares no doctor checks", pin.identity.id),
                        ),
                        Err(diag) => report.fail_with("backend checks", diag),
                    }
                    if project.is_some() {
                        for (ok, what, detail) in package.doctor_packages() {
                            report.line(ok, &what, &detail);
                        }
                    }
                }
                Err(diag) => report.fail_with("backend", diag),
            }
        } else {
            report.line(
                None,
                "backend",
                "no project here; pass `--target <name>` to inspect a backend",
            );
        }
    }
    #[cfg(not(feature = "wasm-backends"))]
    {
        let _ = (options, host, project);
        report.line(
            None,
            "backend",
            "this build has no WebAssembly backend support",
        );
    }
}
