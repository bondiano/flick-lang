//! JS interface to the Flick core for the in-browser demo on the site:
//! checking one module and previewing its compiled JS, without Node or a
//! file system. The built-in modules are compiled into the core, so the
//! module may import them. Built for `wasm32-unknown-unknown` via
//! wasm-bindgen.

use std::io;
use std::path::{Path, PathBuf};

use flick_core::build::{self, Extras, Settings};
use flick_core::diagnostics::render::render_json;
use flick_core::diagnostics::{Diagnostic, Severity};
use flick_core::project::{self, Host};
use flick_core::source::{FileId, SourceMap};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

/// Flick language version this build targets, e.g. `"0.7"`.
#[wasm_bindgen]
pub fn language_version() -> String {
    flick_core::LANGUAGE_VERSION.to_owned()
}

/// The diagnostics of `source` as a JSON array of the events
/// `flick check --diagnostics json` writes.
#[wasm_bindgen]
pub fn check(source: &str) -> String {
    let mut sources = SourceMap::default();
    let diagnostics = project::check(&Demo(source), &mut sources, None, &[path()]);
    diagnostics_json(&sources, &diagnostics).to_string()
}

/// `{ "js": …, "diagnostics": […] }`: the module's JS as `flick build
/// --profile <profile>` writes it, or `null` when it has errors. A profile
/// other than `"release"` is `"debug"`.
#[wasm_bindgen]
pub fn compile(source: &str, profile: &str) -> String {
    let (output, diagnostics) = emit(source, profile);
    let js = output.map_or(Value::Null, |(file, artifacts)| {
        artifacts
            .into_iter()
            .find(|a| a.path == file)
            .map_or(Value::Null, |a| Value::String(a.contents))
    });
    json!({ "js": js, "diagnostics": diagnostics }).to_string()
}

/// `{ "files": { path: contents } | null, "entry": path | null,
/// "diagnostics": […] }`: everything `flick build` writes for the module —
/// its JS, the built-in modules and, when it exports `main`, the runner
/// `flick-main.js`, which is the entry.
#[wasm_bindgen]
pub fn build(source: &str, profile: &str) -> String {
    let (output, diagnostics) = emit(source, profile);
    let (files, entry) = match output {
        Some((_, artifacts)) => {
            let entry = artifacts
                .iter()
                .any(|a| a.path == build::runner_path(false))
                .then(|| build::runner_path(false).to_owned());
            let files: serde_json::Map<String, Value> = artifacts
                .into_iter()
                .filter(|a| Path::new(&a.path).extension().is_some_and(|e| e == "js"))
                .map(|a| (a.path, Value::String(a.contents)))
                .collect();
            (
                Value::Object(files),
                entry.map_or(Value::Null, Value::String),
            )
        }
        None => (Value::Null, Value::Null),
    };
    json!({ "files": files, "entry": entry, "diagnostics": diagnostics }).to_string()
}

/// The build of the demo's module, with the path of its JS, and the
/// diagnostics as JSON; no build when there are errors.
fn emit(source: &str, profile: &str) -> (Option<(String, Vec<build::Artifact>)>, Value) {
    let mut sources = SourceMap::default();
    let (mut diagnostics, compilation) =
        project::compile(&Demo(source), &mut sources, None, &[path()]);
    let mut output = None;
    if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
        let module = compilation.modules.iter().find(|m| m.path == path());
        let file = module.map(|m| format!("{}.js", m.name.replace('.', "/")));
        // like `flick build` without a project: a module with `main` gets the runner
        let entry = module
            .filter(|m| m.interface.exports_value("main"))
            .map(|m| m.name.clone());
        let source_path = |_: FileId| format!("file://{}", path().display());
        let source_dir = |_: &str| path().parent().map(Path::to_path_buf);
        let settings = Settings {
            package: None,
            entry: entry.as_deref(),
            typescript: false,
            release: profile == "release",
            extras: Extras::default(),
            source_path: &source_path,
            source_dir: &source_dir,
            out_dir: Path::new("/dist"),
            root: None,
        };
        let (artifacts, problems) = build::emit(&compilation, &sources, &settings);
        let failed = problems.iter().any(|d| d.severity == Severity::Error);
        diagnostics.extend(problems);
        if let (Some(file), false) = (file, failed) {
            output = Some((file, artifacts));
        }
    }
    (output, diagnostics_json(&sources, &diagnostics))
}

/// The one file of the demo.
fn path() -> PathBuf {
    PathBuf::from("/Main.fl")
}

fn diagnostics_json(sources: &SourceMap, diagnostics: &[Diagnostic]) -> Value {
    render_json(sources, diagnostics)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect::<Vec<_>>()
        .into()
}

/// A file system holding only the demo's module.
struct Demo<'a>(&'a str);

impl Host for Demo<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == self::path() {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix("/").unwrap_or(path).display().to_string()
    }
}
