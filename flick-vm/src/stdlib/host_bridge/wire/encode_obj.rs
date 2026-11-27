//! Heap object encoding helpers for the flat-node wire encoder.

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use flick_core::ir::Shape;
use serde_json::{Value as Json, json};

use crate::stdlib::primitives;
use crate::tier0::{
    Data, DictionaryValue, DurationData, Error, ExitData, Heap, ObjectId, TimeZoneData, Value,
};

use super::WireRegistrar;
use super::encode::{EncFrame, emit, encode_cause, encode_layer, store_slot};

#[expect(clippy::too_many_arguments, reason = "iterative object encoder")]
pub(super) fn encode_object<'a>(
    heap: &mut Heap,
    execution: usize,
    obj_id: ObjectId,
    shape: Option<&'a Shape>,
    cb_arity: Option<usize>,
    slot: Option<(usize, usize)>,
    reg: &mut dyn WireRegistrar,
    nodes: &mut Vec<Json>,
    root_idx: &mut usize,
    stack: &mut Vec<EncFrame<'a>>,
) -> Result<(), Error> {
    let data = &heap.object(obj_id)?.data;
    if let Some(leaf) = encode_leaf_object(data, execution)? {
        let id = emit(nodes, leaf);
        store_slot(stack, root_idx, slot, id);
        return Ok(());
    }
    match data {
        Data::Closure { .. } => {
            let (arity, runtime, sync) = closure_wire_meta(cb_arity, shape);
            let cb_id = reg.register_closure(heap, obj_id, arity)?;
            let id = emit(nodes, json!(["cb", cb_id, arity, runtime, sync]));
            store_slot(stack, root_idx, slot, id);
        }
        Data::Task { .. }
        | Data::MaskedTask { .. }
        | Data::CachedRunTask { .. }
        | Data::CachedInvalidateTask { .. }
        | Data::ExitTask(_) => {
            let cb_id = reg.register_task(heap, obj_id)?;
            let id = emit(nodes, json!(["task", cb_id]));
            store_slot(stack, root_idx, slot, id);
        }
        Data::Tagged {
            tag,
            ordinary_fields,
            fields,
            ..
        } => {
            let children: Vec<Value> = fields[..*ordinary_fields]
                .iter()
                .map(Value::representation)
                .collect();
            encode_tagged(tag, children, shape, slot, stack);
        }
        Data::ListSlice {
            backing,
            offset,
            len,
        } => encode_list_slice(heap, backing, *offset, *len, shape, slot, stack)?,
        Data::Record(entries) => {
            let keys: Vec<String> = entries.iter().map(|(k, _)| k.clone()).collect();
            let vals: Vec<Value> = entries.iter().map(|(_, v)| v.representation()).collect();
            encode_record(&keys, vals, shape, slot, stack);
        }
        Data::Dictionary(entries) => {
            let keys: Vec<String> = entries.iter().map(|(k, _)| k.clone()).collect();
            let getters: Vec<bool> = entries
                .iter()
                .map(|(_, dv)| matches!(dv, DictionaryValue::Getter(_)))
                .collect();
            let raw_vals: Vec<Value> = entries
                .iter()
                .map(|(_, dv)| dv.value().representation())
                .collect();
            encode_dictionary(
                heap, obj_id, keys, getters, raw_vals, slot, reg, nodes, root_idx, stack,
            )?;
        }
        Data::Layer(layer) => encode_layer(heap, layer, slot, nodes, root_idx, stack)?,
        Data::Context(map) => encode_context(map, slot, stack),
        Data::Exit(exit) => encode_exit(exit, slot, stack),
        Data::Cause(cause) => encode_cause(heap, cause, slot, stack),
        other => return Err(unsupported_object_error(other)),
    }
    Ok(())
}

fn encode_list_slice<'a>(
    heap: &Heap,
    backing: &Value,
    offset: usize,
    len: usize,
    shape: Option<&'a Shape>,
    slot: Option<(usize, usize)>,
    stack: &mut Vec<EncFrame<'a>>,
) -> Result<(), Error> {
    let Value::Object(btok) = backing else {
        return Err(Error::PatternKind("list"));
    };
    let Data::Tagged { fields, .. } = &heap.object(btok.id)?.data else {
        return Err(Error::PatternKind("list"));
    };
    let children: Vec<Value> = fields[offset..offset + len]
        .iter()
        .map(Value::representation)
        .collect();
    encode_list_items("list", children, shape, slot, stack);
    Ok(())
}

fn encode_context(
    map: &BTreeMap<String, Value>,
    slot: Option<(usize, usize)>,
    stack: &mut Vec<EncFrame<'_>>,
) {
    let keys: Vec<String> = map.keys().cloned().collect();
    let vals: Vec<Value> = map.values().map(Value::representation).collect();
    let frame_idx = stack.len();
    let n = keys.len();
    stack.push(EncFrame::FinishRec {
        kind: "context",
        keys,
        vals: vec![0; n],
        slot,
    });
    for (i, child) in vals.into_iter().enumerate().rev() {
        stack.push(EncFrame::Visit {
            val: child,
            shape: None,
            cb_arity: None,
            slot: Some((frame_idx, i)),
        });
    }
}

fn encode_exit(exit: &ExitData, slot: Option<(usize, usize)>, stack: &mut Vec<EncFrame<'_>>) {
    let (kind, inner) = match exit {
        ExitData::Success(v) => ("exit_ok", v.representation()),
        ExitData::Failure(c) => ("exit_fail", c.representation()),
    };
    let frame_idx = stack.len();
    stack.push(EncFrame::FinishExit {
        kind,
        inner: [0],
        slot,
    });
    stack.push(EncFrame::Visit {
        val: inner,
        shape: None,
        cb_arity: None,
        slot: Some((frame_idx, 0)),
    });
}

fn encode_leaf_object(data: &Data, execution: usize) -> Result<Option<Json>, Error> {
    let node = match data {
        Data::String(s) | Data::Label(s) => json!(["s", s]),
        Data::Utf16(units) => json!(["s16", units]),
        Data::Bytes(bytes) => json!(["bytes", BASE64.encode(bytes)]),
        Data::JsonNull => json!(["null"]),
        Data::BigInt(s) => json!(["n", s]),
        Data::BigDecimal { value, scale } => {
            json!(["dec", primitives::format_decimal(value, *scale)])
        }
        Data::Duration(d) => match d {
            DurationData::Millis(ms) => json!(["dur", "ms", ms]),
            DurationData::Nanos(ns) => json!(["dur", "ns", ns.to_string()]),
            DurationData::Infinite | DurationData::NegativeInfinite => json!(["dur", "inf"]),
        },
        Data::RegExp { pattern, flags, .. } => json!(["re", pattern, flags]),
        Data::ServiceTag { key } => json!(["stag", key]),
        Data::DateTime(dt) => match &dt.zone {
            None => json!(["dt", dt.ms]),
            Some(TimeZoneData::Offset(off)) => json!(["dt", dt.ms, ["offset", off]]),
            Some(TimeZoneData::Named { id, .. }) => json!(["dt", dt.ms, ["named", id]]),
        },
        Data::ForeignHandle {
            execution: owner,
            host_id,
        }
        | Data::ForeignCycle {
            execution: owner,
            host_id,
        } => {
            if *owner != execution {
                return Err(Error::HostBridge(
                    "foreign handle belongs to another execution".into(),
                ));
            }
            json!(["handle", host_id])
        }
        _ => return Ok(None),
    };
    Ok(Some(node))
}

fn closure_wire_meta(cb_arity: Option<usize>, shape: Option<&Shape>) -> (usize, bool, bool) {
    match (cb_arity, shape) {
        (Some(k), Some(Shape::Fun { runtime, sync, .. })) => (k, *runtime, *sync),
        (Some(k), _) => (k, false, false),
        (
            None,
            Some(Shape::Fun {
                args,
                runtime,
                sync,
                ..
            }),
        ) if args.len() == 1 => (1, *runtime, *sync),
        (None, _) => (1, false, false),
    }
}

fn encode_tagged<'a>(
    tag: &str,
    children: Vec<Value>,
    shape: Option<&'a Shape>,
    slot: Option<(usize, usize)>,
    stack: &mut Vec<EncFrame<'a>>,
) {
    if tag == "$list" || tag == "$tuple" {
        let kind = if tag == "$list" { "list" } else { "tuple" };
        encode_list_items(kind, children, shape, slot, stack);
        return;
    }
    let count = children.len();
    let frame_idx = stack.len();
    let child_shape = |i: usize| -> Option<&'a Shape> {
        match (tag, i, shape) {
            ("Prelude.Option.Some", 0, Some(Shape::Option(s))) => Some(s.as_ref()),
            ("Prelude.Result.Ok", 0, Some(Shape::Result(_, ok))) => Some(ok.as_ref()),
            ("Prelude.Result.Err", 0, Some(Shape::Result(err, _))) => Some(err.as_ref()),
            _ => None,
        }
    };
    stack.push(EncFrame::FinishTag {
        tag: tag.to_owned(),
        fields: vec![0; count],
        slot,
    });
    for (i, child) in children.into_iter().enumerate().rev() {
        stack.push(EncFrame::Visit {
            val: child,
            shape: child_shape(i),
            cb_arity: None,
            slot: Some((frame_idx, i)),
        });
    }
}

fn encode_list_items<'a>(
    kind: &'static str,
    children: Vec<Value>,
    shape: Option<&'a Shape>,
    slot: Option<(usize, usize)>,
    stack: &mut Vec<EncFrame<'a>>,
) {
    let count = children.len();
    let frame_idx = stack.len();
    stack.push(EncFrame::FinishList {
        kind,
        items: vec![0; count],
        slot,
    });
    let list_shape = match shape {
        Some(Shape::List(inner)) => Some(inner.as_ref()),
        _ => None,
    };
    let tuple_shapes = match shape {
        Some(Shape::Tuple(items)) => Some(items.as_slice()),
        _ => None,
    };
    for (i, child) in children.into_iter().enumerate().rev() {
        let cs = tuple_shapes.and_then(|s| s.get(i)).or(list_shape);
        stack.push(EncFrame::Visit {
            val: child,
            shape: cs,
            cb_arity: None,
            slot: Some((frame_idx, i)),
        });
    }
}

fn encode_record<'a>(
    keys: &[String],
    vals: Vec<Value>,
    shape: Option<&'a Shape>,
    slot: Option<(usize, usize)>,
    stack: &mut Vec<EncFrame<'a>>,
) {
    let rec_shapes = match shape {
        Some(Shape::Record(fields)) => Some(fields.as_slice()),
        _ => None,
    };
    let frame_idx = stack.len();
    let n = keys.len();
    stack.push(EncFrame::FinishRec {
        kind: "rec",
        keys: keys.to_vec(),
        vals: vec![0; n],
        slot,
    });
    for (i, child) in vals.into_iter().enumerate().rev() {
        let cs = rec_shapes.and_then(|fs| fs.iter().find(|(n, _)| *n == keys[i]).map(|(_, s)| s));
        stack.push(EncFrame::Visit {
            val: child,
            shape: cs,
            cb_arity: None,
            slot: Some((frame_idx, i)),
        });
    }
}

#[expect(clippy::too_many_arguments, reason = "iterative dictionary encoder")]
fn encode_dictionary(
    heap: &mut Heap,
    obj_id: ObjectId,
    keys: Vec<String>,
    getters: Vec<bool>,
    raw_vals: Vec<Value>,
    slot: Option<(usize, usize)>,
    reg: &mut dyn WireRegistrar,
    nodes: &mut Vec<Json>,
    root_idx: &mut usize,
    stack: &mut Vec<EncFrame<'_>>,
) -> Result<(), Error> {
    let mut vals = vec![0; keys.len()];
    let mut to_visit = Vec::new();
    for (i, (is_getter, child)) in getters.iter().copied().zip(raw_vals).enumerate() {
        if is_getter && let Value::Object(tok) = child {
            let cb_id = reg.register_dict_getter(heap, obj_id, tok.id)?;
            vals[i] = emit(nodes, json!(["cb", cb_id, 0, false, false]));
        } else {
            to_visit.push((i, child));
        }
    }
    if to_visit.is_empty() {
        let entries_json: Vec<Json> = keys
            .into_iter()
            .zip(getters)
            .zip(vals)
            .map(|((k, g), v)| json!([k, g, v]))
            .collect();
        let id = emit(nodes, json!(["dict", entries_json]));
        store_slot(stack, root_idx, slot, id);
    } else {
        let frame_idx = stack.len();
        stack.push(EncFrame::FinishDict {
            keys,
            getters,
            vals,
            slot,
        });
        for (i, child) in to_visit.into_iter().rev() {
            stack.push(EncFrame::Visit {
                val: child,
                shape: None,
                cb_arity: None,
                slot: Some((frame_idx, i)),
            });
        }
    }
    Ok(())
}

fn unsupported_object_error(other: &Data) -> Error {
    let kind = match other {
        Data::DateTime(_) => "DateTime",
        Data::TimeZone(_) => "TimeZone",
        Data::Cron(_) => "Cron",
        Data::Graph(_) => "Graph",
        Data::GraphWalker(_) => "GraphWalker",
        Data::ContextRef { .. } => "ContextRef",
        Data::FiberHandle { .. } => "FiberHandle",
        Data::Deferred { .. } => "Deferred",
        Data::MemoMap { .. } => "MemoMap",
        Data::FiberGroup { .. } => "FiberGroup",
        Data::Lazy { .. } => "Lazy",
        Data::Iterator { .. } => "Iterator",
        Data::Continuation { .. } => "Continuation",
        Data::Var { .. } => "Var",
        Data::Arena { .. } => "Arena",
        Data::Defect(_) => "Defect",
        Data::Seq(_) => "Seq",
        Data::CliSpec(_) => "CliSpec",
        Data::CliMulti(_) => "CliMulti",
        _ => "other",
    };
    Error::HostBridge(format!(
        "cannot marshal unsupported heap object ({kind}) across host bridge"
    ))
}
