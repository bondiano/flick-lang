//! Iterative flat-node wire encoder (top-level driver, primitives, layers, causes).

use flick_core::ir::Shape;
use serde_json::{Value as Json, json};

use crate::memory::causes::{CauseData, StoredReason};
use crate::memory::{Data, Error, Heap, LayerData, Value, primitives};

use super::WireRegistrar;
use super::encode_obj::encode_object;

pub(super) enum EncFrame<'a> {
    Visit {
        val: Value,
        shape: Option<&'a Shape>,
        cb_arity: Option<usize>,
        slot: Option<(usize, usize)>,
    },
    FinishList {
        kind: &'static str,
        items: Vec<usize>,
        slot: Option<(usize, usize)>,
    },
    FinishRec {
        kind: &'static str,
        keys: Vec<String>,
        vals: Vec<usize>,
        slot: Option<(usize, usize)>,
    },
    FinishDict {
        keys: Vec<String>,
        getters: Vec<bool>,
        vals: Vec<usize>,
        slot: Option<(usize, usize)>,
    },
    FinishTag {
        tag: String,
        fields: Vec<usize>,
        slot: Option<(usize, usize)>,
    },
    FinishLayerSucceed {
        key: String,
        impl_slot: [usize; 1],
        slot: Option<(usize, usize)>,
    },
    FinishLayerMerge {
        pair: [usize; 2],
        slot: Option<(usize, usize)>,
    },
    FinishLayerContext {
        ctx_slot: [usize; 1],
        slot: Option<(usize, usize)>,
    },
    FinishLayerMergeAll {
        items: Vec<usize>,
        slot: Option<(usize, usize)>,
    },
    FinishExit {
        kind: &'static str,
        inner: [usize; 1],
        slot: Option<(usize, usize)>,
    },
    FinishCause {
        kinds: Vec<CauseReasonKind>,
        vals: Vec<usize>,
        slot: Option<(usize, usize)>,
    },
}

pub(super) enum CauseReasonKind {
    Fail(usize),
    Die(String),
    Interrupt(Option<usize>),
}

/// Encodes a borrowed Rust `Value` into a flat `{ "root": idx, "nodes": [...] }` payload.
pub(in crate::memory) fn encode_value(
    heap: &mut Heap,
    execution: usize,
    value: &Value,
    shape: Option<&Shape>,
    cb_arity: Option<usize>,
    reg: &mut dyn WireRegistrar,
) -> Result<Json, Error> {
    let mut nodes: Vec<Json> = Vec::new();
    let mut root_idx = 0usize;
    let mut stack: Vec<EncFrame<'_>> = vec![EncFrame::Visit {
        val: value.representation(),
        shape,
        cb_arity,
        slot: None,
    }];

    while let Some(frame) = stack.pop() {
        match frame {
            EncFrame::Visit {
                val,
                shape,
                cb_arity,
                slot,
            } => {
                encode_visit(
                    heap,
                    execution,
                    val,
                    shape,
                    cb_arity,
                    slot,
                    reg,
                    &mut nodes,
                    &mut root_idx,
                    &mut stack,
                )?;
            }
            finish => finish_frame(finish, &mut nodes, &mut root_idx, &mut stack),
        }
    }

    Ok(json!({ "root": root_idx, "nodes": nodes }))
}

fn finish_frame(
    frame: EncFrame<'_>,
    nodes: &mut Vec<Json>,
    root_idx: &mut usize,
    stack: &mut [EncFrame<'_>],
) {
    match frame {
        EncFrame::FinishList { kind, items, slot } => {
            let id = emit(nodes, json!([kind, items]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishRec {
            kind,
            keys,
            vals,
            slot,
        } => {
            let pairs: Vec<Json> = keys
                .into_iter()
                .zip(vals)
                .map(|(k, v)| json!([k, v]))
                .collect();
            let id = emit(nodes, json!([kind, pairs]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishDict {
            keys,
            getters,
            vals,
            slot,
        } => {
            let entries: Vec<Json> = keys
                .into_iter()
                .zip(getters)
                .zip(vals)
                .map(|((k, g), v)| json!([k, g, v]))
                .collect();
            let id = emit(nodes, json!(["dict", entries]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishTag { tag, fields, slot } => {
            let id = emit(nodes, json!(["tag", tag, fields]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishLayerSucceed {
            key,
            impl_slot,
            slot,
        } => {
            let id = emit(nodes, json!(["layer_succeed", key, impl_slot[0]]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishLayerMerge { pair, slot } => {
            let id = emit(nodes, json!(["layer_merge", pair[0], pair[1]]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishLayerContext { ctx_slot, slot } => {
            let id = emit(nodes, json!(["layer_context", ctx_slot[0]]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishLayerMergeAll { items, slot } => {
            let id = emit(nodes, json!(["layer_merge_all", items]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishExit { kind, inner, slot } => {
            let id = emit(nodes, json!([kind, inner[0]]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::FinishCause { kinds, vals, slot } => {
            let reasons: Vec<Json> = kinds
                .into_iter()
                .map(|k| match k {
                    CauseReasonKind::Fail(pos) => json!(["fail", vals[pos]]),
                    CauseReasonKind::Die(msg) => json!(["die", msg]),
                    CauseReasonKind::Interrupt(fid) => json!(["interrupt", fid]),
                })
                .collect();
            let id = emit(nodes, json!(["cause", reasons]));
            store_slot(stack, root_idx, slot, id);
        }
        EncFrame::Visit { .. } => unreachable!("visit frames are dispatched before finish_frame"),
    }
}

pub(super) fn emit(nodes: &mut Vec<Json>, node: Json) -> usize {
    let id = nodes.len();
    nodes.push(node);
    id
}

pub(super) fn store_slot(
    stack: &mut [EncFrame<'_>],
    root_idx: &mut usize,
    slot: Option<(usize, usize)>,
    id: usize,
) {
    let Some((frame_idx, pos)) = slot else {
        *root_idx = id;
        return;
    };
    match &mut stack[frame_idx] {
        EncFrame::FinishList { items, .. }
        | EncFrame::FinishTag { fields: items, .. }
        | EncFrame::FinishLayerMergeAll { items, .. } => {
            items[pos] = id;
        }
        EncFrame::FinishRec { vals, .. }
        | EncFrame::FinishDict { vals, .. }
        | EncFrame::FinishCause { vals, .. } => {
            vals[pos] = id;
        }
        EncFrame::FinishLayerSucceed { impl_slot, .. } => impl_slot[pos] = id,
        EncFrame::FinishLayerContext { ctx_slot, .. } => ctx_slot[pos] = id,
        EncFrame::FinishLayerMerge { pair, .. } => pair[pos] = id,
        EncFrame::FinishExit { inner, .. } => inner[pos] = id,
        EncFrame::Visit { .. } => unreachable!("visit frame cannot hold child slot"),
    }
}

#[expect(clippy::too_many_arguments, reason = "iterative stack encoder step")]
fn encode_visit<'a>(
    heap: &mut Heap,
    execution: usize,
    val: Value,
    shape: Option<&'a Shape>,
    cb_arity: Option<usize>,
    slot: Option<(usize, usize)>,
    reg: &mut dyn WireRegistrar,
    nodes: &mut Vec<Json>,
    root_idx: &mut usize,
    stack: &mut Vec<EncFrame<'a>>,
) -> Result<(), Error> {
    match val {
        Value::Unit => {
            let id = emit(nodes, json!(["u"]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::Bool(b) => {
            let id = emit(nodes, json!(["b", b]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::Int(n) => {
            let id = emit(nodes, json!(["i", n]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::Int8(n) => {
            let id = emit(nodes, json!(["i", n]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::Int16(n) => {
            let id = emit(nodes, json!(["i", n]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::UInt8(n) => {
            let id = emit(nodes, json!(["i", n]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::UInt16(n) => {
            let id = emit(nodes, json!(["i", n]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::UInt32(n) => {
            let id = emit(nodes, json!(["f", f64::from(n)]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::Int64(n) => {
            let id = emit(nodes, json!(["n", n.to_string()]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::UInt64(n) => {
            let id = emit(nodes, json!(["n", n.to_string()]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::Float(f) => {
            let node = if f.is_nan() {
                json!(["fs", "NaN"])
            } else if f == f64::INFINITY {
                json!(["fs", "Infinity"])
            } else if f == f64::NEG_INFINITY {
                json!(["fs", "-Infinity"])
            } else if f == 0.0 && f.is_sign_negative() {
                json!(["fs", "-0"])
            } else {
                json!(["f", f])
            };
            let id = emit(nodes, node);
            store_slot(stack, root_idx, slot, id);
        }
        Value::Float32(f) => {
            let f = f64::from(f);
            let node = if f.is_nan() {
                json!(["fs", "NaN"])
            } else if f == f64::INFINITY {
                json!(["fs", "Infinity"])
            } else if f == f64::NEG_INFINITY {
                json!(["fs", "-Infinity"])
            } else if f == 0.0 && f.is_sign_negative() {
                json!(["fs", "-0"])
            } else {
                json!(["f", f])
            };
            let id = emit(nodes, node);
            store_slot(stack, root_idx, slot, id);
        }
        Value::Owner(owner) => {
            reg.record_owner(owner.index);
            let id = emit(nodes, json!(["owner", owner.index]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::Cell(cell) => {
            let cell_id = reg.register_cell(&cell);
            let id = emit(nodes, json!(["cell", cell_id]));
            store_slot(stack, root_idx, slot, id);
        }
        Value::Object(token) => {
            encode_object(
                heap, execution, token.id, shape, cb_arity, slot, reg, nodes, root_idx, stack,
            )?;
        }
        _ => {
            return Err(Error::HostBridge(
                "cannot marshal execution-local cell/var/arena handle across host bridge".into(),
            ));
        }
    }
    Ok(())
}

pub(super) fn encode_layer(
    heap: &Heap,
    layer: &LayerData,
    slot: Option<(usize, usize)>,
    nodes: &mut Vec<Json>,
    root_idx: &mut usize,
    stack: &mut Vec<EncFrame<'_>>,
) -> Result<(), Error> {
    match layer {
        LayerData::Empty => {
            let id = emit(nodes, json!(["layer_empty"]));
            store_slot(stack, root_idx, slot, id);
        }
        LayerData::Succeed {
            tag,
            implementation,
        } => {
            let Value::Object(tok) = tag else {
                return Err(Error::ExpectedServiceTag);
            };
            let Data::ServiceTag { key } = &heap.object(tok.id)?.data else {
                return Err(Error::ExpectedServiceTag);
            };
            let key = key.clone();
            let impl_val = implementation.representation();
            let frame_idx = stack.len();
            stack.push(EncFrame::FinishLayerSucceed {
                key,
                impl_slot: [0],
                slot,
            });
            stack.push(EncFrame::Visit {
                val: impl_val,
                shape: None,
                cb_arity: None,
                slot: Some((frame_idx, 0)),
            });
        }
        LayerData::Merge { left, right } => {
            let l = left.representation();
            let r = right.representation();
            let frame_idx = stack.len();
            stack.push(EncFrame::FinishLayerMerge { pair: [0, 0], slot });
            stack.push(EncFrame::Visit {
                val: r,
                shape: None,
                cb_arity: None,
                slot: Some((frame_idx, 1)),
            });
            stack.push(EncFrame::Visit {
                val: l,
                shape: None,
                cb_arity: None,
                slot: Some((frame_idx, 0)),
            });
        }
        LayerData::Context(ctx) => {
            let c = ctx.representation();
            let frame_idx = stack.len();
            stack.push(EncFrame::FinishLayerContext {
                ctx_slot: [0],
                slot,
            });
            stack.push(EncFrame::Visit {
                val: c,
                shape: None,
                cb_arity: None,
                slot: Some((frame_idx, 0)),
            });
        }
        LayerData::MergeAll(ls) => {
            let items: Vec<Value> = ls.iter().map(Value::representation).collect();
            let n = items.len();
            let frame_idx = stack.len();
            stack.push(EncFrame::FinishLayerMergeAll {
                items: vec![0; n],
                slot,
            });
            for (i, child) in items.into_iter().enumerate().rev() {
                stack.push(EncFrame::Visit {
                    val: child,
                    shape: None,
                    cb_arity: None,
                    slot: Some((frame_idx, i)),
                });
            }
        }
        _ => {
            return Err(Error::HostBridge(
                "cannot marshal dynamic layer variant across host bridge".into(),
            ));
        }
    }
    Ok(())
}

pub(super) fn encode_cause(
    heap: &Heap,
    cause: &CauseData,
    slot: Option<(usize, usize)>,
    stack: &mut Vec<EncFrame<'_>>,
) {
    let mut kinds = Vec::with_capacity(cause.reasons.len());
    let mut fail_vals = Vec::new();
    for reason in &cause.reasons {
        match reason {
            StoredReason::Fail(v) => {
                let pos = fail_vals.len();
                fail_vals.push(v.representation());
                kinds.push(CauseReasonKind::Fail(pos));
            }
            StoredReason::Die(d) => {
                let msg = primitives::describe_defect(heap, d).unwrap_or_else(|_| "defect".into());
                kinds.push(CauseReasonKind::Die(msg));
            }
            StoredReason::Interrupt { fiber_id } => {
                kinds.push(CauseReasonKind::Interrupt(*fiber_id));
            }
        }
    }
    let frame_idx = stack.len();
    let n = fail_vals.len();
    stack.push(EncFrame::FinishCause {
        kinds,
        vals: vec![0; n],
        slot,
    });
    for (i, val) in fail_vals.into_iter().enumerate().rev() {
        stack.push(EncFrame::Visit {
            val,
            shape: None,
            cb_arity: None,
            slot: Some((frame_idx, i)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{CellHandle, ObjectId};

    struct NoopRegistrar;

    impl WireRegistrar for NoopRegistrar {
        fn register_closure(&mut self, _: &mut Heap, _: ObjectId, _: usize) -> Result<u64, Error> {
            unreachable!("no closures in scalar wire test")
        }
        fn register_task(&mut self, _: &mut Heap, _: ObjectId) -> Result<u64, Error> {
            unreachable!("no tasks in scalar wire test")
        }
        fn register_dict_getter(
            &mut self,
            _: &mut Heap,
            _: ObjectId,
            _: ObjectId,
        ) -> Result<u64, Error> {
            unreachable!("no dict getters in scalar wire test")
        }
        fn register_cell(&mut self, _: &CellHandle) -> u64 {
            unreachable!("no cells in scalar wire test")
        }
        fn record_owner(&mut self, _: usize) {}
    }

    #[test]
    fn int64_and_uint64_encode_with_bigint_wire_tag() {
        let mut heap = Heap::new();
        let mut reg = NoopRegistrar;
        for (value, expected) in [
            (Value::Int64(i64::MIN), "-9223372036854775808"),
            (Value::Int64(i64::MAX), "9223372036854775807"),
            (Value::UInt64(0), "0"),
            (Value::UInt64(u64::MAX), "18446744073709551615"),
        ] {
            let wire = encode_value(&mut heap, 0, &value, None, None, &mut reg).expect("encode");
            assert_eq!(wire, json!({ "root": 0, "nodes": [["n", expected]] }));
            let decoded = super::super::decode_value(&mut heap, 0, &wire).expect("decode");
            assert_eq!(heap.bigint(&decoded), Ok(expected));
            heap.drop_value(decoded).expect("drop");
        }
        assert!(heap.finish().is_ok());
    }
}
