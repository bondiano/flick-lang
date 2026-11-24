//! `flick lsp --stdio`: the language server. stdout carries only the
//! protocol; logs go to stderr. Every change re-analyzes the project the
//! document belongs to with the open buffers over the files, through the
//! same analysis that compiles it, and publishes the diagnostics `flick
//! check` reports, positions converted to UTF-16.

mod deferred;
mod diagnostics;
mod features;
mod logging;
mod members;
pub mod protocol;
mod registration;
mod requests;
mod snapshot;
mod worker;

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::io::{self, BufReader, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use flick_core::diagnostics::Diagnostic;
use flick_core::project;
use serde_json::Value;
use tracing::{debug, error, info, info_span};

use crate::builtins;
use deferred::Deferred;
use diagnostics::{lsp_diagnostic, publish};
use snapshot::{BUILTIN_DIR, builtin_index, canonical, overlay};
pub use snapshot::{Caches, Context, Snapshot, analyze};
use worker::Worker;

/// Changes in other contexts can still touch an input of an in-flight analysis.
/// Its source map is available when it finishes.
struct Pending {
    generation: u64,
    changed_inputs: HashSet<PathBuf>,
}

pub struct Server {
    docs: HashMap<PathBuf, String>,
    versions: HashMap<PathBuf, i64>,
    /// Clients subscribe to proof updates by querying a goal.
    proof_watch: bool,
    /// The analysis of each context since the last change of a document it
    /// reads: a request of one context does not cost the next request of
    /// another its analysis.
    snapshots: HashMap<Context, Snapshot>,
    /// The context analyzed last, which `workspace/symbol` starts from.
    latest: Option<Context>,
    caches: Caches,
    /// Each context clears only the diagnostics it previously published.
    published: HashMap<Context, HashSet<PathBuf>>,
    out: Box<dyn Write>,
    /// The extensions of loaded files registered with the client; none when
    /// the client registers nothing later.
    registered: Option<HashSet<String>>,
    /// The requests the server sent the client.
    requests: u64,
    /// The thread that analyzes document changes off the message loop.
    worker: Worker,
    /// The newest analysis asked for each independently published context.
    pending: HashMap<Context, Pending>,
    /// Counts revisions, so a slow analysis of an old text is dropped.
    generation: u64,
    /// The requests held until the analysis of their document is published.
    deferred: Vec<Deferred>,
}

pub fn serve() -> ExitCode {
    logging::init();
    // the server cannot check a file without them; the editor shows the log
    if let Err(diagnostic) = builtins::prepare() {
        error!(code = %diagnostic.code, "{}", diagnostic.message);
        return ExitCode::FAILURE;
    }
    info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = process::id(),
        "language server started"
    );
    let mut server = Server::new(Box::new(io::stdout()));
    // the protocol reader has its own thread: waiting for the editor's next
    // message does not stop a finished analysis from being published
    let (messages, input) = mpsc::channel::<io::Result<Option<Value>>>();
    thread::Builder::new()
        .name("flick-lsp-input".to_owned())
        .spawn(move || {
            let mut reader = BufReader::new(io::stdin().lock());
            loop {
                let message = protocol::read(&mut reader);
                let last = !matches!(message, Ok(Some(_)));
                if messages.send(message).is_err() || last {
                    return;
                }
            }
        })
        .expect("the protocol reader starts");
    let mut shut_down = false;
    loop {
        // publish what the worker finished and answer what waited for it
        server.guarded(Server::poll);
        let message = match input.recv_timeout(Duration::from_millis(10)) {
            Ok(Ok(Some(message))) => message,
            Ok(Ok(None)) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                server.guarded(Server::settle);
                info!(
                    shutdown = shut_down,
                    reason = "eof",
                    "language server stopped"
                );
                return ExitCode::from(u8::from(!shut_down));
            }
            Ok(Err(err)) => {
                error!(error = %err, "cannot read protocol message");
                return ExitCode::FAILURE;
            }
            // nothing from the editor for a while: keep publishing analyses
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
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
            // what the client answers a request of the server's
            ("", Some(_)) => debug!("response received"),
            ("$/cancelRequest", None) => server.cancel_request(params),
            (_, Some(id)) => {
                // a request reads the analysis of its document: holding it
                // until the background analysis lands keeps the loop free
                // and lets the client cancel it meanwhile
                if server.should_defer(method, params) {
                    server.defer(id.clone(), method, params.clone());
                } else {
                    server.answer(id, method, params);
                }
            }
            (_, None) => {
                server.guarded(|s| s.notify(method, params));
            }
        }
        debug!(elapsed = ?started.elapsed(), "message handled");
    }
}

impl Server {
    fn new(out: Box<dyn Write>) -> Self {
        Self {
            docs: HashMap::new(),
            versions: HashMap::new(),
            proof_watch: false,
            snapshots: HashMap::new(),
            latest: None,
            caches: Caches::new(),
            published: HashMap::new(),
            out,
            registered: None,
            requests: 0,
            worker: Worker::new(),
            pending: HashMap::new(),
            generation: 0,
            deferred: Vec::new(),
        }
    }

    /// Runs `f`; a panic in it, which the hook has logged, is `None`, and
    /// the analyses it may have left half-built are dropped, so the next
    /// message analyzes anew instead of the editor losing the server.
    fn guarded<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> Option<T> {
        let out = panic::catch_unwind(AssertUnwindSafe(|| f(self))).ok();
        if out.is_none() {
            self.snapshots.clear();
            self.latest = None;
            self.caches.clear();
            self.worker.clear(None);
        }
        out
    }

    fn send(&mut self, message: &Value) {
        if let Err(err) = protocol::write(&mut self.out, message) {
            error!(error = %err, "cannot write protocol message");
        }
    }

    fn notify(&mut self, method: &str, params: &Value) {
        // a notification about files on disk, not about an open document
        if method == "workspace/didChangeWatchedFiles" {
            self.watched_files(params);
            return;
        }
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        let Some(path) = protocol::path_of(uri) else {
            return;
        };
        if builtin_index(&path).is_some() {
            match method {
                "textDocument/didOpen" => self.refresh(&path),
                "textDocument/didClose" => self.clear(&Context::of(&self.docs, &path)),
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
                // Incremental sync: each change replaces the range it names
                // in the text as the previous changes left it. A change
                // without a range still replaces the whole document.
                let changes = params["contentChanges"].as_array();
                let text = self.docs.entry(path.clone()).or_default();
                if let Some(changes) = changes {
                    apply_changes(text, changes);
                }
            }
            "textDocument/didClose" => {
                let context = Context::of(&self.docs, &path);
                self.docs.remove(&path);
                self.versions.remove(&path);
                self.input_changed(&path);
                // the closed text is gone: analyze an open document of its
                // project instead, so the disk view is what gets published
                if let Some(open) = self.open_in(&context) {
                    self.refresh(&open);
                } else {
                    self.clear(&context);
                    self.generation += 1;
                    self.proof_state_changed();
                }
                return;
            }
            "textDocument/didSave" => {}
            _ => return,
        }
        if let Some(version) = params["textDocument"]["version"].as_i64() {
            self.versions.insert(path.clone(), version);
        }
        self.input_changed(&path);
        self.refresh(&path);
    }

    fn open_in(&self, context: &Context) -> Option<PathBuf> {
        self.docs
            .keys()
            .find(|p| Context::of(&self.docs, p) == *context)
            .cloned()
    }

    fn input_changed(&mut self, path: &Path) {
        let mut dependents = Vec::new();
        let changed = Context::of(&self.docs, path);
        self.snapshots.retain(|context, snapshot| {
            let invalidated = snapshot.reads(path);
            if invalidated && *context != changed {
                dependents.push(context.clone());
            }
            !invalidated
        });
        for pending in self.pending.values_mut() {
            pending.changed_inputs.insert(path.to_path_buf());
        }
        for context in dependents {
            if let Some(open) = self.open_in(&context) {
                self.refresh(&open);
            }
        }
    }

    fn clear(&mut self, context: &Context) {
        // what read `path` went when the notification arrived
        self.snapshots.remove(context);
        if self.latest.as_ref() == Some(context) {
            self.latest = None;
        }
        // a project keeps its cache for the next document opened in it, but
        // only the project closed last does: each closed one holds tens of
        // MiB; a standalone file's goes with its last document, or every
        // file ever opened would stay in memory
        match context {
            Context::Standalone(_) => {
                self.caches.remove(context);
                self.worker.clear(Some(context.clone()));
            }
            Context::Project(_) => {
                let mut kept: HashSet<Context> = self
                    .docs
                    .keys()
                    .map(|p| Context::of(&self.docs, p))
                    .collect();
                kept.insert(context.clone());
                self.caches.retain(|cached, _| kept.contains(cached));
                self.worker.retain(kept);
            }
            Context::Builtin(_) => {}
        }
        // analyses still running for it describe a text nobody sees now
        self.pending.remove(context);
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
            let open = owner.and_then(|owner| self.open_in(&owner));
            match open {
                Some(open) => others.push(open),
                None => self.send(&publish(&file, &[])),
            }
        }
        others.sort();
        others.dedup();
        others
    }

    /// Analyzes the document's context in the background and publishes the
    /// result when the worker finishes, unless a newer revision replaced it.
    fn refresh(&mut self, path: &Path) {
        self.generation += 1;
        let generation = self.generation;
        let context = Context::of(&self.docs, path);
        self.pending.insert(
            context.clone(),
            Pending {
                generation,
                changed_inputs: HashSet::new(),
            },
        );
        self.worker.submit(worker::Job {
            path: path.to_path_buf(),
            context,
            docs: self.docs.clone(),
            generation,
        });
        self.proof_state_changed();
    }

    fn proof_state_changed(&mut self) {
        if self.proof_watch {
            self.send(&protocol::notification(
                "flick/proofStateChanged",
                &serde_json::json!({ "generation": self.generation }),
            ));
        }
    }

    /// Consumes a completion only for the job still responsible for its context.
    fn publish(&mut self, done: worker::Done) {
        let Some(pending) = self.pending.get(&done.context) else {
            return;
        };
        if pending.generation != done.generation {
            return;
        }
        let mut snapshot = done.snapshot;
        let current = Context::of(&self.docs, &done.path);
        let stale = current != done.context
            || snapshot.context != done.context
            || pending
                .changed_inputs
                .iter()
                .any(|path| snapshot.reads(path));
        self.pending.remove(&done.context);
        if stale {
            // A new manifest may have moved this document, or another project
            // edited a dependency while we were discovering the inputs.
            if current != done.context
                && let Some(open) = self.open_in(&done.context)
            {
                self.refresh(&open);
            }
            if !self.pending.contains_key(&current) && self.docs.contains_key(&done.path) {
                self.refresh(&done.path);
            }
            return;
        }
        snapshot.generation = done.generation;
        self.publish_snapshot(&done.path, snapshot);
    }

    fn publish_snapshot(&mut self, path: &Path, snapshot: Snapshot) {
        self.register_loaded(snapshot.analysis.modules.iter().map(|m| m.path.as_path()));
        let mut by_file: HashMap<PathBuf, Vec<&Diagnostic>> = HashMap::new();
        for d in &snapshot.analysis.diagnostics {
            if let Some(span) = d.span {
                by_file
                    .entry(PathBuf::from(&snapshot.sources.get(span.file).path))
                    .or_default()
                    .push(d);
            }
        }
        // a library's files are reported by its own analysis, not its dependents';
        // the manifest of the workspace around the project is the project's too
        // (one that does not read included), as no other analysis reports it
        if let Context::Project(root) = &snapshot.context {
            let around = |file: &Path| {
                file.file_name() == Some(OsStr::new(project::CONFIG_FILE))
                    && (file.parent()).is_some_and(|dir| root.starts_with(canonical(dir)))
            };
            by_file.retain(|file, _| {
                Context::of(&self.docs, file) == snapshot.context || around(file)
            });
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
        self.latest = Some(context.clone());
        self.snapshots.insert(context.clone(), snapshot);
        self.proof_state_changed();
        for m in messages {
            self.send(&m);
        }
        // another context still showing a stale file publishes it again, once
        for open in self.release(stale) {
            if Context::of(&self.docs, &open) != context {
                self.refresh(&open);
            }
        }
        // the members that compile a library check it again with it
        if let Context::Project(dir) = &context {
            for member in members::dependents(&self.docs, dir) {
                let member = Context::Project(canonical(&member));
                if let Some(open) = self.open_in(&member) {
                    self.refresh(&open);
                }
            }
        }
    }

    /// Publishes the analyses that finished and answers the requests they held.
    fn poll(&mut self) {
        while let Some(done) = self.worker.try_recv() {
            self.publish(done);
        }
        self.resume();
    }

    /// Waits for every queued analysis, for the end of input.
    fn settle(&mut self) {
        while !self.pending.is_empty() {
            match self.worker.recv_timeout(Duration::from_secs(30)) {
                Some(done) => self.publish(done),
                None => break,
            }
        }
    }

    /// Files on disk outside the editor changed: the analyses that read them
    /// are stale, so their caches go and the open documents are analyzed anew.
    fn watched_files(&mut self, params: &Value) {
        let mut contexts = HashSet::new();
        let mut manifest = false;
        for change in params["changes"].as_array().into_iter().flatten() {
            let Some(path) = change["uri"].as_str().and_then(protocol::path_of) else {
                continue;
            };
            self.input_changed(&path);
            manifest |= path.file_name() == Some(OsStr::new(project::CONFIG_FILE));
            contexts.insert(Context::of(&self.docs, &path));
        }
        if manifest {
            self.worker.clear(None);
            self.caches.clear();
            self.snapshots.clear();
            self.latest = None;
            self.pending.clear();
        } else {
            for context in &contexts {
                self.worker.clear(Some(context.clone()));
                self.caches.remove(context);
                self.snapshots.remove(context);
            }
            self.pending
                .retain(|context, _| !contexts.contains(context));
        }
        let open: Vec<PathBuf> = self.docs.keys().cloned().collect();
        for path in open {
            if manifest || contexts.contains(&Context::of(&self.docs, &path)) {
                self.refresh(&path);
            }
        }
    }
}

/// Applies the changes of a `textDocument/didChange` notification in order:
/// each range refers to the text as the previous changes left it.
fn apply_changes(text: &mut String, changes: &[Value]) {
    for change in changes {
        let replacement = change["text"].as_str().unwrap_or("");
        match change.get("range") {
            Some(range) if !range.is_null() => {
                let start = protocol::offset(text, &range["start"]) as usize;
                let end = protocol::offset(text, &range["end"]) as usize;
                let start = start.min(text.len());
                let end = end.min(text.len());
                let (start, end) = if start <= end {
                    (start, end)
                } else {
                    (end, start)
                };
                text.replace_range(start..end, replacement);
            }
            _ => replacement.clone_into(text),
        }
    }
}

#[cfg(test)]
mod tests;
