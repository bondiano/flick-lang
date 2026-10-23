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
    let mut sources = SourceMap::default();
    let (mut diagnostics, compilation) =
        project::compile(&Demo(source), &mut sources, None, &[path()]);
    let mut js = Value::Null;
    if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
        let module = compilation.modules.iter().find(|m| m.path == path());
        let file = module.map(|m| format!("{}.js", m.name.replace('.', "/")));
        let source_path = |_: FileId| format!("file://{}", path().display());
        let source_dir = |_: &str| path().parent().map(Path::to_path_buf);
        let settings = Settings {
            package: None,
            entry: None,
            typescript: false,
            release: profile == "release",
            extras: Extras::default(),
            source_path: &source_path,
            source_dir: &source_dir,
            out_dir: Path::new("/dist"),
            root: None,
        };
        let (artifacts, problems) = build::emit(&compilation, &sources, &settings);
        diagnostics.extend(problems);
        if let Some(artifact) = artifacts.iter().find(|a| Some(&a.path) == file.as_ref()) {
            js = Value::String(artifact.contents.clone());
        }
    }
    json!({ "js": js, "diagnostics": diagnostics_json(&sources, &diagnostics) }).to_string()
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
