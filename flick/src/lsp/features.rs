//! Requests about a whole document or workspace: outline, semantic tokens,
//! folding, inlay hints, signature help and workspace symbols.

use std::path::Path;

use flick_core::ide::{Ide, Outline};
use flick_core::source::FileId;
use serde_json::{Value, json};

use super::Snapshot;
use super::protocol::{self, Lines};
use super::requests::ide;

pub(super) fn document_symbols(ide: &Ide, text: &str, file: FileId) -> Value {
    fn symbol(lines: &Lines, o: &Outline) -> Value {
        json!({
            "name": o.name,
            "kind": o.kind,
            "range": lines.range(o.range.0, o.range.1),
            "selectionRange": lines.range(o.selection.0, o.selection.1),
            "children": o.children.iter().map(|c| symbol(lines, c)).collect::<Vec<_>>(),
        })
    }
    let lines = Lines::of(text);
    let symbols: Vec<Value> = ide
        .document_symbols(file)
        .iter()
        .map(|o| symbol(&lines, o))
        .collect();
    Value::Array(symbols)
}

/// `textDocument/semanticTokens/full`: five numbers per token, each
/// position relative to the token before.
pub(super) fn semantic_tokens(ide: &Ide, text: &str, file: FileId) -> Value {
    let mut data: Vec<u64> = Vec::new();
    let lines = Lines::of(text);
    let (mut line, mut column) = (0, 0);
    for (start, end, kind) in ide.semantic_tokens(file) {
        let (l, c) = lines.line_column(start);
        let length = text[start as usize..end as usize].encode_utf16().count() as u64;
        let delta_column = if l == line { c - column } else { c };
        data.extend([l - line, delta_column, length, u64::from(kind), 0]);
        (line, column) = (l, c);
    }
    json!({ "data": data })
}

pub(super) fn folding_ranges(ide: &Ide, text: &str, file: FileId) -> Value {
    let lines = Lines::of(text);
    let ranges: Vec<Value> = ide
        .folding_ranges(file)
        .into_iter()
        .map(|(s, e)| (lines.line_column(s).0, lines.line_column(e).0))
        .filter(|(s, e)| s < e)
        .map(|(s, e)| json!({ "startLine": s, "endLine": e }))
        .collect();
    Value::Array(ranges)
}

pub(super) fn inlay_hints(ide: &Ide, text: &str, file: FileId) -> Value {
    let lines = Lines::of(text);
    let hints: Vec<Value> = ide
        .inlay_hints(file)
        .into_iter()
        .map(|(at, label)| json!({ "position": lines.position(at), "label": label, "kind": 1 }))
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

/// The symbols of `snapshot` that match `query`, as LSP symbol information.
pub(super) fn symbols(snapshot: &Snapshot, query: &str) -> Vec<Value> {
    let ide = ide(snapshot);
    ide.workspace_symbols(query)
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
        .collect()
}
