//! `flick lsp --stdio`: the language server. stdout carries only the
//! protocol; logs go to stderr. Every change re-analyzes the project the
//! document belongs to with the open buffers over the files, through the
//! same analysis that compiles it, and publishes the diagnostics `flick
//! check` reports, positions converted to UTF-16.

mod features;
mod logging;
pub mod protocol;
mod requests;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, BufReader, Write};
use std::path::{MAIN_SEPARATOR, Path, PathBuf};
use std::process::{self, ExitCode};
use std::sync::LazyLock;
use std::time::Instant;

use flick_core::diagnostics::{Diagnostic, Severity};
use flick_core::project::config::Emit;
use flick_core::project::{self, Analysis, Analyzed, Compilation, Host, ModuleCache};
use flick_core::source::{FileId, SourceMap};
use flick_core::{resolve, syntax, types};
use serde_json::{Value, json};
use tracing::{debug, debug_span, error, info, info_span, warn};

use crate::{builtins, sources};

static BUILTIN_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let temp = env::temp_dir();
    temp.canonicalize().unwrap_or(temp).join("flick-lsp")
});

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => canonical(parent).join(name),
            _ => project::normalize(path),
        })
}

/// Files read from the open buffers first.
struct Overlay<'a> {
    docs: HashMap<PathBuf, (&'a Path, &'a str)>,
}

impl Host for Overlay<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self
            .docs
            .get(path)
            .or_else(|| self.docs.get(&canonical(path)))
        {
            Some((_, text)) => Ok(text.as_bytes().to_vec()),
            None => fs::read(path),
        }
    }

    fn display(&self, path: &Path) -> String {
        self.docs
            .get(path)
            .or_else(|| self.docs.get(&canonical(path)))
            .map_or(path, |(original, _)| *original)
            .display()
            .to_string()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Context {
    Project(PathBuf),
    Standalone(PathBuf),
    Builtin(PathBuf),
}

impl Context {
    fn of(docs: &HashMap<PathBuf, String>, path: &Path) -> Self {
        if builtin_index(path).is_some() {
            return Self::Builtin(canonical(path));
        }
        for dir in path.parent().unwrap_or(Path::new("/")).ancestors() {
            let config = dir.join(project::CONFIG_FILE);
            if docs.contains_key(&config) || config.is_file() {
                return Self::Project(canonical(dir));
            }
        }
        Self::Standalone(canonical(path))
    }
}

/// One analysis and the sources it points into.
pub struct Snapshot {
    pub analysis: Analysis,
    pub sources: SourceMap,
    /// The project's root, where `node_modules` is.
    pub root: PathBuf,
    /// The target selected by the project's build configuration.
    pub emit: Emit,
    context: Context,
}

impl Snapshot {
    pub fn file(&self, path: &Path) -> Option<FileId> {
        self.analysis
            .modules
            .iter()
            .find(|m| m.path == path)
            .or_else(|| {
                let path = canonical(path);
                self.analysis
                    .modules
                    .iter()
                    .find(|m| canonical(&m.path) == path)
            })
            .map(|m| m.file)
    }
}

/// What each context kept of its earlier analyses.
pub type Caches = HashMap<Context, ModuleCache>;

/// Analyzes the project of `path` with `docs` over the files, reusing the
/// modules that did not change since the context's earlier analyses.
pub fn analyze(docs: &HashMap<PathBuf, String>, path: &Path, caches: &mut Caches) -> Snapshot {
    let started = Instant::now();
    let _span =
        debug_span!("analysis", path = %path.display(), open_documents = docs.len()).entered();
    if let Some(index) = builtin_index(path) {
        let snapshot = analyze_builtin(&canonical(path), index);
        debug!(
            modules = snapshot.analysis.modules.len(),
            diagnostics = snapshot.analysis.diagnostics.len(),
            elapsed = ?started.elapsed(),
            "built-in analysis completed"
        );
        return snapshot;
    }
    let host = Overlay {
        docs: docs
            .iter()
            .map(|(path, text)| (canonical(path), (path.as_path(), text.as_str())))
            .collect(),
    };
    let mut sources = SourceMap::default();
    let dir = path
        .parent()
        .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
    let found = project::discover(&host, &mut sources, &dir, None)
        .ok()
        .flatten();
    let context = Context::of(docs, path);
    let mut roots: Vec<PathBuf> = Vec::new();
    let directories = found.as_ref().map(|p| [p.source_root(), p.test_root()]);
    if let Some(directories) = &directories {
        for root in directories {
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
    if path.extension().is_some_and(|e| e == "fl") {
        roots.push(path.to_path_buf());
    }
    roots.retain(|root| Context::of(docs, root) == context);
    // Aliases of one project must keep the same source/test module paths.
    if let Some(directories) = &directories {
        let physical = directories.each_ref().map(|dir| canonical(dir));
        for root in &mut roots {
            let path = canonical(root);
            for (dir, physical) in directories.iter().zip(&physical) {
                if let Ok(relative) = path.strip_prefix(physical) {
                    *root = dir.join(relative);
                    break;
                }
            }
        }
    }
    roots.sort();
    let mut seen = HashSet::new();
    roots.retain(|root| seen.insert(canonical(root)));
    let cache = caches.entry(context.clone()).or_default();
    let analysis = project::analyze_with(&host, &mut sources, found.as_ref(), &roots, cache);
    let emit = found.as_ref().map_or(Emit::Js, |p| p.config.build.emit);
    let root = found.map_or(dir, |p| p.root);
    debug!(
        root = %root.display(),
        modules = analysis.modules.len(),
        diagnostics = analysis.diagnostics.len(),
        elapsed = ?started.elapsed(),
        "analysis completed"
    );
    Snapshot {
        analysis,
        sources,
        root,
        emit,
        context,
    }
}

fn builtin_index(path: &Path) -> Option<usize> {
    if path.extension()? != "fl" {
        return None;
    }
    let path = canonical(path);
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
        thresholds: types::Thresholds::default(),
        effects: true,
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
                typing: typing.into(),
                interface,
            }],
            compilation: Compilation::default(),
            diagnostics,
        },
        sources,
        root: path.parent().expect("source directory").to_path_buf(),
        emit: Emit::Js,
        context: Context::Builtin(path.to_path_buf()),
    }
}

pub struct Server {
    docs: HashMap<PathBuf, String>,
    snapshot: Option<Snapshot>,
    caches: Caches,
    /// Each context clears only the diagnostics it previously published.
    published: HashMap<Context, HashSet<PathBuf>>,
    out: Box<dyn Write>,
}

pub fn serve() -> ExitCode {
    logging::init();
    builtins::prepare();
    info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = process::id(),
        "language server started"
    );
    let mut server = Server {
        docs: HashMap::new(),
        snapshot: None,
        caches: Caches::new(),
        published: HashMap::new(),
        out: Box::new(io::stdout()),
    };
    let mut input = BufReader::new(io::stdin().lock());
    let mut shut_down = false;
    loop {
        let message = match protocol::read(&mut input) {
            Ok(Some(m)) => m,
            Ok(None) => {
                info!(
                    shutdown = shut_down,
                    reason = "eof",
                    "language server stopped"
                );
                return ExitCode::from(u8::from(!shut_down));
            }
            Err(err) => {
                error!(error = %err, "cannot read protocol message");
                return ExitCode::FAILURE;
            }
        };
        let method = message["method"].as_str().unwrap_or("");
        let params = &message["params"];
        let _span = info_span!(
            "lsp.message",
            method,
            id = %message["id"],
            uri = params["textDocument"]["uri"].as_str(),
            line = params["position"]["line"].as_u64(),
            character = params["position"]["character"].as_u64(),
        )
        .entered();
        let started = Instant::now();
        debug!("message received");
        match (method, message.get("id")) {
            ("exit", _) => {
                info!(
                    shutdown = shut_down,
                    reason = "exit",
                    "language server stopped"
                );
                return ExitCode::from(u8::from(!shut_down));
            }
            ("shutdown", Some(id)) => {
                info!("shutdown requested");
                shut_down = true;
                server.send(&protocol::response(id, &Value::Null));
            }
            (_, Some(id)) => {
                let reply = match server.request(method, params) {
                    Ok(result) => protocol::response(id, &result),
                    Err((code, text)) => {
                        warn!(code, error = %text, "request failed");
                        protocol::error(id, code, &text)
                    }
                };
                server.send(&reply);
            }
            (_, None) => server.notify(method, params),
        }
        debug!(elapsed = ?started.elapsed(), "message handled");
    }
}

impl Server {
    fn send(&mut self, message: &Value) {
        if let Err(err) = protocol::write(&mut self.out, message) {
            error!(error = %err, "cannot write protocol message");
        }
    }

    fn notify(&mut self, method: &str, params: &Value) {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        let Some(path) = protocol::path_of(uri) else {
            return;
        };
        if builtin_index(&path).is_some() {
            match method {
                "textDocument/didOpen" => self.refresh(&path),
                "textDocument/didClose" => self.clear(&Context::of(&self.docs, &path), &path),
                _ => {}
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
                let context = Context::of(&self.docs, &path);
                if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.context == context || s.file(&path).is_some())
                {
                    self.snapshot = None;
                }
                return;
            }
            "textDocument/didClose" => {
                let context = Context::of(&self.docs, &path);
                self.docs.remove(&path);
                if !self
                    .docs
                    .keys()
                    .any(|p| Context::of(&self.docs, p) == context)
                {
                    self.clear(&context, &path);
                    return;
                }
            }
            "textDocument/didSave" => {}
            _ => return,
        }
        self.refresh(&path);
    }

    fn clear(&mut self, context: &Context, path: &Path) {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.context == *context || s.file(path).is_some())
        {
            self.snapshot = None;
        }
        // a project keeps its cache for the next document opened in it; a
        // standalone file's goes with its last document, or every file ever
        // opened would stay in memory
        if let Context::Standalone(_) = context {
            self.caches.remove(context);
        }
        if let Some(files) = self.published.remove(context) {
            let others = self.release(files);
            for open in others {
                self.refresh(&open);
            }
        }
    }

    /// Empties the diagnostics of `files` no longer published for a context,
    /// except those another live context also publishes: an open document of
    /// each such context, whose analysis publishes them again.
    fn release(&mut self, files: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
        let mut others = Vec::new();
        for file in files {
            let owner = self
                .published
                .iter()
                .find(|(_, published)| published.contains(&file))
                .map(|(context, _)| context.clone());
            let open = owner.and_then(|owner| {
                self.docs
                    .keys()
                    .find(|doc| Context::of(&self.docs, doc) == owner)
                    .cloned()
            });
            match open {
                Some(open) => others.push(open),
                None => self.send(&publish(&file, &[])),
            }
        }
        others.sort();
        others.dedup();
        others
    }

    /// Re-analyzes and publishes this context's diagnostics.
    fn refresh(&mut self, path: &Path) {
        let snapshot = analyze(&self.docs, path, &mut self.caches);
        let mut by_file: HashMap<PathBuf, Vec<&Diagnostic>> = HashMap::new();
        for d in &snapshot.analysis.diagnostics {
            if let Some(span) = d.span {
                by_file
                    .entry(PathBuf::from(&snapshot.sources.get(span.file).path))
                    .or_default()
                    .push(d);
            }
        }
        // Open documents always get a report, even an empty one.
        for open in self.docs.keys() {
            if Context::of(&self.docs, open) == snapshot.context {
                by_file.entry(open.clone()).or_default();
            }
        }
        if builtin_index(path).is_some() {
            by_file.entry(path.to_path_buf()).or_default();
        }
        let current: HashSet<_> = by_file.keys().cloned().collect();
        let stale: Vec<PathBuf> = self
            .published
            .insert(snapshot.context.clone(), current)
            .into_iter()
            .flatten()
            .filter(|file| !by_file.contains_key(file))
            .collect();
        let mut messages = Vec::new();
        for (file, diagnostics) in &by_file {
            let list: Vec<Value> = diagnostics
                .iter()
                .map(|d| lsp_diagnostic(&snapshot.sources, d))
                .collect();
            messages.push(publish(file, &list));
        }
        let context = snapshot.context.clone();
        self.snapshot = Some(snapshot);
        for m in messages {
            self.send(&m);
        }
        // another context still showing a stale file publishes it again, once
        for open in self.release(stale) {
            if Context::of(&self.docs, &open) != context {
                self.refresh(&open);
            }
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

#[cfg(test)]
mod tests;
