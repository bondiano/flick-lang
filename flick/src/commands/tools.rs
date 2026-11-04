//! `flick explain` and `flick doctor`.

use std::fs;
use std::path::Path;
use std::process::Command as Process;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project;
use flick_core::source::SourceMap;

use crate::args::{ExplainArguments, GlobalArguments};
use crate::command::Outcome;
use crate::sources::FsHost;
use crate::stdlib;
use crate::toolchain::{self, COMPILER, EFFECT, NODE, PNPM, RUNTIME};

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

/// The output of `program --version`, if it runs.
fn version_of(program: &str) -> Option<String> {
    let out = Process::new(program).arg("--version").output().ok()?;
    out.status.success().then(|| {
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .trim_start_matches('v')
            .to_owned()
    })
}

fn at_least(found: &str, wanted: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parse(found) >= parse(wanted)
}

/// Checks the toolchain and the project's installed tree; optional
/// adapters that are missing are notes, not failures.
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
    match version_of("node") {
        Some(v) => line(
            Some(at_least(&v, NODE)),
            "Node.js",
            format!("{v} (needs >= {NODE})"),
        ),
        None => line(
            Some(false),
            "Node.js",
            format!("not found (needs >= {NODE})"),
        ),
    }
    let host = FsHost::new();
    let explicit = global.project.as_deref().map(Path::new);
    let found = project::discover(&host, &mut SourceMap::default(), &host.cwd, explicit);
    let root = found.ok().flatten().map(|p| p.root);
    let pinned = root
        .as_ref()
        .and_then(|r| fs::read_to_string(r.join("package.json")).ok())
        .and_then(|t| project::pnpm::pinned(&t))
        .unwrap_or_else(|| PNPM.to_owned());
    match version_of("pnpm") {
        Some(v) => line(
            Some(v == pinned),
            "pnpm",
            format!("{v} (the project pins {pinned})"),
        ),
        None => line(
            Some(false),
            "pnpm",
            format!("not found (the project pins {pinned})"),
        ),
    }
    installed(root.as_deref(), &mut line);
    // the mapper is verified against one TypeScript release
    let mapper = root
        .as_deref()
        .and_then(|r| toolchain::installed(r, "typescript"));
    let detail = match mapper {
        Some(v) if v == toolchain::TYPESCRIPT_GO => {
            format!("typescript {v}, the mapper's release")
        }
        Some(v) => format!(
            "typescript {v}: the mapper is verified against {}",
            toolchain::TYPESCRIPT_GO
        ),
        None => "typescript not installed: editors use `flick lsp` only".to_owned(),
    };
    line(None, "Content Mapper", detail);
    Outcome {
        sources: SourceMap::default(),
        diagnostics: failed
            .into_iter()
            .map(|f| Diagnostic::new(codes::P015, Severity::Error, f))
            .collect(),
    }
}

/// The runtime, Effect and the optional adapters installed in `root`.
fn installed(root: Option<&Path>, line: &mut impl FnMut(Option<bool>, &str, String)) {
    match root {
        Some(root) => {
            for (package, wanted) in [("@flick-lang/runtime", RUNTIME), ("effect", EFFECT)] {
                let base = root.join("node_modules/@flick-lang/runtime");
                let found = toolchain::installed(root, package)
                    .or_else(|| toolchain::installed(&base, package));
                match found {
                    Some(v) => line(Some(v == wanted), package, format!("{v} (needs {wanted})")),
                    None => line(
                        None,
                        package,
                        format!("not installed; `flick install` (needs {wanted})"),
                    ),
                }
            }
            for (package, what) in [
                ("pg", "PostgreSQL driver of `Database.pg`"),
                ("react", "React adapter of `Ui.Dom.mount`"),
            ] {
                let found = toolchain::installed(root, package).or_else(|| {
                    toolchain::installed(&root.join("node_modules/@flick-lang/runtime"), package)
                });
                line(
                    None,
                    package,
                    found.map_or_else(
                        || format!("not installed: the {what} is unavailable"),
                        |v| format!("{v}, the {what}"),
                    ),
                );
            }
        }
        None => line(
            None,
            "project",
            "none here: runtime and adapters not checked".to_owned(),
        ),
    }
}
