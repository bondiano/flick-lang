//! Tier-neutral pure `Context.*` map and reference primitives (Stage 4A).

use std::collections::BTreeMap;
use std::mem;

use flick_core::ir::memory::LayerIntrinsic;

use crate::tier0::{Data, DefectSpec, Error, Heap, Value};

use super::coop::{HOST_CTX_KEY, clone_list_values, concat_host_context_lists, extract_opt_value};

/// Outcome of a pure `Context.*` primitive: either a value or a language defect
/// (e.g., `Service not found` on `Context.get`).
pub(crate) enum ContextOutcome {
    Value(Value),
    Defect(DefectSpec),
}

pub(crate) fn service_tag_key<'a>(heap: &'a Heap, val: &Value) -> Result<&'a str, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::ExpectedServiceTag);
    };
    match &heap.object(tok.id)?.data {
        Data::ServiceTag { key } | Data::ContextRef { key, .. } => Ok(key),
        _ => Err(Error::ExpectedServiceTag),
    }
}

pub(crate) fn expect_context_map<'a>(
    heap: &'a Heap,
    val: &Value,
) -> Result<&'a BTreeMap<String, Value>, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::ExpectedContext);
    };
    let Data::Context(map) = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedContext);
    };
    Ok(map)
}

pub(crate) fn clone_context_map(
    heap: &mut Heap,
    val: &Value,
) -> Result<BTreeMap<String, Value>, Error> {
    let pairs: Vec<(String, Value)> = expect_context_map(heap, val)?
        .iter()
        .map(|(k, v)| (k.clone(), v.representation()))
        .collect();
    let mut out = BTreeMap::new();
    for (k, rep) in pairs {
        match heap.dup(&rep) {
            Ok(v) => {
                out.insert(k, v);
            }
            Err(err) => {
                heap.drop_values(out.into_values().collect())?;
                return Err(err);
            }
        }
    }
    Ok(out)
}

pub(crate) fn merge_context_values(heap: &mut Heap, contexts: Vec<Value>) -> Result<Value, Error> {
    let mut merged: BTreeMap<String, Value> = BTreeMap::new();
    let mut iter = contexts.into_iter();
    while let Some(ctx) = iter.next() {
        let map = match clone_context_map(heap, &ctx) {
            Ok(m) => m,
            Err(err) => {
                heap.drop_value(ctx)?;
                heap.drop_values(iter.collect())?;
                heap.drop_values(merged.into_values().collect())?;
                return Err(err);
            }
        };
        heap.drop_value(ctx)?;
        for (k, v) in map {
            if k == HOST_CTX_KEY
                && let Some(prev) = merged.remove(HOST_CTX_KEY)
            {
                let combined = concat_host_context_lists(heap, prev, v)?;
                merged.insert(k, combined);
            } else if let Some(old) = merged.insert(k, v) {
                heap.drop_value(old)?;
            }
        }
    }
    Ok(heap.allocate(Data::Context(merged)))
}

/// Evaluates pure `Context.*` intrinsics, consuming the operand vector.
pub(crate) fn eval_pure_context(
    heap: &mut Heap,
    op: LayerIntrinsic,
    scratch: &mut Vec<Value>,
) -> Result<ContextOutcome, Error> {
    let out = match op {
        LayerIntrinsic::ContextEmpty => {
            heap.drop_values(mem::take(scratch))?;
            heap.allocate(Data::Context(BTreeMap::new()))
        }
        LayerIntrinsic::ContextMake => {
            let imp = scratch.pop().expect("impl");
            let tag = scratch.pop().expect("tag");
            let key = service_tag_key(heap, &tag)?.to_owned();
            heap.drop_value(tag)?;
            let mut map = BTreeMap::new();
            map.insert(key, imp);
            heap.allocate(Data::Context(map))
        }
        LayerIntrinsic::ContextAdd => {
            let ctx = scratch.pop().expect("ctx");
            let imp = scratch.pop().expect("impl");
            let tag = scratch.pop().expect("tag");
            let key = service_tag_key(heap, &tag)?.to_owned();
            heap.drop_value(tag)?;
            let mut map = clone_context_map(heap, &ctx)?;
            heap.drop_value(ctx)?;
            if let Some(old) = map.insert(key, imp) {
                heap.drop_value(old)?;
            }
            heap.allocate(Data::Context(map))
        }
        LayerIntrinsic::ContextAddOrOmit => return exec_context_add_or_omit(heap, scratch),
        LayerIntrinsic::ContextGet => return exec_context_get(heap, scratch),
        LayerIntrinsic::ContextGetOption => return exec_context_get_option(heap, scratch),
        LayerIntrinsic::ContextMerge => {
            // `Context.merge that self`: `that` replaces `self` (matches Go ContextMerge).
            let self_ctx = scratch.pop().expect("self ctx");
            let that_ctx = scratch.pop().expect("that ctx");
            merge_context_values(heap, vec![self_ctx, that_ctx])?
        }
        LayerIntrinsic::ContextMergeAll => {
            let rest = scratch.pop().expect("rest contexts");
            let first = scratch.pop().expect("first context");
            let mut all = vec![first];
            all.extend(clone_list_values(heap, &rest)?);
            heap.drop_value(rest)?;
            merge_context_values(heap, all)?
        }
        LayerIntrinsic::ContextPick => return exec_context_pick(heap, scratch),
        LayerIntrinsic::ContextOmit => {
            let ctx = scratch.pop().expect("ctx");
            let tag = scratch.pop().expect("tag");
            let key = service_tag_key(heap, &tag)?.to_owned();
            heap.drop_value(tag)?;
            let mut map = clone_context_map(heap, &ctx)?;
            heap.drop_value(ctx)?;
            if let Some(old) = map.remove(&key) {
                heap.drop_value(old)?;
            }
            heap.allocate(Data::Context(map))
        }
        LayerIntrinsic::ContextReference => {
            let default_fn = scratch.pop().expect("default fn");
            let key_val = scratch.pop().expect("ref key");
            let key = heap.string(&key_val)?.to_owned();
            heap.drop_value(key_val)?;
            heap.allocate(Data::ContextRef { key, default_fn })
        }
        _ => unreachable!("non-pure context intrinsic"),
    };
    Ok(ContextOutcome::Value(out))
}

fn exec_context_add_or_omit(
    heap: &mut Heap,
    scratch: &mut Vec<Value>,
) -> Result<ContextOutcome, Error> {
    let ctx = scratch.pop().expect("ctx");
    let opt = scratch.pop().expect("opt impl");
    let tag = scratch.pop().expect("tag");
    let key = service_tag_key(heap, &tag)?.to_owned();
    heap.drop_value(tag)?;
    let mut map = clone_context_map(heap, &ctx)?;
    heap.drop_value(ctx)?;
    if let Some(imp) = extract_opt_value(heap, opt)? {
        if let Some(old) = map.insert(key, imp) {
            heap.drop_value(old)?;
        }
    } else if let Some(old) = map.remove(&key) {
        heap.drop_value(old)?;
    }
    Ok(ContextOutcome::Value(heap.allocate(Data::Context(map))))
}

fn exec_context_get(heap: &mut Heap, scratch: &mut Vec<Value>) -> Result<ContextOutcome, Error> {
    let ctx = scratch.pop().expect("ctx");
    let tag = scratch.pop().expect("tag");
    let key = service_tag_key(heap, &tag)?.to_owned();
    heap.drop_value(tag)?;
    let rep = expect_context_map(heap, &ctx)?
        .get(&key)
        .map(Value::representation);
    let Some(r) = rep else {
        heap.drop_value(ctx)?;
        return Ok(ContextOutcome::Defect(DefectSpec::custom(format!(
            "Service not found: {key}"
        ))));
    };
    let out = heap.dup(&r)?;
    heap.drop_value(ctx)?;
    Ok(ContextOutcome::Value(out))
}

fn exec_context_get_option(
    heap: &mut Heap,
    scratch: &mut Vec<Value>,
) -> Result<ContextOutcome, Error> {
    let ctx = scratch.pop().expect("ctx");
    let tag = scratch.pop().expect("tag");
    let key = service_tag_key(heap, &tag)?.to_owned();
    heap.drop_value(tag)?;
    let rep = expect_context_map(heap, &ctx)?
        .get(&key)
        .map(Value::representation);
    let out = match rep {
        Some(r) => {
            let dup = heap.dup(&r)?;
            heap.construct("Prelude.Option.Some", vec![dup])?
        }
        None => heap.construct("Prelude.Option.None", vec![])?,
    };
    heap.drop_value(ctx)?;
    Ok(ContextOutcome::Value(out))
}

fn exec_context_pick(heap: &mut Heap, scratch: &mut Vec<Value>) -> Result<ContextOutcome, Error> {
    let ctx = scratch.pop().expect("ctx");
    let tag = scratch.pop().expect("tag");
    let key = service_tag_key(heap, &tag)?.to_owned();
    heap.drop_value(tag)?;
    let rep = expect_context_map(heap, &ctx)?
        .get(&key)
        .map(Value::representation);
    let mut map = BTreeMap::new();
    if let Some(r) = rep {
        map.insert(key, heap.dup(&r)?);
    }
    heap.drop_value(ctx)?;
    Ok(ContextOutcome::Value(heap.allocate(Data::Context(map))))
}
