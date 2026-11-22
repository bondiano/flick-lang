//! `flick explain` and `flick doctor`.

use std::fs;
use std::path::Path;
use std::process::Command as Process;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project;
use flick_core::source::SourceMap;

use crate::args::{ExplainArguments, GlobalArguments};
#[cfg(feature = "wasm-backends")]
use crate::backends;
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
            println!("{wanted}: {}\n\n{}", info.title, info.explain.trim_end());
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

/// The output of `program --version`, if it runs.
fn version_of(program: &str) -> Option<String> {
    output_of(program, &["--version"]).map(|v| v.trim_start_matches('v').to_owned())
}

/// The version `git --version` prints, such as `2.43.0`.
fn git_version() -> Option<String> {
    output_of("git", &["--version"]).map(|v| v.trim_start_matches("git version ").to_owned())
}

/// The version `go version` prints, such as `1.23.12`.
fn go_version() -> Option<String> {
    let printed = output_of("go", &["version"])?;
    Some(
        printed
            .split_whitespace()
            .find(|word| word.starts_with("go1"))
            .map_or(printed.clone(), |word| {
                word.trim_start_matches("go").to_owned()
            }),
    )
}

/// Checks the toolchain and the project's installed tree via its backend;
/// optional tools and adapters that are missing are notes, not failures.
pub fn doctor(global: &GlobalArguments) -> Outcome {
    let mut failed = Vec::new();
    let mut line = |ok: Option<bool>, what: &str, detail: String| {
        let mark = match ok {
            Some(true) => "ok  ",
            Some(false) => "FAIL",
            None => "note",
        };
        println!("{mark} {what}: {detail}");
        if ok == Some(false) {
            failed.push(format!("{what}: {detail}"));
        }
    };
    line(Some(true), "compiler", format!("flick {COMPILER}"));
    let host = FsHost::new();
    let explicit = global.project.as_deref().map(Path::new);
    let project = project::discover(&host, &mut SourceMap::default(), &host.cwd, explicit)
        .ok()
        .flatten();
    // a note, not a failure: a checkout with `FLICK_STD_PATH`, path
    // dependencies and a warm package store fetches nothing
    match git_version() {
        Some(v) => line(
            Some(true),
            "git",
            format!("{v} (fetches the implicit `std` and `git` dependencies)"),
        ),
        None => line(
            None,
            "git",
            "not found: `flick install` fetches the implicit `std` from its release tag, and `check`, \
             `build`, `run` and `test` do too when it is not in the package store; set \
             `FLICK_STD_PATH` to a local `packages/std` to work without it"
                .to_owned(),
        ),
    }
    // the go target is optional: a missing toolchain is a note, as for adapters
    match go_version() {
        Some(v) => line(None, "Go", format!("{v} (the `go` target only)")),
        None => line(
            None,
            "Go",
            "not found (the `go` target only: `--target go`)".to_owned(),
        ),
    }
    #[cfg(feature = "wasm-backends")]
    {
        let node = version_of("node");
        let pnpm = version_of("pnpm");
        if let Some(lines) = backends::doctor(project.as_ref(), node.as_deref(), pnpm.as_deref()) {
            for (ok, what, detail) in lines {
                line(ok, &what, detail);
            }
        }
    }
    #[cfg(not(feature = "wasm-backends"))]
    {
        let _ = (project, version_of);
    }
    Outcome {
        sources: SourceMap::default(),
        diagnostics: failed
            .into_iter()
            .map(|f| Diagnostic::new(codes::P015, Severity::Error, f))
            .collect(),
    }
}
