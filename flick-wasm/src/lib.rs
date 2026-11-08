//! JS interface to the Flick core for the playground on the site: one module,
//! analysed once for each change, its diagnostics, its build, and what an
//! editor asks of it, without Node or a file system. The built-in modules are
//! compiled into the core, so the module may import them. Built for
//! `wasm32-unknown-unknown` via wasm-bindgen.
//!
//! The core counts in UTF-8 bytes and an editor in UTF-16 units, so every
//! position crosses this boundary as a UTF-16 offset.

use std::io;
use std::path::{Path, PathBuf};

use flick_core::build::{self, Extras, Settings};
use flick_core::diagnostics::render::json_events;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::fmt;
use flick_core::ide::{CompletionKind, Ide};
use flick_core::project::config::Backend;
use flick_core::project::{self, Analysis, Host};
use flick_core::source::{FileId, SourceMap};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

/// Version of the compiler in this build, e.g. `"0.1.0"`.
#[wasm_bindgen]
pub fn compiler_version() -> String {
    flick_core::VERSION.to_owned()
}

/// The module being edited and its last analysis.
#[wasm_bindgen]
pub struct Session {
    text: String,
    sources: SourceMap,
    analysis: Analysis,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Session {
        let mut sources = SourceMap::default();
        let analysis = project::analyze(&Demo(""), &mut sources, None, &[path()]);
        Session {
            text: String::new(),
            sources,
            analysis,
        }
    }

    /// Analyses `source`, unless it is the text analysed last, and gives its
    /// diagnostics as a JSON array of the events `flick check --diagnostics
    /// json` writes, positions in UTF-16 (see [`positions`]).
    pub fn update(&mut self, source: &str) -> String {
        if source != self.text {
            source.clone_into(&mut self.text);
            self.sources = SourceMap::default();
            self.analysis = project::analyze(&Demo(source), &mut self.sources, None, &[path()]);
        }
        self.diagnostics(&self.analysis.diagnostics).to_string()
    }

    /// `{ "js", "dts", "files", "entry", "diagnostics", "dts_diagnostics" }`
    /// for the last analysed module as `flick build --profile <profile>
    /// --emit-dts` writes it: its JS and `.d.ts`, every JS file with the
    /// built-in modules and, when it exports `main`, the runner
    /// `flick-main.js`, which is the entry. All but the diagnostics are
    /// `null` when there are errors. What the `.d.ts` leaves out, `FLICK-F004`,
    /// is apart: it says nothing about the program. A profile other than
    /// `"release"` is `"debug"`.
    pub fn emit(&self, profile: &str) -> String {
        let mut diagnostics = self.analysis.diagnostics.clone();
        let mut declarations = Vec::new();
        let mut out = json!({ "js": null, "dts": null, "files": null, "entry": null });
        if !has_errors(&diagnostics) {
            let (artifacts, problems) = self.artifacts(profile);
            let (left_out, problems): (Vec<_>, Vec<_>) =
                problems.into_iter().partition(|d| d.code == codes::F004);
            declarations = left_out;
            diagnostics.extend(problems);
            if !has_errors(&diagnostics) {
                out = self.output(artifacts);
            }
        }
        out["diagnostics"] = self.diagnostics(&diagnostics);
        out["dts_diagnostics"] = self.diagnostics(&declarations);
        out.to_string()
    }

    /// `{ "text", "from", "to" }`: what hovering at the UTF-16 offset `at`
    /// shows, as Markdown, and the range it is about; `null` for nothing.
    pub fn hover(&self, at: u32) -> String {
        let Some(file) = self.file() else {
            return Value::Null.to_string();
        };
        let ide = Ide {
            analysis: &self.analysis,
            sources: &self.sources,
        };
        ide.hover(file, byte_offset(&self.text, at))
            .map_or(Value::Null, |(text, (start, end))| {
                json!({
                    "text": text,
                    "from": utf16_offset(&self.text, start),
                    "to": utf16_offset(&self.text, end),
                })
            })
            .to_string()
    }

    /// A JSON array of `{ "label", "kind", "detail" }`: what may stand at the
    /// UTF-16 offset `at`; `dotted` when a `.` was just typed.
    pub fn complete(&self, at: u32, dotted: bool) -> String {
        let Some(file) = self.file() else {
            return Value::Array(Vec::new()).to_string();
        };
        let ide = Ide {
            analysis: &self.analysis,
            sources: &self.sources,
        };
        let items: Vec<Value> = ide
            .complete(file, byte_offset(&self.text, at), dotted)
            .into_iter()
            .map(|c| json!({ "label": c.label, "kind": kind(c.kind), "detail": c.detail }))
            .collect();
        Value::Array(items).to_string()
    }
}

impl Session {
    fn file(&self) -> Option<FileId> {
        self.analysis
            .modules
            .iter()
            .find(|m| m.path == path())
            .map(|m| m.file)
    }

    fn artifacts(&self, profile: &str) -> (Vec<build::Artifact>, Vec<Diagnostic>) {
        let compilation = &self.analysis.compilation;
        let module = compilation.modules.iter().find(|m| m.path == path());
        // like `flick build` without a project: a module with `main` gets the runner
        let entries: Vec<(String, String)> = module
            .filter(|m| m.interface.exports_value("main"))
            .map(|m| ("main".to_owned(), m.name.clone()))
            .into_iter()
            .collect();
        let source_path = |_: FileId| format!("file://{}", path().display());
        let source_dir = |_: &str| path().parent().map(Path::to_path_buf);
        let settings = Settings {
            package: None,
            entries: &entries,
            typescript: false,
            release: profile == "release",
            extras: Extras {
                dts: true,
                ..Extras::default()
            },
            source_path: &source_path,
            source_dir: &source_dir,
            out_dir: Path::new("/dist"),
            root: None,
            backend: Backend::Effect,
            ui: None,
            runners: &[],
            core_modules: &[],
            core_only: false,
        };
        build::emit(compilation, &self.sources, &settings)
    }

    fn output(&self, artifacts: Vec<build::Artifact>) -> Value {
        let stem = (self.analysis.compilation.modules.iter())
            .find(|m| m.path == path())
            .map(|m| m.name.replace('.', "/"));
        let contents = |path: &str| {
            (artifacts.iter())
                .find(|a| a.path == path)
                .map_or(Value::Null, |a| Value::String(a.contents.clone()))
        };
        let (js, dts) = stem.map_or((Value::Null, Value::Null), |stem| {
            (
                contents(&format!("{stem}.js")),
                contents(&format!("{stem}.d.ts")),
            )
        });
        let runner = build::runner_path(false);
        let entry = (artifacts.iter())
            .any(|a| a.path == runner)
            .then(|| Value::String(runner.to_owned()));
        let files: serde_json::Map<String, Value> = artifacts
            .into_iter()
            .filter(|a| Path::new(&a.path).extension().is_some_and(|e| e == "js"))
            .map(|a| (a.path, Value::String(a.contents)))
            .collect();
        json!({
            "js": js,
            "dts": dts,
            "files": files,
            "entry": entry.unwrap_or(Value::Null),
        })
    }

    fn diagnostics(&self, diagnostics: &[Diagnostic]) -> Value {
        let mut events =
            serde_json::to_value(json_events(&self.sources, diagnostics).collect::<Vec<_>>())
                .expect("diagnostic events serialize");
        positions(&self.text, &mut events);
        events
    }
}

/// `{ "text" }`, the module `source` formatted, or `{ "diagnostics" }` when it
/// does not parse.
#[wasm_bindgen]
pub fn format(source: &str) -> String {
    let mut sources = SourceMap::default();
    let file = sources.add(Demo("").display(&path()), source);
    match fmt::format(source, file) {
        Ok(text) => json!({ "text": text }),
        Err(diagnostics) => {
            let mut events =
                serde_json::to_value(json_events(&sources, &diagnostics).collect::<Vec<_>>())
                    .expect("diagnostic events serialize");
            positions(source, &mut events);
            json!({ "diagnostics": events })
        }
    }
    .to_string()
}

/// What a diagnostic code means and what to do about it, as Markdown; empty
/// for a code the compiler does not have.
#[wasm_bindgen]
pub fn explain(code: &str) -> String {
    codes::lookup(code).map_or_else(String::new, |info| info.explain.to_owned())
}

fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}

fn kind(kind: CompletionKind) -> &'static str {
    match kind {
        CompletionKind::Local => "local",
        CompletionKind::Value => "value",
        CompletionKind::Constructor => "constructor",
        CompletionKind::Type => "type",
        CompletionKind::Module => "module",
        CompletionKind::Field => "field",
        CompletionKind::Operation => "operation",
        CompletionKind::Keyword => "keyword",
    }
}

/// Rewrites each UTF-8 `range` in the module `text` of the diagnostic events
/// `events`, theirs, their related notes' and their fixes' edits, to UTF-16
/// offsets `from` and `to`; an event of the module also gets `at`, its
/// `line:column` from 1. Ranges in the built-in modules stay as they are.
pub fn positions(text: &str, events: &mut Value) {
    let Some(events) = events.as_array_mut() else {
        return;
    };
    for event in events {
        let start = convert(text, event);
        if let Some(start) = start {
            event["at"] = Value::String(line_column(text, start));
        }
        for key in ["related", "fixes"] {
            for item in event[key].as_array_mut().into_iter().flatten() {
                convert(text, item);
                for edit in item["edits"].as_array_mut().into_iter().flatten() {
                    convert(text, edit);
                }
            }
        }
    }
}

/// [`positions`] for one object with `path` and `range`; the UTF-8 start of
/// its range when it is in the module.
fn convert(text: &str, located: &mut Value) -> Option<u32> {
    if located["path"].as_str() != Some(DISPLAYED) {
        return None;
    }
    let range = located.get("range")?;
    let start = u32::try_from(range["start"].as_u64()?).ok()?;
    let end = u32::try_from(range["end"].as_u64()?).ok()?;
    located["from"] = json!(utf16_offset(text, start));
    located["to"] = json!(utf16_offset(text, end));
    Some(start)
}

/// The UTF-16 offset of the UTF-8 offset `byte` of `text`.
#[must_use]
pub fn utf16_offset(text: &str, byte: u32) -> u32 {
    let byte = floor_boundary(text, byte as usize);
    u32::try_from(text[..byte].encode_utf16().count()).unwrap_or(u32::MAX)
}

/// The UTF-8 offset of the UTF-16 offset `unit` of `text`; inside a
/// surrogate pair, the start of its character.
#[must_use]
pub fn byte_offset(text: &str, unit: u32) -> u32 {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        units += c.len_utf16();
        if units > unit as usize {
            return u32::try_from(i).unwrap_or(u32::MAX);
        }
    }
    u32::try_from(text.len()).unwrap_or(u32::MAX)
}

fn floor_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// `line:column` from 1 of the UTF-8 offset `byte`, the column in UTF-16.
fn line_column(text: &str, byte: u32) -> String {
    let before = &text[..floor_boundary(text, byte as usize)];
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .encode_utf16()
        .count()
        + 1;
    format!("{line}:{column}")
}

/// The one file of the playground, and how diagnostics name it.
fn path() -> PathBuf {
    PathBuf::from("/Main.fl")
}

const DISPLAYED: &str = "Main.fl";

/// A file system holding only the playground's module.
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
