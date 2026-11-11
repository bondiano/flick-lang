//! Requests: hover, definition, references, completion, rename, quick
//! fixes and formatting, answered from the current analysis; built-in
//! modules are materialized as read-only `.fl` files for every editor.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;

use flick_core::diagnostics::Edit;
use flick_core::fmt;
use flick_core::ide::{CompletionKind, Ide, Location, TOKEN_TYPES, Target};
use flick_core::plugin::Expander;
use flick_core::source::{FileId, SourceMap};
use flick_core::syntax::{kind::SyntaxKind as K, lexer};
use flick_core::types::builtin_sources;
use serde_json::{Value, json};

use super::{
    BUILTIN_DIR, Context, Server, Snapshot, analyze, builtin_index, features, lsp_diagnostic,
    protocol,
};

#[path = "ffi.rs"]
mod ffi;
#[path = "foreign.rs"]
mod foreign;

pub(super) type Reply = Result<Value, (i64, String)>;

const INVALID_PARAMS: i64 = -32602;
pub(super) const REQUEST_FAILED: i64 = -32803;

pub(super) fn ide(snapshot: &Snapshot) -> Ide<'_> {
    Ide {
        analysis: &snapshot.analysis,
        sources: &snapshot.sources,
    }
}

impl Server {
    pub(super) fn request(&mut self, method: &str, params: &Value) -> Reply {
        match method {
            "initialize" => {
                self.registered = super::registration::registrations(params);
                Ok(capabilities())
            }
            "textDocument/formatting" => {
                let (path, text) = self.document(params)?;
                // the plugins of the module's blocks, as the analysis loaded them
                let snapshot = self.snapshot_for(&path);
                let plugins: HashMap<String, Arc<dyn Expander>> = (snapshot.file(&path))
                    .and_then(|file| snapshot.analysis.modules.iter().find(|m| m.file == file))
                    .map(|m| m.editing.iter())
                    .into_iter()
                    .flatten()
                    .map(|b| (b.block.head.clone(), Arc::clone(&b.plugin)))
                    .collect();
                let plugin_of = |head: &str| plugins.get(head).cloned();
                let Ok(formatted) = fmt::format_with(&text, FileId::default(), &plugin_of) else {
                    return Ok(Value::Null);
                };
                let end = u32::try_from(text.len()).unwrap_or(u32::MAX);
                Ok(json!([{ "range": protocol::range(&text, 0, end), "newText": formatted }]))
            }
            "textDocument/completion" => self.completion(params),
            "textDocument/codeAction" => self.code_actions(params),
            "workspace/symbol" => Ok(self.workspace_symbols(params)),
            _ => self.at_position(method, params),
        }
    }

    /// The path and current text of the request's document.
    fn document(&self, params: &Value) -> Result<(PathBuf, String), (i64, String)> {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        let path =
            protocol::path_of(uri).ok_or((INVALID_PARAMS, format!("not a file: `{uri}`")))?;
        let text = match self.docs.get(&path) {
            Some(t) => t.clone(),
            None => fs::read_to_string(&path).map_err(|e| (REQUEST_FAILED, e.to_string()))?,
        };
        Ok((path, text))
    }

    pub(super) fn snapshot_for(&mut self, path: &Path) -> &Snapshot {
        let context = Context::of(&self.docs, path);
        let kept = self.snapshots.get(&context);
        let fresh = kept.is_some_and(|s| s.file(path).is_some());
        tracing::debug!(cached = fresh, "selecting analysis snapshot");
        if !fresh {
            let snapshot = analyze(&self.docs, path, &mut self.caches);
            self.snapshots.insert(context.clone(), snapshot);
        }
        self.latest = Some(context.clone());
        &self.snapshots[&context]
    }

    fn at_position(&mut self, method: &str, params: &Value) -> Reply {
        let (path, text) = self.document(params)?;
        if matches!(method, "textDocument/prepareRename" | "textDocument/rename")
            && builtin_index(&path).is_some()
        {
            return Err((
                REQUEST_FAILED,
                "built-in module sources are read-only".into(),
            ));
        }
        let offset = protocol::offset(&text, &params["position"]);
        match method {
            "textDocument/references" => return self.references(&path, offset),
            "textDocument/rename" => {
                return self.rename(&path, offset, params["newName"].as_str().unwrap_or(""));
            }
            _ => {}
        }
        let snapshot = self.snapshot_for(&path);
        let Some(file) = snapshot.file(&path) else {
            return Ok(Value::Null);
        };
        let ide = ide(snapshot);
        match method {
            "textDocument/hover" => {
                Ok(ide
                    .hover(file, offset)
                    .map_or(Value::Null, |(markdown, (s, e))| {
                        json!({
                            "contents": { "kind": "markdown", "value": markdown },
                            "range": protocol::range(&text, s, e),
                        })
                    }))
            }
            "textDocument/documentSymbol" => Ok(features::document_symbols(&ide, file)),
            "textDocument/semanticTokens/full" => Ok(features::semantic_tokens(&ide, file)),
            "textDocument/foldingRange" => Ok(features::folding_ranges(&ide, file)),
            "textDocument/inlayHint" => Ok(features::inlay_hints(&ide, file)),
            "textDocument/signatureHelp" => Ok(features::signature_help(&ide, file, offset)),
            "textDocument/typeDefinition" => ide
                .type_definition(file, offset)
                .map_or(Ok(Value::Null), |l| location(snapshot, &l)),
            "textDocument/definition" => ide
                .field_definition(file, offset)
                .or_else(|| foreign::definition(snapshot, file, offset))
                .or_else(|| {
                    ide.symbol_at(file, offset)
                        .and_then(|(symbol, _)| ide.definition(&symbol))
                })
                .map_or(Ok(Value::Null), |l| location(snapshot, &l)),
            "textDocument/prepareRename" => {
                let (_, (s, e), _) = ide
                    .renamable(file, offset)
                    .map_err(|e| (REQUEST_FAILED, e))?;
                Ok(protocol::range(&text, s, e))
            }
            _ => Err((-32601, format!("`{method}` is not supported"))),
        }
    }

    /// Quick fixes, the edits of the fixes of the diagnostics in the range,
    /// then the rewrites the range offers, then the regeneration of an
    /// imported module.
    fn code_actions(&mut self, params: &Value) -> Reply {
        let (path, text) = self.document(params)?;
        let start = protocol::offset(&text, &params["range"]["start"]);
        let end = protocol::offset(&text, &params["range"]["end"]);
        let snapshot = self.snapshot_for(&path);
        let Some(file) = snapshot.file(&path) else {
            return Ok(json!([]));
        };
        let mut actions = Vec::new();
        for d in &snapshot.analysis.diagnostics {
            let Some(span) = d.span.filter(|s| s.file == file) else {
                continue;
            };
            if span.end < start || end < span.start {
                continue;
            }
            for fix in d.fixes.iter().filter(|f| !f.edits.is_empty()) {
                actions.push(json!({
                    "title": fix.message,
                    "kind": "quickfix",
                    "diagnostics": [lsp_diagnostic(&snapshot.sources, d)],
                    "isPreferred": true,
                    "edit": { "changes": changes(&snapshot.sources, &fix.edits) },
                }));
            }
        }
        for action in ide(snapshot).actions(file, start, end) {
            actions.push(json!({
                "title": action.title,
                "kind": action.kind,
                "edit": { "changes": changes(&snapshot.sources, &action.edits) },
            }));
        }
        actions.extend(ffi::regenerate(&path, &text, params));
        Ok(Value::Array(actions))
    }

    /// Completions; after a `.` the incomplete member is cut out so the
    /// module parses, and the text before the dot is analyzed.
    fn completion(&mut self, params: &Value) -> Reply {
        let (path, text) = self.document(params)?;
        let cursor = protocol::offset(&text, &params["position"]) as usize;
        if in_comment_or_string(&text, cursor) {
            return Ok(json!([]));
        }
        let word = text[..cursor]
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_')
            .last()
            .map_or(cursor, |(i, _)| i);
        let dotted = text[..word].ends_with('.');
        let cut;
        let (snapshot, offset) = if dotted {
            let dot = word - 1;
            let mut docs = self.docs.clone();
            docs.insert(path.clone(), format!("{}{}", &text[..dot], &text[cursor..]));
            cut = analyze(&docs, &path, &mut self.caches);
            (&cut, dot)
        } else if self.docs.contains_key(&path) {
            // the text of an open document is what its last change analyzed
            (self.snapshot_for(&path), word)
        } else {
            let mut docs = self.docs.clone();
            docs.insert(path.clone(), text.clone());
            cut = analyze(&docs, &path, &mut self.caches);
            (&cut, word)
        };
        let Some(file) = snapshot.file(&path) else {
            return Ok(json!([]));
        };
        let items: Vec<Value> = ide(snapshot)
            .complete(file, u32::try_from(offset).unwrap_or(0), dotted)
            .into_iter()
            .map(|c| {
                let kind = match c.kind {
                    CompletionKind::Local => 6,
                    CompletionKind::Value => 3,
                    CompletionKind::Constructor => 4,
                    CompletionKind::Type => 7,
                    CompletionKind::Module => 9,
                    CompletionKind::Field => 5,
                    CompletionKind::Operation => 2,
                    CompletionKind::Keyword => 14,
                };
                let mut item = json!({
                    "label": c.label,
                    "kind": kind,
                    "detail": c.detail,
                    "sortText": format!("{}{}", c.sort, c.label),
                });
                if let Some(doc) = c.documentation {
                    item["documentation"] = json!({ "kind": "markdown", "value": doc });
                }
                if let Some(snippet) = c.snippet {
                    item["insertText"] = json!(snippet);
                    item["insertTextFormat"] = json!(2);
                }
                if !c.edits.is_empty() {
                    // imports go above the cursor, where both texts agree
                    let edits: Vec<Value> = c
                        .edits
                        .iter()
                        .map(|e| json!({ "range": protocol::range(&text, e.span.start, e.span.end), "newText": e.replacement }))
                        .collect();
                    item["additionalTextEdits"] = json!(edits);
                }
                item
            })
            .collect();
        tracing::debug!(items = items.len(), dotted, "completion ready");
        Ok(Value::Array(items))
    }
}

/// An LSP location of a declaration.
pub(super) fn location(snapshot: &Snapshot, l: &Location) -> Reply {
    let path;
    let text;
    let (path, text) = match &l.target {
        Target::File(f) => {
            let source = snapshot.sources.get(*f);
            (Path::new(&source.path), source.text.as_str())
        }
        Target::Builtin(module) => {
            let text = builtin_sources()
                .iter()
                .find(|(n, _)| n == module)
                .map(|(_, text)| *text)
                .ok_or((REQUEST_FAILED, format!("no built-in module `{module}`")))?;
            path = builtin_file(module, text).map_err(|e| {
                (
                    REQUEST_FAILED,
                    format!("cannot open built-in `{module}`: {e}"),
                )
            })?;
            (path.as_path(), text)
        }
        Target::Path(p) => {
            text = fs::read_to_string(p).map_err(|e| (REQUEST_FAILED, e.to_string()))?;
            (p.as_path(), text.as_str())
        }
    };
    Ok(json!({ "uri": protocol::uri_of(path), "range": protocol::range(text, l.start, l.end) }))
}

/// Content-addressed sources stay valid across server/compiler restarts.
fn builtin_file(module: &str, text: &str) -> io::Result<PathBuf> {
    let mut hash = DefaultHasher::new();
    text.hash(&mut hash);
    let path = BUILTIN_DIR
        .join(format!("{:016x}", hash.finish()))
        .join(module.replace('.', "/"))
        .with_extension("fl");
    if fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(path);
    }
    fs::create_dir_all(path.parent().expect("source directory"))?;
    let pending = path.with_extension(format!("{}.tmp", process::id()));
    fs::write(&pending, text)?;
    let mut permissions = fs::metadata(&pending)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&pending, permissions)?;
    fs::rename(&pending, &path)?;
    Ok(path)
}

/// Whether `cursor` is inside a comment, a doc comment or a string: prose,
/// where names of the program are not offered. The end of a line comment is
/// still inside it; a string's closing quote is not.
fn in_comment_or_string(text: &str, cursor: usize) -> bool {
    let Ok(cursor) = u32::try_from(cursor) else {
        return false;
    };
    let (tokens, _) = lexer::lex(text, FileId::default());
    tokens.iter().any(|t| match t.kind {
        K::LineComment | K::DocComment => t.start < cursor && cursor <= t.end,
        K::BlockComment => {
            t.start < cursor && (cursor < t.end || !text[..t.end as usize].ends_with("*/"))
        }
        K::String | K::StringEnd => {
            t.start < cursor && (cursor < t.end || !closed(&text[t.start as usize..t.end as usize]))
        }
        // the text before the `\{` of a hole
        K::StringStart | K::StringMid => t.start < cursor && cursor <= t.end - 2,
        _ => false,
    })
}

/// Whether string literal text has its closing quote.
fn closed(literal: &str) -> bool {
    literal.len() > 1 && literal.trim_end_matches('#').ends_with('"')
}

fn capabilities() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": { "openClose": true, "change": 1, "save": true },
            "hoverProvider": true,
            "definitionProvider": true,
            "referencesProvider": true,
            "completionProvider": { "triggerCharacters": ["."] },
            "renameProvider": { "prepareProvider": true },
            "codeActionProvider": {
                "codeActionKinds": ["quickfix", "refactor.rewrite", "refactor.extract", "refactor.inline"],
            },
            "documentFormattingProvider": true,
            "documentSymbolProvider": true,
            "workspaceSymbolProvider": true,
            "typeDefinitionProvider": true,
            "foldingRangeProvider": true,
            "inlayHintProvider": true,
            "signatureHelpProvider": { "triggerCharacters": [" "] },
            "semanticTokensProvider": {
                "legend": { "tokenTypes": TOKEN_TYPES, "tokenModifiers": [] },
                "full": true,
            },
        },
        "serverInfo": { "name": "flick", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// LSP `changes`: the edits by document URI.
fn changes(sources: &SourceMap, edits: &[Edit]) -> serde_json::Map<String, Value> {
    let mut changes = serde_json::Map::new();
    for edit in edits {
        let source = sources.get(edit.span.file);
        let range = protocol::range(&source.text, edit.span.start, edit.span.end);
        changes
            .entry(protocol::uri_of(Path::new(&source.path)))
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("array")
            .push(json!({ "range": range, "newText": edit.replacement }));
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::in_comment_or_string;

    #[test]
    fn the_text_of_a_string_with_holes_is_not_code() {
        let text = r#"let s = "ab \{name} cd \{x} ef""#;
        let at = |s: &str| text.find(s).expect("in text");
        assert!(in_comment_or_string(text, at("b ")));
        assert!(in_comment_or_string(text, at(" cd") + 2));
        assert!(in_comment_or_string(text, at(" ef") + 2));
        assert!(!in_comment_or_string(text, at("name") + 2));
        assert!(!in_comment_or_string(text, at("x}") + 1));
        assert!(!in_comment_or_string(text, text.len()));
    }
}
