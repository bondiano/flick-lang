//! Requests: hover, definition, references, completion, rename and
//! formatting, answered from the current analysis; the text of built-in
//! modules as read-only documents.

use std::fs;
use std::path::{Path, PathBuf};

use flick_core::fmt;
use flick_core::ide::{CompletionKind, Ide, Location, Target};
use flick_core::source::FileId;
use flick_core::types::builtin_sources;
use serde_json::{Value, json};

use super::{Server, Snapshot, analyze, protocol};

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
            "workspace/textDocumentContent" | "flick/builtinSource" => {
                let uri = params["uri"].as_str().unwrap_or("");
                let module = uri
                    .strip_prefix("flick-builtin:///")
                    .and_then(|m| m.strip_suffix(".fl"))
                    .ok_or((INVALID_PARAMS, format!("`{uri}` is not a built-in module")))?;
                let (_, text) = builtin_sources()
                    .iter()
                    .find(|(n, _)| *n == module)
                    .ok_or((INVALID_PARAMS, format!("no built-in module `{module}`")))?;
                Ok(json!({ "text": text }))
            }
            "textDocument/formatting" => {
                let (_, text) = self.document(params)?;
                let Ok(formatted) = fmt::format(&text, FileId::default()) else {
                    return Ok(Value::Null);
                };
                let end = u32::try_from(text.len()).unwrap_or(u32::MAX);
                Ok(json!([{ "range": protocol::range(&text, 0, end), "newText": formatted }]))
            }
            "textDocument/completion" => self.completion(params),
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
            "textDocument/definition" => Ok(ide
                .symbol_at(file, offset)
                .and_then(|(symbol, _)| ide.definition(&symbol))
                .map_or(Value::Null, |l| location(snapshot, &l))),
            "textDocument/references" => {
                let Some((symbol, _)) = ide.symbol_at(file, offset) else {
                    return Ok(json!([]));
                };
                let refs: Vec<Value> = ide
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
                Ok(Value::Array(refs))
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

    /// Completions; after a `.` the incomplete member is cut out so the
    /// module parses, and the text before the dot is analyzed.
    fn completion(&mut self, params: &Value) -> Reply {
        let (path, text) = self.document(params)?;
        let cursor = protocol::offset(&text, &params["position"]) as usize;
        let word = text[..cursor]
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_ascii_alphanumeric() || *c == '_')
            .last()
            .map_or(cursor, |(i, _)| i);
        let dotted = text[..word].ends_with('.');
        let (snapshot, offset) = if dotted {
            let dot = word - 1;
            let mut docs = self.docs.clone();
            docs.insert(path.clone(), format!("{}{}", &text[..dot], &text[cursor..]));
            (analyze(&docs, &path), dot)
        } else {
            let mut docs = self.docs.clone();
            docs.insert(path.clone(), text.clone());
            (analyze(&docs, &path), word)
        };
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
                };
                json!({ "label": c.label, "kind": kind, "detail": c.detail })
            })
            .collect();
        Ok(Value::Array(items))
    }
}

/// An LSP location of a declaration.
fn location(snapshot: &Snapshot, l: &Location) -> Value {
    let (uri, text) = match &l.target {
        Target::File(f) => {
            let source = snapshot.sources.get(*f);
            (
                protocol::uri_of(Path::new(&source.path)),
                source.text.clone(),
            )
        }
        Target::Builtin(module) => {
            let text = builtin_sources()
                .iter()
                .find(|(n, _)| n == module)
                .map_or("", |(_, t)| *t);
            (protocol::builtin_uri(module), text.to_owned())
        }
        Target::Path(p) => (
            protocol::uri_of(p),
            fs::read_to_string(p).unwrap_or_default(),
        ),
    };
    json!({ "uri": uri, "range": protocol::range(&text, l.start, l.end) })
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
            "documentFormattingProvider": true,
            "workspace": { "textDocumentContent": { "schemes": ["flick-builtin"] } },
        },
        "serverInfo": { "name": "flick", "version": env!("CARGO_PKG_VERSION") },
    })
}
