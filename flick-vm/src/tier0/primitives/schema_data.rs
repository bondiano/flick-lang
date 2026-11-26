//! Counted ADT, record, tuple, XML, email, and `Differ.json_patch` helpers for `Schema`/`Codec`.

use std::mem;

use flick_core::ir::Lit;

use super::json_patch_ops::{PatchOp, apply_one, collect_diff, drop_ops, record_reps};
use super::list_ops::{dup_reps, slice_reps};
use super::record_ops::put_js_key;
use super::schema_ops::{is_json_null, js_string_value};
use super::{Data, Error, Heap, Value, charge};

pub(super) fn tag_of(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("ADT constructor"));
    };
    let Data::Tagged { tag, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("ADT constructor"));
    };
    let full = tag.clone();
    let short = full.rsplit('.').next().unwrap_or(&full).to_owned();
    let f = heap.literal(&Lit::String(full))?;
    let s = match heap.literal(&Lit::String(short)) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(f)?;
            return Err(err);
        }
    };
    heap.construct("$tuple", vec![f, s])
}

pub(super) fn deconstruct_adt(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let Value::Object(tok) = val else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let (tag, reps) = match &heap.object(tok.id)?.data {
        Data::Tagged {
            tag,
            ordinary_fields,
            fields,
            ..
        } if tag != "$list" && tag != "$tuple" => (
            tag.clone(),
            fields[..*ordinary_fields]
                .iter()
                .map(Value::representation)
                .collect::<Vec<_>>(),
        ),
        _ => return heap.construct("Prelude.Option.None", vec![]),
    };
    charge(fuel, reps.len())?;
    let duped = dup_reps(heap, &reps)?;
    let list = heap.construct("$list", duped)?;
    let tag_val = match heap.literal(&Lit::String(tag)) {
        Ok(v) => v,
        Err(err) => {
            heap.drop_value(list)?;
            return Err(err);
        }
    };
    let pair = heap.construct("$tuple", vec![tag_val, list])?;
    heap.construct("Prelude.Option.Some", vec![pair])
}

pub(super) fn construct_adt(
    heap: &mut Heap,
    tag_val: &Value,
    fields_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let tag = heap.string_lossy(tag_val)?;
    let reps = slice_reps(heap, fields_val)?;
    charge(fuel, reps.len())?;
    let duped = dup_reps(heap, &reps)?;
    heap.construct(&tag, duped)
}

pub(super) fn record_entries(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let Value::Object(tok) = val else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let pairs: Vec<(String, Value)> = match &heap.object(tok.id)?.data {
        Data::Record(fields) => fields
            .iter()
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect(),
        _ => return heap.construct("Prelude.Option.None", vec![]),
    };
    charge(fuel, pairs.len())?;
    let mut out = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        let kv = match heap.literal(&Lit::String(k)) {
            Ok(x) => x,
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        };
        let vv = match heap.dup(&v) {
            Ok(x) => x,
            Err(err) => {
                heap.drop_value(kv)?;
                heap.drop_values(out)?;
                return Err(err);
            }
        };
        match heap.construct("$tuple", vec![kv, vv]) {
            Ok(pair) => out.push(pair),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    let list = heap.construct("$list", out)?;
    heap.construct("Prelude.Option.Some", vec![list])
}

pub(super) fn record_from_entries(
    heap: &mut Heap,
    entries_val: &Value,
    sorted: bool,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, entries_val)?;
    charge(fuel, reps.len())?;
    let mut fields: Vec<(String, Value)> = Vec::with_capacity(reps.len());
    for rep in &reps {
        let Value::Object(tok) = rep else {
            heap.drop_values(fields.into_iter().map(|(_, v)| v).collect())?;
            return Err(Error::PatternKind("tuple"));
        };
        let (k, v_rep) = match &heap.object(tok.id)?.data {
            Data::Tagged { fields: f, .. } if f.len() == 2 => {
                (heap.string_lossy(&f[0])?, f[1].representation())
            }
            _ => {
                heap.drop_values(fields.into_iter().map(|(_, v)| v).collect())?;
                return Err(Error::PatternKind("tuple"));
            }
        };
        let v = match heap.dup(&v_rep) {
            Ok(x) => x,
            Err(err) => {
                heap.drop_values(fields.into_iter().map(|(_, v)| v).collect())?;
                return Err(err);
            }
        };
        if sorted {
            if let Some((_, slot)) = fields.iter_mut().find(|(ek, _)| ek == &k) {
                let old = mem::replace(slot, v);
                heap.drop_value(old)?;
            } else {
                fields.push((k, v));
            }
        } else if let Some(old) = put_js_key(&mut fields, k, v) {
            heap.drop_value(old)?;
        }
    }
    if sorted {
        fields.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
    }
    heap.record(fields)
}

pub(super) fn record_lookup(
    heap: &mut Heap,
    obj: &Value,
    key_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let key = heap.string_lossy(key_val)?;
    charge(fuel, key.len())?;
    let Value::Object(tok) = obj else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let found = match &heap.object(tok.id)?.data {
        Data::Record(fields) => fields
            .iter()
            .find(|(k, _)| k == &key)
            .map(|(_, v)| v.representation()),
        _ => None,
    };
    match found {
        Some(v) => {
            let duped = heap.dup(&v)?;
            heap.construct("Prelude.Option.Some", vec![duped])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

pub(super) fn tuple_to_list(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    if matches!(val, Value::Unit) {
        let empty = heap.construct("$list", vec![])?;
        return heap.construct("Prelude.Option.Some", vec![empty]);
    }
    let Value::Object(tok) = val else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let reps = match &heap.object(tok.id)?.data {
        Data::Tagged { tag, fields, .. } if tag == "$tuple" => {
            fields.iter().map(Value::representation).collect::<Vec<_>>()
        }
        _ => return heap.construct("Prelude.Option.None", vec![]),
    };
    charge(fuel, reps.len())?;
    let duped = dup_reps(heap, &reps)?;
    let list = heap.construct("$list", duped)?;
    heap.construct("Prelude.Option.Some", vec![list])
}

pub(super) fn list_from_wire(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let Value::Object(tok) = val else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let is_list = matches!(&heap.object(tok.id)?.data, Data::ListSlice { .. })
        || matches!(&heap.object(tok.id)?.data, Data::Tagged { tag, .. } if tag == "$list");
    if !is_list {
        return heap.construct("Prelude.Option.None", vec![]);
    }
    let reps = slice_reps(heap, val)?;
    charge(fuel, reps.len())?;
    let duped = dup_reps(heap, &reps)?;
    let list = heap.construct("$list", duped)?;
    heap.construct("Prelude.Option.Some", vec![list])
}

pub(super) fn tuple_from_list(
    heap: &mut Heap,
    list_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    charge(fuel, reps.len())?;
    if reps.is_empty() {
        return Ok(Value::Unit);
    }
    let duped = dup_reps(heap, &reps)?;
    heap.construct("$tuple", duped)
}

pub(super) fn is_redacted(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    unwrap_single_tag(heap, val, "Redacted.Redacted.$Redacted")
}

pub(super) fn is_chunk(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    unwrap_single_tag(heap, val, "Chunk.Core.Chunk.$Chunk")
}

fn unwrap_single_tag(heap: &mut Heap, val: &Value, want: &str) -> Result<Value, Error> {
    let Value::Object(tok) = val else {
        return heap.construct("Prelude.Option.None", vec![]);
    };
    let inner = match &heap.object(tok.id)?.data {
        Data::Tagged { tag, fields, .. } if tag == want && fields.len() == 1 => {
            Some(fields[0].representation())
        }
        _ => None,
    };
    match inner {
        Some(v) => {
            let duped = heap.dup(&v)?;
            heap.construct("Prelude.Option.Some", vec![duped])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

pub(super) fn is_email(s: &str) -> bool {
    let is_sp = |c: char| c.is_whitespace() || c == '\u{FEFF}';
    let Some((local, rest)) = s.split_once('@') else {
        return false;
    };
    if local.is_empty()
        || local.chars().any(|c| c == '@' || is_sp(c))
        || rest.chars().any(|c| c == '@' || is_sp(c))
    {
        return false;
    }
    let Some((host, tld)) = rest.split_once('.') else {
        return false;
    };
    !host.is_empty() && !tld.is_empty()
}

fn xml_name(name: &str) -> String {
    let mut safe = String::new();
    let starts_ok = name
        .as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_');
    if !starts_ok {
        safe.push('_');
    }
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            safe.push(ch);
        } else {
            safe.push('_');
        }
    }
    if safe.len() >= 3 && safe[..3].eq_ignore_ascii_case("xml") {
        format!("_{safe}")
    } else {
        safe
    }
}

fn escape_xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn encode_xml(
    heap: &mut Heap,
    root_val: &Value,
    node_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let root = heap.string_lossy(root_val)?;
    let mut lines = Vec::new();
    xml_node(heap, &mut lines, &root, node_val, 0, "", fuel)?;
    heap.literal(&Lit::String(lines.join("\n")))
}

fn xml_node(
    heap: &Heap,
    lines: &mut Vec<String>,
    name: &str,
    node: &Value,
    depth: usize,
    original: &str,
    fuel: &mut u64,
) -> Result<(), Error> {
    charge(fuel, 1)?;
    let safe = xml_name(name);
    let attrs = if safe != name || (!original.is_empty() && original != name) {
        let orig = if original.is_empty() { name } else { original };
        let esc = escape_xml_text(orig).replace('"', "&quot;");
        format!(r#" data-name="{esc}""#)
    } else {
        String::new()
    };
    let indent = "  ".repeat(depth);
    if is_json_null(heap, node)? {
        lines.push(format!("{indent}<{safe}{attrs}/>"));
        return Ok(());
    }
    if let Value::Object(tok) = node {
        match &heap.object(tok.id)?.data {
            Data::ListSlice { .. } => {
                let reps = slice_reps(heap, node)?;
                return write_xml_list(heap, lines, &indent, &safe, &attrs, &reps, depth, fuel);
            }
            Data::Tagged { tag, fields, .. } if tag == "$list" => {
                return write_xml_list(heap, lines, &indent, &safe, &attrs, fields, depth, fuel);
            }
            Data::Record(fields) => {
                if fields.is_empty() {
                    lines.push(format!("{indent}<{safe}{attrs}/>"));
                    return Ok(());
                }
                lines.push(format!("{indent}<{safe}{attrs}>"));
                let mut sorted: Vec<(String, Value)> = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.representation()))
                    .collect();
                sorted.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
                for (k, v) in &sorted {
                    xml_node(heap, lines, &xml_name(k), v, depth + 1, k, fuel)?;
                }
                lines.push(format!("{indent}</{safe}>"));
                return Ok(());
            }
            _ => {}
        }
    }
    let text = escape_xml_text(&js_string_value(heap, node, fuel)?);
    lines.push(format!("{indent}<{safe}{attrs}>{text}</{safe}>"));
    Ok(())
}

#[expect(clippy::too_many_arguments, reason = "XML list formatting helper")]
fn write_xml_list(
    heap: &Heap,
    lines: &mut Vec<String>,
    indent: &str,
    safe: &str,
    attrs: &str,
    items: &[Value],
    depth: usize,
    fuel: &mut u64,
) -> Result<(), Error> {
    if items.is_empty() {
        lines.push(format!("{indent}<{safe}{attrs}/>"));
        return Ok(());
    }
    lines.push(format!("{indent}<{safe}{attrs}>"));
    for elem in items {
        xml_node(heap, lines, "item", elem, depth + 1, "", fuel)?;
    }
    lines.push(format!("{indent}</{safe}>"));
    Ok(())
}

pub(super) fn differ_patch_ops(
    heap: &mut Heap,
    old: &Value,
    new: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let mut ops = Vec::new();
    if let Err(err) = collect_diff(heap, old, new, "", &mut ops, fuel) {
        drop_ops(heap, ops)?;
        return Err(err);
    }
    let mut out = Vec::with_capacity(ops.len());
    for op in ops {
        let op_name = heap.literal(&Lit::String(op.kind_name().to_owned()));
        let path_val = heap.literal(&Lit::String(op.path().to_owned()));
        let rec = match (op_name, path_val, op) {
            (Ok(k), Ok(p), PatchOp::Remove { .. }) => {
                heap.record(vec![("op".into(), k), ("path".into(), p)])
            }
            (Ok(k), Ok(p), PatchOp::Add { value, .. } | PatchOp::Replace { value, .. }) => heap
                .record(vec![
                    ("op".into(), k),
                    ("path".into(), p),
                    ("value".into(), value),
                ]),
            (a, b, op) => {
                if let Ok(v) = a {
                    heap.drop_value(v)?;
                }
                if let Ok(v) = b {
                    heap.drop_value(v)?;
                }
                if let Some(v) = op.into_value() {
                    heap.drop_value(v)?;
                }
                heap.drop_values(out)?;
                return Err(Error::OutOfFuel);
            }
        };
        match rec {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

pub(super) fn differ_apply_patch(
    heap: &mut Heap,
    doc_val: &Value,
    patch_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let op_reps = slice_reps(heap, patch_val)?;
    charge(fuel, op_reps.len())?;
    let mut cur = heap.dup(doc_val)?;
    for op_rep in &op_reps {
        let Some(fields) = record_reps(heap, op_rep)? else {
            heap.drop_value(cur)?;
            return Err(Error::PatternKind("JsonPatch object"));
        };
        let op_str = fields
            .iter()
            .find(|(k, _)| k == "op")
            .and_then(|(_, v)| heap.string_lossy(v).ok())
            .unwrap_or_default();
        let path = fields
            .iter()
            .find(|(k, _)| k == "path")
            .and_then(|(_, v)| heap.string_lossy(v).ok())
            .unwrap_or_default();
        let val = fields
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| v.representation());
        let op = match op_str.as_str() {
            "remove" => PatchOp::Remove { path },
            "add" => PatchOp::Add {
                path,
                value: val.unwrap_or(Value::Unit),
            },
            "replace" => PatchOp::Replace {
                path,
                value: val.unwrap_or(Value::Unit),
            },
            other => {
                heap.drop_value(cur)?;
                let msg = heap.literal(&Lit::String(format!("Unsupported op: {other}")))?;
                return heap.construct("Prelude.Result.Err", vec![msg]);
            }
        };
        match apply_one(heap, cur, &op, fuel) {
            Ok(Ok(next)) => cur = next,
            Ok(Err(msg)) => {
                let s = heap.literal(&Lit::String(msg))?;
                return heap.construct("Prelude.Result.Err", vec![s]);
            }
            Err(err) => return Err(err),
        }
    }
    heap.construct("Prelude.Result.Ok", vec![cur])
}
