//! JS interface to the Flick core for the playground on the site: one module,
//! analysed once for each change, its diagnostics, its component build request
//! and result validation, and what an editor asks of it, without Node or a file
//! system. The built-in modules are compiled into the core, so the module may
//! import them. Built for `wasm32-unknown-unknown` via wasm-bindgen.
//!
//! Code generation is decoupled from this module on `wasm32`: `prepare_emit`
//! emits a verified `flick:backend@0.1.0` `BuildRequest` and content-addressed
//! inputs for the pinned official `flick/js` component running in an isolated
//! browser worker, and `finish_emit` validates the returned `BuildResult` and
//! maps diagnostics back to UTF-16 editor positions.
//!
//! The core counts in UTF-8 bytes and an editor in UTF-16 units, so every
//! position crosses this boundary as a UTF-16 offset.

use std::cell::{RefCell, RefMut};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use flick_core::build;
use flick_core::diagnostics::render::json_events;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::fmt;
use flick_core::ide::{CompletionKind, Ide};
use flick_core::project::config::Config;
use flick_core::project::{self, Analysis, Host, ModuleCache, Project};
use flick_core::source::{FileId, SourceMap};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

mod component;
mod ide;
mod utf16;

pub use utf16::{byte_offset, positions, utf16_offset};

use component::{ComponentReply, PrepareReply, PreparedEmit};

/// Version of the compiler in this build, e.g. `"0.1.0"`.
#[wasm_bindgen]
pub fn compiler_version() -> String {
    flick_core::VERSION.to_owned()
}

/// The module being edited, its last analysis, and the cache the next one
/// reuses: a revision parses and checks only what changed.
#[wasm_bindgen]
pub struct Session {
    text: String,
    sources: SourceMap,
    analysis: Analysis,
    /// The in-memory project the cached analysis is of, so that it stages
    /// the built-in modules as a run on Flick's own runtime does.
    rt: Project,
    cache: ModuleCache,
    /// The analysis `emit` builds from: a cached editor run drops IR, so
    /// emission takes a cold run of the same text, kept until it changes.
    emitted: RefCell<Option<Box<Emitted>>>,
    /// The most recent prepared `flick:backend@0.1.0` request awaiting `finish_emit`.
    prepared: RefCell<Option<Box<PreparedEmit>>>,
}

/// A cold analysis of the text it is of, with the sources its file ids name.
struct Emitted {
    text: String,
    analysis: Analysis,
    sources: SourceMap,
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
        let rt = rt_project();
        let mut sources = SourceMap::default();
        let mut cache = ModuleCache::default();
        let analysis =
            project::analyze_with(&Demo(""), &mut sources, Some(&rt), &[path()], &mut cache);
        Session {
            text: String::new(),
            sources,
            analysis,
            rt,
            cache,
            emitted: RefCell::new(None),
            prepared: RefCell::new(None),
        }
    }

    /// Analyses `source`, unless it is the text analysed last, and gives its
    /// diagnostics as a JSON array of the events `flick check --diagnostics
    /// json` writes, positions in UTF-16 (see [`positions`]).
    pub fn update(&mut self, source: &str) -> String {
        if source != self.text {
            source.clone_into(&mut self.text);
            self.sources = SourceMap::default();
            self.emitted = RefCell::new(None);
            self.prepared = RefCell::new(None);
            let text = &self.text;
            let sources = &mut self.sources;
            let (rt, cache) = (&self.rt, &mut self.cache);
            self.analysis = project::analyze_with(&Demo(text), sources, Some(rt), &[path()], cache);
        }
        self.diagnostics(&self.sources, &self.analysis.diagnostics)
            .to_string()
    }

    /// Prepares a `flick:backend@0.1.0` `BuildRequest` and declared input
    /// manifest for the pinned official `flick/js` component.
    ///
    /// Returns either `{ "status": "done", "emitted": <Emitted> }` when the
    /// frontend already failed, or `{ "status": "ready", "pin", "request",
    /// "program_b64", "inputs" }` to dispatch to the isolated backend worker.
    pub fn prepare_emit(&self, profile: &str, include_declarations: bool) -> String {
        let mut emitted = self.emitted_mut();
        let Emitted {
            analysis, sources, ..
        } = &mut *emitted;
        match component::prepare(profile, include_declarations, analysis, sources) {
            Ok(prepared) => {
                let mut wire_request = prepared.request.clone();
                let program_b64 = component::base64_encode(&wire_request.program);
                wire_request.program.clear();
                let inputs: BTreeMap<String, String> = prepared
                    .inputs
                    .iter()
                    .map(|(k, v)| (k.clone(), component::base64_encode(v)))
                    .collect();
                let reply = PrepareReply::Ready {
                    pin: prepared.request.backend.clone(),
                    request: Box::new(wire_request),
                    program_b64,
                    inputs,
                };
                *self.prepared.borrow_mut() = Some(Box::new(prepared));
                serde_json::to_string(&reply).expect("prepare reply serializes")
            }
            Err((diagnostics, dts_diagnostics)) => {
                *self.prepared.borrow_mut() = None;
                let mut out = Self::empty_output();
                out["diagnostics"] = self.diagnostics(sources, &diagnostics);
                out["dts_diagnostics"] = self.diagnostics(sources, &dts_diagnostics);
                serde_json::to_string(&PrepareReply::Done { emitted: out })
                    .expect("prepare error reply serializes")
            }
        }
    }

    /// Validates the `flick:backend@0.1.0` `BuildResult` (or error diagnostics)
    /// returned by the isolated backend worker and builds the playground
    /// `Emitted` JSON payload with UTF-16 editor positions.
    pub fn finish_emit(
        &self,
        profile: &str,
        include_declarations: bool,
        result_json: &str,
    ) -> String {
        let prepared = self.prepared.borrow_mut().take();
        let emitted = self.emitted_mut();
        let sources = &emitted.sources;
        let analysis = &emitted.analysis;
        let Some(prepared) = prepared else {
            let mut out = Self::empty_output();
            let diag = Diagnostic::new(
                codes::P013,
                Severity::Error,
                "finish_emit called without a matching prepare_emit",
            );
            out["diagnostics"] = self.diagnostics(sources, &[diag]);
            out["dts_diagnostics"] = Value::Array(Vec::new());
            return out.to_string();
        };
        if prepared.profile != profile || prepared.include_declarations != include_declarations {
            let mut out = Self::empty_output();
            let diag = Diagnostic::new(
                codes::P013,
                Severity::Error,
                "finish_emit profile or declaration options differ from prepare_emit",
            );
            out["diagnostics"] = self.diagnostics(sources, &[diag]);
            out["dts_diagnostics"] = Value::Array(Vec::new());
            return out.to_string();
        }
        let reply: ComponentReply = match serde_json::from_str(result_json) {
            Ok(reply) => reply,
            Err(error) => ComponentReply::TransportError {
                message: format!("invalid backend worker reply JSON: {error}"),
            },
        };
        let (artifacts, diagnostics) = component::finish(&prepared, reply);
        let mut out = if has_errors(&diagnostics) {
            Self::empty_output()
        } else {
            Self::output(artifacts, include_declarations, analysis)
        };
        out["diagnostics"] = self.diagnostics(sources, &diagnostics);
        out["dts_diagnostics"] = self.diagnostics(sources, &prepared.dts_diagnostics);
        out.to_string()
    }

    /// `{ "js", "dts", "files", "entry", "diagnostics", "dts_diagnostics" }`
    /// for the last analysed module as `flick build --profile <profile>
    /// --declarations` writes it. On native targets, drives the exact same
    /// `prepare_emit` -> `flick/js` component contract -> `finish_emit` path.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn emit(&self, profile: &str) -> String {
        self.emit_via_component(profile, false)
    }

    /// The normal emit plus declaration files for resolving re-exported signatures.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn emit_declarations(&self, profile: &str) -> String {
        self.emit_via_component(profile, true)
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
    /// The cold analysis of the current text, made once for each revision.
    fn emitted_mut(&self) -> RefMut<'_, Emitted> {
        let stale = self
            .emitted
            .borrow()
            .as_ref()
            .is_none_or(|e| e.text != self.text);
        if stale {
            let mut sources = SourceMap::default();
            let analysis = project::analyze_rt(&Demo(&self.text), &mut sources, None, &[path()]);
            let emitted = Emitted {
                text: self.text.clone(),
                analysis,
                sources,
            };
            *self.emitted.borrow_mut() = Some(Box::new(emitted));
        }
        RefMut::map(self.emitted.borrow_mut(), |slot| {
            slot.as_deref_mut().expect("the analysis was just made")
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn emit_via_component(&self, profile: &str, include_declarations: bool) -> String {
        let prepared_json = self.prepare_emit(profile, include_declarations);
        let prepared_value: Value =
            serde_json::from_str(&prepared_json).expect("prepare_emit returns valid JSON");
        if prepared_value["status"] == "done" {
            return prepared_value["emitted"].to_string();
        }
        let backend = flick_backend_js::Javascript;
        let descriptor = flick_backend::Backend::describe(&backend);
        let reply = {
            let prepared_ref = self.prepared.borrow();
            let prepared = prepared_ref
                .as_ref()
                .expect("prepare_emit stored prepared state");
            let reserved_paths: Vec<String> =
                prepared.shared_dts.iter().map(|a| a.path.clone()).collect();
            let deny_step = |_step: &flick_backend::ToolchainStep| {
                Err("browser host executes no toolchain steps".to_owned())
            };
            let deny_launch = |_launch: &flick_backend::Launch| {
                Err("browser host executes no native launches".to_owned())
            };
            let policy = flick_backend::Policy {
                toolchains: &[],
                environment: &[],
                reserved_paths: &reserved_paths,
                authorize_step: &deny_step,
                authorize_launch: &deny_launch,
                limits: flick_backend::Limits::default(),
            };
            match flick_backend::generate(
                &backend,
                &prepared.request,
                &policy,
                |id| prepared.inputs.get(id).cloned(),
                false,
            ) {
                Ok(generated) => json!({
                    "status": "ok",
                    "descriptor": descriptor,
                    "result": generated.result(),
                }),
                Err(flick_backend::Failure::Rejected(diagnostics)) => json!({
                    "status": "err",
                    "descriptor": descriptor,
                    "diagnostics": diagnostics,
                }),
                Err(why) => json!({
                    "status": "transport_error",
                    "message": why.to_string(),
                }),
            }
        };
        self.finish_emit(profile, include_declarations, &reply.to_string())
    }

    fn empty_output() -> Value {
        json!({ "js": null, "dts": null, "files": null, "entry": null, "tests": null })
    }

    fn file(&self) -> Option<FileId> {
        self.analysis
            .modules
            .iter()
            .find(|m| m.path == path())
            .map(|m| m.file)
    }

    fn output(
        artifacts: Vec<build::Artifact>,
        include_declarations: bool,
        analysis: &Analysis,
    ) -> Value {
        let stem = (analysis.compilation.modules.iter())
            .find(|m| m.path == path())
            .map(|m| m.name.replace('.', "/"));
        let declaration_entry = if include_declarations {
            stem.as_ref().map(|stem| format!("{stem}.d.ts"))
        } else {
            None
        };
        // `extend`, not `collect`: collecting into a map compiles a sort of
        // its own into this module, where the map's `insert` is already there
        let mut declarations = serde_json::Map::new();
        if include_declarations {
            declarations.extend(
                (artifacts.iter())
                    .filter(|a| {
                        a.path.ends_with(".d.ts")
                            && Some(a.path.as_str()) != declaration_entry.as_deref()
                    })
                    .map(|a| (a.path.clone(), Value::String(a.contents.clone()))),
            );
        }
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
        let mut files = serde_json::Map::new();
        files.extend(
            (artifacts.into_iter())
                .filter(|a| Path::new(&a.path).extension().is_some_and(|e| e == "js"))
                .map(|a| (a.path, Value::String(a.contents))),
        );
        let mut output = json!({
            "js": js,
            "dts": dts,
            "files": files,
            "entry": entry.unwrap_or(Value::Null),
            "tests": analysis.compilation.modules.iter()
                .find(|m| m.path == path() && m.interface.exports_value("tests"))
                .map(|m| json!({ "module": m.name, "entry": format!("{}.js", m.name.replace('.', "/")) })),
        });
        if include_declarations {
            output["declarations"] = Value::Object(declarations);
            output["declaration_entry"] = declaration_entry.map_or(Value::Null, Value::String);
        }
        output
    }

    fn diagnostics(&self, sources: &SourceMap, diagnostics: &[Diagnostic]) -> Value {
        let mut events =
            serde_json::to_value(json_events(sources, diagnostics).collect::<Vec<_>>())
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

/// The in-memory project of the playground: one file at the root, staged for
/// Flick's own runtime, with no manifest on disk.
fn rt_project() -> Project {
    let config = Config {
        source_dir: ".".to_owned(),
        ..Config::default()
    };
    let mut sources = SourceMap::default();
    let config_file = sources.add("/flick.toml".to_owned(), "");
    Project {
        root: PathBuf::from("/"),
        config,
        config_file,
        workspace: None,
    }
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod emit_tests;
