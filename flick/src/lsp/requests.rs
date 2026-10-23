//! Requests: hover, definition, references, completion, rename, quick
//! fixes and formatting, answered from the current analysis; built-in
//! modules are materialized as read-only `.fl` files for every editor.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::process;

use flick_core::fmt;
use flick_core::ide::{CompletionKind, Ide, Location, Target};
use flick_core::source::FileId;
use flick_core::syntax::{self, cst::Node, kind::SyntaxKind as K, lexer};
use flick_core::types::builtin_sources;
use serde_json::{Value, json};

use super::{BUILTIN_DIR, Server, Snapshot, analyze, builtin_index, lsp_diagnostic, protocol};

type Reply = Result<Value, (i64, String)>;

const INVALID_PARAMS: i64 = -32602;
const REQUEST_FAILED: i64 = -32803;

/// The directory of an installed package of the project at `root`.
fn package_dir(root: &Path) -> impl Fn(&str) -> Option<PathBuf> + '_ {
    move |package| {
        let dir = root.join("node_modules").join(package);
        dir.exists().then_some(dir)
    }
}

fn ide<'a>(snapshot: &'a Snapshot, dir: &'a dyn Fn(&str) -> Option<PathBuf>) -> Ide<'a> {
    Ide {
        analysis: &snapshot.analysis,
        sources: &snapshot.sources,
        package_dir: dir,
    }
}

impl Server {
    pub(super) fn request(&mut self, method: &str, params: &Value) -> Reply {
        match method {
            "initialize" => Ok(capabilities()),
            "textDocument/formatting" => {
                let (_, text) = self.document(params)?;
                let Ok(formatted) = fmt::format(&text, FileId::default()) else {
                    return Ok(Value::Null);
                };
                let end = u32::try_from(text.len()).unwrap_or(u32::MAX);
                Ok(json!([{ "range": protocol::range(&text, 0, end), "newText": formatted }]))
            }
            "textDocument/completion" => self.completion(params),
            "textDocument/codeAction" => self.code_actions(params),
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

    fn snapshot_for(&mut self, path: &Path) -> &Snapshot {
        let fresh = self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.file(path).is_some());
        if !fresh {
            self.snapshot = Some(analyze(&self.docs, path));
        }
        self.snapshot.as_ref().expect("analyzed")
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
        let snapshot = self.snapshot_for(&path);
        let Some(file) = snapshot.file(&path) else {
            return Ok(Value::Null);
        };
        let dir = package_dir(&snapshot.root);
        let ide = ide(snapshot, &dir);
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
            "textDocument/definition" => ide
                .symbol_at(file, offset)
                .and_then(|(symbol, _)| ide.definition(&symbol))
                .map_or(Ok(Value::Null), |l| location(snapshot, &l)),
            "textDocument/references" => {
                let Some((symbol, _)) = ide.symbol_at(file, offset) else {
                    return Ok(json!([]));
                };
                let refs: Result<Vec<Value>, _> = ide
                    .references(&symbol)
                    .into_iter()
                    .map(|(f, (s, e))| {
                        location(
                            snapshot,
                            &Location {
                                target: Target::File(f),
                                start: s,
                                end: e,
                            },
                        )
                    })
                    .collect();
                Ok(Value::Array(refs?))
            }
            "textDocument/prepareRename" => {
                let (_, (s, e), _) = ide
                    .renamable(file, offset)
                    .map_err(|e| (REQUEST_FAILED, e))?;
                Ok(protocol::range(&text, s, e))
            }
            "textDocument/rename" => {
                let new_name = params["newName"].as_str().unwrap_or("");
                let edits = ide
                    .rename(file, offset, new_name)
                    .map_err(|e| (REQUEST_FAILED, e))?;
                let mut changes = serde_json::Map::new();
                for (f, (s, e)) in edits {
                    let source = snapshot.sources.get(f);
                    let uri = protocol::uri_of(Path::new(&source.path));
                    let edit = json!({ "range": protocol::range(&source.text, s, e), "newText": new_name });
                    changes
                        .entry(uri)
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .expect("array")
                        .push(edit);
                }
                Ok(json!({ "changes": changes }))
            }
            _ => Err((-32601, format!("`{method}` is not supported"))),
        }
    }

    /// Quick fixes: the edits of the fixes of the diagnostics in the range.
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
                let mut changes = serde_json::Map::new();
                for edit in &fix.edits {
                    let source = snapshot.sources.get(edit.span.file);
                    let range = protocol::range(&source.text, edit.span.start, edit.span.end);
                    changes
                        .entry(protocol::uri_of(Path::new(&source.path)))
                        .or_insert_with(|| json!([]))
                        .as_array_mut()
                        .expect("array")
                        .push(json!({ "range": range, "newText": edit.replacement }));
                }
                actions.push(json!({
                    "title": fix.message,
                    "kind": "quickfix",
                    "diagnostics": [lsp_diagnostic(&snapshot.sources, d)],
                    "isPreferred": true,
                    "edit": { "changes": changes },
                }));
            }
        }
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
        let (mut snapshot, offset) = if dotted {
            let dot = word - 1;
            let mut docs = self.docs.clone();
            docs.insert(path.clone(), format!("{}{}", &text[..dot], &text[cursor..]));
            (analyze(&docs, &path), dot)
        } else {
            let mut docs = self.docs.clone();
            docs.insert(path.clone(), text.clone());
            (analyze(&docs, &path), word)
        };
        if snapshot.file(&path).is_none()
            && let Some(replacement) = completion_recovery(&text, word, cursor)
        {
            let mut docs = self.docs.clone();
            docs.insert(
                path.clone(),
                format!("{}{replacement}{}", &text[..word], &text[cursor..]),
            );
            snapshot = analyze(&docs, &path);
        }
        let Some(file) = snapshot.file(&path) else {
            return Ok(json!([]));
        };
        let dir = package_dir(&snapshot.root);
        let items: Vec<Value> = ide(&snapshot, &dir)
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
                json!({ "label": c.label, "kind": kind, "detail": c.detail })
            })
            .collect();
        Ok(Value::Array(items))
    }
}

/// An LSP location of a declaration.
fn location(snapshot: &Snapshot, l: &Location) -> Reply {
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
        K::String => {
            t.start < cursor && (cursor < t.end || !closed(&text[t.start as usize..t.end as usize]))
        }
        _ => false,
    })
}

/// Whether string literal text has its closing quote.
fn closed(literal: &str) -> bool {
    literal.len() > 1 && literal.trim_end_matches('#').ends_with('"')
}

/// Like removing an unfinished `.member`, supply syntax for the expression
/// or clause currently being typed; never change the open buffer.
fn completion_recovery(text: &str, offset: usize, cursor: usize) -> Option<&'static str> {
    fn in_sql(node: &Node, kind: K, offset: u32) -> bool {
        let (start, end) = node.range();
        start <= offset
            && offset <= end
            && (node.kind == kind || node.nodes().any(|n| in_sql(n, kind, offset)))
    }
    let tree = syntax::parse(text, FileId::default()).tree;
    let tokens = tree.tokens();
    let previous = tokens
        .iter()
        .rev()
        .find(|t| !t.kind.is_trivia() && t.end as usize <= offset)?;
    let at = u32::try_from(offset).ok()?;
    // a column of a `sql table` that ends in `as` or `via`: a type or a codec
    if in_sql(&tree, K::SqlTableDecl, previous.start) {
        return match (previous.kind, previous.text(text)) {
            (K::AsKw, _) => Some("FlickCompletion"),
            (K::Ident, "via") => Some("__flick_completion"),
            _ => None,
        };
    }
    if !in_sql(&tree, K::SqlExpr, at) {
        return None;
    }
    let next = tokens
        .iter()
        .find(|t| !t.kind.is_trivia() && t.start as usize >= cursor);
    let select = if next.is_some_and(|t| t.kind == K::LBrace) {
        "select"
    } else {
        "select {}"
    };
    Some(match previous.kind {
        K::FromKw => "__flick_completion in __flick_completion",
        K::Ident if matches!(previous.text(text), "join" | "left_join") => {
            "__flick_completion in __flick_completion"
        }
        K::InKw | K::Eq | K::Comma | K::Dot | K::LParen => "__flick_completion",
        K::LBrace => select,
        _ if text[..offset].rsplit('\n').next()?.trim().is_empty() => select,
        _ => "__flick_completion",
    })
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
            "codeActionProvider": { "codeActionKinds": ["quickfix"] },
            "documentFormattingProvider": true,
        },
        "serverInfo": { "name": "flick", "version": env!("CARGO_PKG_VERSION") },
    })
}
