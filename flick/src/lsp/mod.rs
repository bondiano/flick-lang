//! `flick lsp --stdio`: the language server. stdout carries only the
//! protocol; logs go to stderr. Every change re-analyzes the project the
//! document belongs to with the open buffers over the files, through the
//! same analysis that compiles it, and publishes the diagnostics `flick
//! check` reports, positions converted to UTF-16.

mod features;
mod protocol;
mod requests;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, BufReader};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::process::ExitCode;
use std::sync::LazyLock;

use flick_core::diagnostics::{Diagnostic, Severity};
use flick_core::project::{self, Analysis, Analyzed, Compilation, Host};
use flick_core::source::{FileId, SourceMap};
use flick_core::{resolve, syntax, types};
use serde_json::{Value, json};

use crate::sources;

static BUILTIN_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let temp = env::temp_dir();
    temp.canonicalize().unwrap_or(temp).join("flick-lsp")
});

/// Files read from the open buffers first.
struct Overlay<'a> {
    docs: &'a HashMap<PathBuf, String>,
}

impl Host for Overlay<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self.docs.get(path) {
            Some(text) => Ok(text.clone().into_bytes()),
            None => fs::read(path),
        }
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

/// One analysis and the sources it points into.
pub struct Snapshot {
    pub analysis: Analysis,
    pub sources: SourceMap,
    /// The project's root, where `node_modules` is.
    pub root: PathBuf,
}

impl Snapshot {
    pub fn file(&self, path: &Path) -> Option<FileId> {
        self.analysis
            .modules
            .iter()
            .find(|m| m.path == path)
            .map(|m| m.file)
    }
}

/// Analyzes the project of `path` with `docs` over the files.
pub fn analyze(docs: &HashMap<PathBuf, String>, path: &Path) -> Snapshot {
    if let Some(index) = builtin_index(path) {
        return analyze_builtin(path, index);
    }
    let host = Overlay { docs };
    let mut sources = SourceMap::default();
    let dir = path
        .parent()
        .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
    let found = project::discover(&host, &mut sources, &dir, None)
        .ok()
        .flatten();
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(p) = &found {
        for root in [p.source_root(), p.test_root()] {
            let mut files = Vec::new();
            if root.is_dir() {
                sources::source_files(&root.to_string_lossy(), &mut files);
            }
            roots.extend(files.into_iter().map(PathBuf::from));
        }
    }
    roots.extend(
        docs.keys()
            .filter(|p| p.extension().is_some_and(|e| e == "fl"))
            .cloned(),
    );
    roots.sort();
    roots.dedup();
    let analysis = project::analyze(&host, &mut sources, found.as_ref(), &roots);
    let root = found.map_or(dir, |p| p.root);
    Snapshot {
        analysis,
        sources,
        root,
    }
}

fn builtin_index(path: &Path) -> Option<usize> {
    if path.extension()? != "fl" {
        return None;
    }
    let relative = path.strip_prefix(&*BUILTIN_DIR).ok()?;
    let mut parts = relative.components();
    parts.next()?;
    let module = parts.as_path().with_extension("");
    let module = module.to_str()?.replace(MAIN_SEPARATOR, ".");
    types::builtin_sources()
        .iter()
        .position(|(name, _)| *name == module)
}

/// Check library sources in the compiler's dependency order, not as project
/// roots importing their own already-registered instances.
fn analyze_builtin(path: &Path, index: usize) -> Snapshot {
    let (name, text) = types::builtin_sources()[index];
    let dependencies = &types::builtins()[..index];
    let mut sources = SourceMap::default();
    let file = sources.add(path.to_string_lossy(), text);
    let parsed = syntax::parse(text, file);
    let module_syntax = resolve::module_syntax(&parsed.tree, text, file);
    let module = resolve::Module {
        name,
        file,
        text,
        tree: &parsed.tree,
        syntax: &module_syntax,
    };
    let imported = |name: &str| dependencies.iter().find(|i| i.module == name).cloned();
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &imported);
    diagnostics.extend(parsed.diagnostics);
    let local = |name: &str| types::builtin_sources().iter().any(|(n, _)| *n == name);
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        sql: Default::default(),
        no_let_assert: false,
    };
    let (checked, typing) =
        types::check_typed(&module, &imported, dependencies, options, &mut interface);
    diagnostics.extend(checked);
    Snapshot {
        analysis: Analysis {
            modules: vec![Analyzed {
                name: name.to_owned(),
                path: path.to_path_buf(),
                file,
                test: false,
                tree: parsed.tree,
                typing,
                interface,
            }],
            compilation: Compilation::default(),
            diagnostics,
        },
        sources,
        root: path.parent().expect("source directory").to_path_buf(),
    }
}

pub struct Server {
    docs: HashMap<PathBuf, String>,
    snapshot: Option<Snapshot>,
    /// Files that have diagnostics published, to clear them later.
    published: HashSet<PathBuf>,
    out: io::Stdout,
}

pub fn serve() -> ExitCode {
    let mut server = Server {
        docs: HashMap::new(),
        snapshot: None,
        published: HashSet::new(),
        out: io::stdout(),
    };
    let mut input = BufReader::new(io::stdin().lock());
    let mut shut_down = false;
    loop {
        let message = match protocol::read(&mut input) {
            Ok(Some(m)) => m,
            Ok(None) => return ExitCode::from(u8::from(!shut_down)),
            Err(err) => {
                eprintln!("flick lsp: {err}");
                return ExitCode::FAILURE;
            }
        };
        let method = message["method"].as_str().unwrap_or("");
        let params = &message["params"];
        match (method, message.get("id")) {
            ("exit", _) => return ExitCode::from(u8::from(!shut_down)),
            ("shutdown", Some(id)) => {
                shut_down = true;
                server.send(&protocol::response(id, &Value::Null));
            }
            (_, Some(id)) => {
                let reply = match server.request(method, params) {
                    Ok(result) => protocol::response(id, &result),
                    Err((code, text)) => protocol::error(id, code, &text),
                };
                server.send(&reply);
            }
            (_, None) => server.notify(method, params),
        }
    }
}

impl Server {
    fn send(&mut self, message: &Value) {
        if let Err(err) = protocol::write(&mut self.out, message) {
            eprintln!("flick lsp: cannot write: {err}");
        }
    }

    fn notify(&mut self, method: &str, params: &Value) {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        let Some(path) = protocol::path_of(uri) else {
            return;
        };
        if let Some(index) = builtin_index(&path) {
            if method == "textDocument/didOpen" {
                let snapshot = analyze_builtin(&path, index);
                let diagnostics: Vec<_> = snapshot
                    .analysis
                    .diagnostics
                    .iter()
                    .filter(|d| d.span.is_some())
                    .map(|d| lsp_diagnostic(&snapshot.sources, d))
                    .collect();
                self.send(&publish(&path, &diagnostics));
                self.snapshot = Some(snapshot);
            }
            return;
        }
        match method {
            "textDocument/didOpen" => {
                let text = params["textDocument"]["text"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned();
                self.docs.insert(path.clone(), text);
            }
            "textDocument/didChange" => {
                let changes = params["contentChanges"].as_array();
                if let Some(text) = changes
                    .and_then(|c| c.last())
                    .and_then(|c| c["text"].as_str())
                {
                    self.docs.insert(path.clone(), text.to_owned());
                }
                // like Gleam, diagnostics wait for a save so half-typed code
                // is not flagged; requests re-analyze the new text lazily
                self.snapshot = None;
                return;
            }
            "textDocument/didClose" => {
                self.docs.remove(&path);
            }
            "textDocument/didSave" => {}
            _ => return,
        }
        self.refresh(&path);
    }

    /// Re-analyzes and publishes every file's diagnostics.
    fn refresh(&mut self, path: &Path) {
        let snapshot = analyze(&self.docs, path);
        let mut by_file: HashMap<PathBuf, Vec<&Diagnostic>> = HashMap::new();
        for d in &snapshot.analysis.diagnostics {
            if let Some(span) = d.span {
                by_file
                    .entry(PathBuf::from(&snapshot.sources.get(span.file).path))
                    .or_default()
                    .push(d);
            }
        }
        // open documents always get a report, even an empty one
        for open in self.docs.keys() {
            by_file.entry(open.clone()).or_default();
        }
        let mut messages = Vec::new();
        for stale in self
            .published
            .difference(&by_file.keys().cloned().collect())
        {
            messages.push(publish(stale, &[]));
        }
        for (file, diagnostics) in &by_file {
            let list: Vec<Value> = diagnostics
                .iter()
                .map(|d| lsp_diagnostic(&snapshot.sources, d))
                .collect();
            messages.push(publish(file, &list));
        }
        self.published = by_file.keys().cloned().collect();
        self.snapshot = Some(snapshot);
        for m in messages {
            self.send(&m);
        }
    }
}

fn publish(path: &Path, diagnostics: &[Value]) -> Value {
    protocol::notification(
        "textDocument/publishDiagnostics",
        &json!({ "uri": protocol::uri_of(path), "diagnostics": diagnostics }),
    )
}

/// A diagnostic in LSP terms; its code keeps the `FLICK-` spelling.
pub fn lsp_diagnostic(sources: &SourceMap, d: &Diagnostic) -> Value {
    let span = d.span.expect("only located diagnostics are published");
    let text = &sources.get(span.file).text;
    let severity = match d.severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Note => 3,
    };
    let related: Vec<Value> = d
        .related
        .iter()
        .map(|r| {
            let file = sources.get(r.span.file);
            json!({
                "location": {
                    "uri": protocol::uri_of(Path::new(&file.path)),
                    "range": protocol::range(&file.text, r.span.start, r.span.end),
                },
                "message": r.message,
            })
        })
        .collect();
    // fixes without edits are advice; editors show them under the message
    let message = d
        .fixes
        .iter()
        .filter(|f| f.edits.is_empty())
        .fold(d.message.clone(), |m, f| {
            format!("{m}\nhelp: {}", f.message)
        });
    json!({
        "range": protocol::range(text, span.start, span.end),
        "severity": severity,
        "code": d.code.to_string(),
        "source": "flick",
        "message": message,
        "relatedInformation": related,
        // 1 is `Unnecessary`: editors grey the code out
        "tags": if d.unnecessary() { json!([1]) } else { json!([]) },
    })
}
