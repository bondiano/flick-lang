//! Requests about a whole document or workspace: outline, semantic tokens,
//! folding, inlay hints, signature help and workspace symbols.

use std::path::Path;

use flick_core::ide::{Ide, Outline};
use flick_core::source::{FileId, SourceFile};
use serde_json::{Value, json};

use super::requests::ide;
use super::{Snapshot, protocol};

pub(super) fn document_symbols(ide: &Ide, file: FileId) -> Value {
    fn symbol(source: &SourceFile, o: &Outline) -> Value {
        json!({
            "name": o.name,
            "kind": o.kind,
            "range": protocol::range_in(source, o.range.0, o.range.1),
            "selectionRange": protocol::range_in(source, o.selection.0, o.selection.1),
            "children": o.children.iter().map(|c| symbol(source, c)).collect::<Vec<_>>(),
        })
    }
    let source = ide.sources.get(file);
    let symbols: Vec<Value> = ide
        .document_symbols(file)
        .iter()
        .map(|o| symbol(source, o))
        .collect();
    Value::Array(symbols)
}

/// `textDocument/semanticTokens/full`: five numbers per token, each
/// position relative to the token before.
pub(super) fn semantic_tokens(ide: &Ide, file: FileId) -> Value {
    let source = ide.sources.get(file);
    let mut data: Vec<u64> = Vec::new();
    let (mut line, mut column) = (0, 0);
    for (start, end, kind) in ide.semantic_tokens(file) {
        let (l, c) = source.utf16_position(start);
        let (l, c) = (u64::from(l), u64::from(c));
        let token = &source.text[start as usize..end as usize];
        let length = token.encode_utf16().count() as u64;
        let delta_column = if l == line { c - column } else { c };
        data.extend([l - line, delta_column, length, u64::from(kind), 0]);
        (line, column) = (l, c);
    }
    json!({ "data": data })
}

pub(super) fn folding_ranges(ide: &Ide, file: FileId) -> Value {
    let source = ide.sources.get(file);
    let ranges: Vec<Value> = ide
        .folding_ranges(file)
        .into_iter()
        .map(|(s, e)| (source.utf16_position(s).0, source.utf16_position(e).0))
        .filter(|(s, e)| s < e)
        .map(|(s, e)| json!({ "startLine": s, "endLine": e }))
        .collect();
    Value::Array(ranges)
}

pub(super) fn inlay_hints(ide: &Ide, file: FileId) -> Value {
    let source = ide.sources.get(file);
    let hints: Vec<Value> = ide
        .inlay_hints(file)
        .into_iter()
        .map(|(at, label)| json!({ "position": protocol::position_in(source, at), "label": label, "kind": 1 }))
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
