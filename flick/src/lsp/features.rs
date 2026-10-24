//! Requests about a whole document or workspace: outline, semantic tokens,
//! folding, inlay hints, signature help and workspace symbols.

use std::path::Path;

use flick_core::ide::{Ide, Outline};
use flick_core::source::FileId;
use serde_json::{Value, json};

use super::requests::{ide, package_dir};
use super::{Server, protocol};

/// The line and UTF-16 column of `offset`.
fn line_column(text: &str, offset: u32) -> (u64, u64) {
    let p = protocol::position(text, offset);
    (
        p["line"].as_u64().unwrap_or(0),
        p["character"].as_u64().unwrap_or(0),
    )
}

pub(super) fn document_symbols(ide: &Ide, text: &str, file: FileId) -> Value {
    fn symbol(text: &str, o: &Outline) -> Value {
        json!({
            "name": o.name,
            "kind": o.kind,
            "range": protocol::range(text, o.range.0, o.range.1),
            "selectionRange": protocol::range(text, o.selection.0, o.selection.1),
            "children": o.children.iter().map(|c| symbol(text, c)).collect::<Vec<_>>(),
        })
    }
    let symbols: Vec<Value> = ide
        .document_symbols(file)
        .iter()
        .map(|o| symbol(text, o))
        .collect();
    Value::Array(symbols)
}

/// `textDocument/semanticTokens/full`: five numbers per token, each
/// position relative to the token before.
pub(super) fn semantic_tokens(ide: &Ide, text: &str, file: FileId) -> Value {
    let mut data: Vec<u64> = Vec::new();
    let (mut line, mut column) = (0, 0);
    for (start, end, kind) in ide.semantic_tokens(file) {
        let (l, c) = line_column(text, start);
        let length = text[start as usize..end as usize].encode_utf16().count() as u64;
        let delta_column = if l == line { c - column } else { c };
        data.extend([l - line, delta_column, length, u64::from(kind), 0]);
        (line, column) = (l, c);
    }
    json!({ "data": data })
}

pub(super) fn folding_ranges(ide: &Ide, text: &str, file: FileId) -> Value {
    let ranges: Vec<Value> = ide
        .folding_ranges(file)
        .into_iter()
        .map(|(s, e)| (line_column(text, s).0, line_column(text, e).0))
        .filter(|(s, e)| s < e)
        .map(|(s, e)| json!({ "startLine": s, "endLine": e }))
        .collect();
    Value::Array(ranges)
}

pub(super) fn inlay_hints(ide: &Ide, text: &str, file: FileId) -> Value {
    let hints: Vec<Value> = ide
        .inlay_hints(file)
        .into_iter()
        .map(|(at, label)| json!({ "position": protocol::position(text, at), "label": label, "kind": 1 }))
        .collect();
    Value::Array(hints)
}

pub(super) fn signature_help(ide: &Ide, file: FileId, offset: u32) -> Value {
    let Some(help) = ide.signature_help(file, offset) else {
        return Value::Null;
    };
    // parameter labels as UTF-16 offsets into the label
    let utf16 = |at: u32| help.label[..at as usize].encode_utf16().count();
    let params: Vec<Value> = help
        .params
        .iter()
        .map(|(s, e)| json!({ "label": [utf16(*s), utf16(*e)] }))
        .collect();
    json!({
        "signatures": [{ "label": help.label, "parameters": params }],
        "activeSignature": 0,
        "activeParameter": help.active,
    })
}

impl Server {
    /// `workspace/symbol` over the latest analysis; none before one exists.
    pub(super) fn workspace_symbols(&self, params: &Value) -> Value {
        let Some(snapshot) = &self.snapshot else {
            return json!([]);
        };
        let query = params["query"].as_str().unwrap_or("");
        let dir = package_dir(&snapshot.root);
        let ide = ide(snapshot, &dir);
        let symbols: Vec<Value> = ide
            .workspace_symbols(query)
            .into_iter()
            .map(|(file, o)| {
                let source = snapshot.sources.get(file);
                json!({
                    "name": o.name,
                    "kind": o.kind,
                    "location": {
                        "uri": protocol::uri_of(Path::new(&source.path)),
                        "range": protocol::range(&source.text, o.selection.0, o.selection.1),
                    },
                })
            })
            .collect();
        Value::Array(symbols)
    }
}
