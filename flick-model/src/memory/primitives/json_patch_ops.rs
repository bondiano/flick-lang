//! Counted `JsonPatch` (RFC 6902) primitives over native JSON values.

use std::collections::BTreeSet;
use std::mem;

use flick_core::ir::Lit;
use flick_core::opt::json_string;

use super::json_pointer_ops::{escape_token, unescape_token};
use super::list_ops::{dup_reps, slice_reps};
use super::record_ops::put_js_key;
use super::{Data, Error, Heap, Value, charge};

pub(super) enum PatchOp {
    Add { path: String, value: Value },
    Remove { path: String },
    Replace { path: String, value: Value },
}

impl PatchOp {
    pub(super) fn kind_name(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Remove { .. } => "remove",
            Self::Replace { .. } => "replace",
        }
    }

    pub(super) fn path(&self) -> &str {
        match self {
            Self::Add { path, .. } | Self::Remove { path } | Self::Replace { path, .. } => path,
        }
    }

    pub(super) fn into_value(self) -> Option<Value> {
        match self {
            Self::Add { value, .. } | Self::Replace { value, .. } => Some(value),
            Self::Remove { .. } => None,
        }
    }
}

pub(super) fn drop_ops(heap: &mut Heap, ops: Vec<PatchOp>) -> Result<(), Error> {
    let vals: Vec<Value> = ops.into_iter().filter_map(PatchOp::into_value).collect();
    heap.drop_values(vals)
}

pub(super) fn patch_get(
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
        let built = match op {
            PatchOp::Remove { path } => {
                let p = heap.literal(&Lit::String(path))?;
                heap.construct("JsonPatch.Operation.Remove", vec![p])
            }
            PatchOp::Add { path, value } => {
                let p = match heap.literal(&Lit::String(path)) {
                    Ok(v) => v,
                    Err(err) => {
                        heap.drop_value(value)?;
                        heap.drop_values(out)?;
                        return Err(err);
                    }
                };
                heap.construct("JsonPatch.Operation.Add", vec![p, value])
            }
            PatchOp::Replace { path, value } => {
                let p = match heap.literal(&Lit::String(path)) {
                    Ok(v) => v,
                    Err(err) => {
                        heap.drop_value(value)?;
                        heap.drop_values(out)?;
                        return Err(err);
                    }
                };
                heap.construct("JsonPatch.Operation.Replace", vec![p, value])
            }
        };
        match built {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

pub(super) fn collect_diff(
    heap: &mut Heap,
    old: &Value,
    new: &Value,
    path: &str,
    out: &mut Vec<PatchOp>,
    fuel: &mut u64,
) -> Result<(), Error> {
    charge(fuel, 1)?;
    if let (Ok(old_elems), Ok(new_elems)) = (slice_reps(heap, old), slice_reps(heap, new)) {
        let shared = old_elems.len().min(new_elems.len());
        for i in 0..shared {
            collect_diff(
                heap,
                &old_elems[i],
                &new_elems[i],
                &format!("{path}/{i}"),
                out,
                fuel,
            )?;
        }
        for i in (new_elems.len()..old_elems.len()).rev() {
            out.push(PatchOp::Remove {
                path: format!("{path}/{i}"),
            });
        }
        for (i, item) in new_elems.iter().enumerate().skip(old_elems.len()) {
            out.push(PatchOp::Add {
                path: format!("{path}/{i}"),
                value: heap.dup(item)?,
            });
        }
        return Ok(());
    }
    if let (Some(o_pairs), Some(n_pairs)) = (record_reps(heap, old)?, record_reps(heap, new)?) {
        let mut all: BTreeSet<String> = BTreeSet::new();
        for (k, _) in &o_pairs {
            all.insert(k.clone());
        }
        for (k, _) in &n_pairs {
            all.insert(k.clone());
        }
        let mut sorted: Vec<String> = all.into_iter().collect();
        sorted.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        for k in sorted {
            let at = format!("{path}/{}", escape_token(&k));
            let ov = o_pairs.iter().find(|(ok, _)| ok == &k).map(|(_, v)| v);
            let nv = n_pairs.iter().find(|(nk, _)| nk == &k).map(|(_, v)| v);
            match (ov, nv) {
                (Some(o), Some(n)) => collect_diff(heap, o, n, &at, out, fuel)?,
                (None, Some(n)) => out.push(PatchOp::Add {
                    path: at,
                    value: heap.dup(n)?,
                }),
                (Some(_), None) => out.push(PatchOp::Remove { path: at }),
                (None, None) => {}
            }
        }
        return Ok(());
    }
    if json_scalar_same(heap, old, new)? {
        return Ok(());
    }
    out.push(PatchOp::Replace {
        path: path.to_owned(),
        value: heap.dup(new)?,
    });
    Ok(())
}

pub(super) fn record_reps(heap: &Heap, val: &Value) -> Result<Option<Vec<(String, Value)>>, Error> {
    let Value::Object(tok) = val else {
        return Ok(None);
    };
    let Data::Record(fields) = &heap.object(tok.id)?.data else {
        return Ok(None);
    };
    Ok(Some(
        fields
            .iter()
            .map(|(k, v)| (k.clone(), v.representation()))
            .collect(),
    ))
}

fn json_scalar_same(heap: &Heap, left: &Value, right: &Value) -> Result<bool, Error> {
    Ok(match (left, right) {
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => {
            a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
        }
        (Value::Object(a), Value::Object(b)) => {
            match (&heap.object(a.id)?.data, &heap.object(b.id)?.data) {
                (Data::JsonNull, Data::JsonNull) => true,
                (Data::String(x), Data::String(y)) => x == y,
                (Data::Utf16(x), Data::Utf16(y)) => x == y,
                _ => false,
            }
        }
        _ => false,
    })
}

pub(super) fn patch_apply(
    heap: &mut Heap,
    patch_val: &Value,
    doc_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let op_reps = slice_reps(heap, patch_val)?;
    charge(fuel, op_reps.len())?;
    let mut cur = heap.dup(doc_val)?;
    for op_rep in &op_reps {
        let op = match decode_operation(heap, op_rep) {
            Ok(op) => op,
            Err(err) => {
                heap.drop_value(cur)?;
                return Err(err);
            }
        };
        match apply_one(heap, cur, &op, fuel) {
            Ok(Ok(next)) => cur = next,
            Ok(Err(msg)) => {
                let s = heap.literal(&Lit::String(msg))?;
                let err = heap.construct("JsonPatch.PatchError.PatchError", vec![s])?;
                return heap.construct("Prelude.Result.Err", vec![err]);
            }
            Err(err) => return Err(err),
        }
    }
    heap.construct("Prelude.Result.Ok", vec![cur])
}

fn decode_operation(heap: &Heap, val: &Value) -> Result<PatchOp, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("JsonPatch.Operation"));
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("JsonPatch.Operation"));
    };
    let path = heap.string_lossy(&fields[0])?;
    match tag.as_str() {
        "JsonPatch.Operation.Remove" => Ok(PatchOp::Remove { path }),
        "JsonPatch.Operation.Add" => Ok(PatchOp::Add {
            path,
            value: fields[1].representation(),
        }),
        "JsonPatch.Operation.Replace" => Ok(PatchOp::Replace {
            path,
            value: fields[1].representation(),
        }),
        _ => Err(Error::PatternKind("JsonPatch.Operation")),
    }
}

fn parse_array_index(token: &str) -> Result<usize, String> {
    if token == "0"
        || (!token.is_empty()
            && token.as_bytes()[0].is_ascii_digit()
            && token.as_bytes()[0] != b'0'
            && token.bytes().all(|b| b.is_ascii_digit()))
    {
        Ok(token.parse::<usize>().unwrap_or(usize::MAX))
    } else {
        Err(format!("Invalid array index: \"{token}\""))
    }
}

enum Step {
    Array { list_rep: Value, index: usize },
    Object { rec_rep: Value, key: String },
}

pub(super) fn apply_one(
    heap: &mut Heap,
    doc: Value,
    op: &PatchOp,
    fuel: &mut u64,
) -> Result<Result<Value, String>, Error> {
    charge(fuel, 1)?;
    let path = op.path();
    let kind = op.kind_name();
    if path.is_empty() {
        if kind == "remove" {
            heap.drop_value(doc)?;
            return Ok(Err("Unsupported operation at the root".into()));
        }
        let val_rep = match op {
            PatchOp::Add { value, .. } | PatchOp::Replace { value, .. } => value,
            PatchOp::Remove { .. } => unreachable!(),
        };
        let next = match heap.dup(val_rep) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_value(doc)?;
                return Err(err);
            }
        };
        heap.drop_value(doc)?;
        return Ok(Ok(next));
    }
    let Some(rest) = path.strip_prefix('/') else {
        heap.drop_value(doc)?;
        return Ok(Err(format!(
            "Invalid JSON Pointer, it must start with \"/\": {}",
            json_string(path)
        )));
    };
    let tokens: Vec<String> = rest.split('/').map(unescape_token).collect();
    let not_found =
        || format!("Cannot {kind} at \"{path}\" (parent not found or not a container).");
    let mut stack = Vec::new();
    let mut cur = doc.representation();
    for token in &tokens[..tokens.len() - 1] {
        if let Ok(elems) = slice_reps(heap, &cur) {
            let idx = match parse_array_index(token) {
                Ok(i) => i,
                Err(msg) => {
                    heap.drop_value(doc)?;
                    return Ok(Err(msg));
                }
            };
            if idx >= elems.len() {
                heap.drop_value(doc)?;
                return Ok(Err(not_found()));
            }
            stack.push(Step::Array {
                list_rep: cur,
                index: idx,
            });
            cur = elems[idx].representation();
        } else if let Some(pairs) = record_reps(heap, &cur)? {
            let Some((_, child)) = pairs.iter().find(|(k, _)| k == token) else {
                heap.drop_value(doc)?;
                return Ok(Err(not_found()));
            };
            stack.push(Step::Object {
                rec_rep: cur,
                key: token.clone(),
            });
            cur = child.representation();
        } else {
            heap.drop_value(doc)?;
            return Ok(Err(not_found()));
        }
    }
    let last = &tokens[tokens.len() - 1];
    let updated = match update_leaf(heap, &cur, last, op, &not_found) {
        Ok(Ok(v)) => v,
        Ok(Err(msg)) => {
            heap.drop_value(doc)?;
            return Ok(Err(msg));
        }
        Err(err) => {
            heap.drop_value(doc)?;
            return Err(err);
        }
    };
    let rebuilt = rebuild_path(heap, stack, updated);
    heap.drop_value(doc)?;
    Ok(Ok(rebuilt?))
}

fn update_leaf(
    heap: &mut Heap,
    parent: &Value,
    last: &str,
    op: &PatchOp,
    not_found: &impl Fn() -> String,
) -> Result<Result<Value, String>, Error> {
    let kind = op.kind_name();
    let path = op.path();
    if let Ok(elems) = slice_reps(heap, parent) {
        if last == "-" && kind != "add" {
            return Ok(Err(format!("\"-\" is not valid for {kind} at \"{path}\".")));
        }
        let idx = if last == "-" {
            elems.len()
        } else {
            match parse_array_index(last) {
                Ok(i) => i,
                Err(msg) => return Ok(Err(msg)),
            }
        };
        let limit = if kind == "add" {
            elems.len()
        } else {
            elems.len().saturating_sub(1)
        };
        if elems.is_empty() && kind != "add" || idx > limit {
            return Ok(Err(format!("Array index out of bounds at \"{path}\".")));
        }
        let mut owned = dup_reps(heap, &elems)?;
        match op {
            PatchOp::Add { value, .. } => {
                let inserted = match heap.dup(value) {
                    Ok(v) => v,
                    Err(err) => {
                        heap.drop_values(owned)?;
                        return Err(err);
                    }
                };
                owned.insert(idx, inserted);
            }
            PatchOp::Remove { .. } => {
                let removed = owned.remove(idx);
                heap.drop_value(removed)?;
            }
            PatchOp::Replace { value, .. } => {
                let rep = match heap.dup(value) {
                    Ok(v) => v,
                    Err(err) => {
                        heap.drop_values(owned)?;
                        return Err(err);
                    }
                };
                let old = mem::replace(&mut owned[idx], rep);
                heap.drop_value(old)?;
            }
        }
        return Ok(Ok(heap.construct("$list", owned)?));
    }
    if let Some(pairs) = record_reps(heap, parent)? {
        let has_key = pairs.iter().any(|(k, _)| k == last);
        if kind != "add" && !has_key {
            return Ok(Err(format!(
                "Property \"{last}\" does not exist at \"{path}\"."
            )));
        }
        let mut owned = Vec::with_capacity(pairs.len() + 1);
        for (k, v) in pairs {
            if kind == "remove" && k == last {
                continue;
            }
            match heap.dup(&v) {
                Ok(dv) => owned.push((k, dv)),
                Err(err) => {
                    drop_pairs(heap, owned)?;
                    return Err(err);
                }
            }
        }
        if let PatchOp::Add { value, .. } | PatchOp::Replace { value, .. } = op {
            let dv = match heap.dup(value) {
                Ok(v) => v,
                Err(err) => {
                    drop_pairs(heap, owned)?;
                    return Err(err);
                }
            };
            if let Some(old) = put_js_key(&mut owned, last.to_owned(), dv) {
                heap.drop_value(old)?;
            }
        }
        return Ok(Ok(heap.record(owned)?));
    }
    Ok(Err(not_found()))
}

fn rebuild_path(heap: &mut Heap, stack: Vec<Step>, mut updated: Value) -> Result<Value, Error> {
    for step in stack.into_iter().rev() {
        match step {
            Step::Array { list_rep, index } => {
                let elems = slice_reps(heap, &list_rep)?;
                let mut owned = match dup_reps(heap, &elems) {
                    Ok(v) => v,
                    Err(err) => {
                        heap.drop_value(updated)?;
                        return Err(err);
                    }
                };
                let old = mem::replace(&mut owned[index], updated);
                heap.drop_value(old)?;
                updated = heap.construct("$list", owned)?;
            }
            Step::Object { rec_rep, key } => {
                let pairs = record_reps(heap, &rec_rep)?.ok_or(Error::ExpectedRecord)?;
                let mut owned = Vec::with_capacity(pairs.len());
                for (k, v) in pairs {
                    match heap.dup(&v) {
                        Ok(dv) => owned.push((k, dv)),
                        Err(err) => {
                            drop_pairs(heap, owned)?;
                            heap.drop_value(updated)?;
                            return Err(err);
                        }
                    }
                }
                if let Some(old) = put_js_key(&mut owned, key, updated) {
                    heap.drop_value(old)?;
                }
                updated = heap.record(owned)?;
            }
        }
    }
    Ok(updated)
}

fn drop_pairs(heap: &mut Heap, pairs: Vec<(String, Value)>) -> Result<(), Error> {
    heap.drop_values(pairs.into_iter().map(|(_, v)| v).collect())
}
