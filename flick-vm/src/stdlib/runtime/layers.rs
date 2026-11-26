//! Tier-neutral `Layer` constructors, static layer evaluation, and service lookup.

use std::collections::BTreeMap;
use std::mem;

use flick_core::ir::memory::LayerIntrinsic;

use crate::stdlib::hooks::{Fuel, charge};
use crate::tier0::{Data, Error, Heap, LayerData, Value};

use super::context::{clone_context_map, expect_context_map, service_tag_key};
use super::coop::{HOST_CTX_KEY, clone_list_values, concat_host_context_lists};

pub(crate) fn ensure_layer(heap: &Heap, value: &Value) -> Result<(), Error> {
    let Value::Object(value) = value else {
        return Err(Error::ExpectedLayer);
    };
    match &heap.object(value.id)?.data {
        Data::Layer(_) | Data::ForeignHandle { .. } => Ok(()),
        _ => Err(Error::ExpectedLayer),
    }
}

pub(crate) fn layer_is_static(heap: &Heap, layer: &Value) -> Result<bool, Error> {
    let mut pending = vec![layer];
    while let Some(v) = pending.pop() {
        let Value::Object(tok) = v else {
            return Err(Error::ExpectedLayer);
        };
        match &heap.object(tok.id)?.data {
            Data::ForeignHandle { .. } => return Ok(false),
            Data::Layer(data) => match data {
                LayerData::Succeed { .. } | LayerData::Empty | LayerData::Context(_) => {}
                LayerData::Merge { left, right } => pending.extend([left, right]),
                LayerData::Dynamic { .. }
                | LayerData::Fresh(_)
                | LayerData::MergeAll(_)
                | LayerData::OrDie(_) => return Ok(false),
            },
            _ => return Err(Error::ExpectedLayer),
        }
    }
    Ok(true)
}

pub(crate) fn lookup_layer_service<'a>(
    heap: &'a Heap,
    value: &'a Value,
    effect: &str,
    fuel: Fuel<'_>,
) -> Result<Option<&'a Value>, Error> {
    let mut pending = vec![value];
    while let Some(cur) = pending.pop() {
        charge(fuel, 1)?;
        let Value::Object(tok) = cur else {
            return Err(Error::ExpectedLayer);
        };
        match &heap.object(tok.id)?.data {
            Data::Context(map) => {
                if let Some(imp) = map.get(effect) {
                    return Ok(Some(imp));
                }
            }
            Data::ForeignHandle { .. } | Data::Layer(LayerData::Empty) => {}
            Data::Layer(LayerData::Context(ctx)) => pending.push(ctx),
            Data::Layer(LayerData::Merge { left, right }) => {
                // Native servicesOf processes left-to-right with replacement;
                // searching right-first gives the same complete-provider key.
                pending.extend([left, right]);
            }
            Data::Layer(LayerData::Succeed {
                tag,
                implementation,
            }) => {
                let Value::Object(tag) = tag else {
                    return Err(Error::ExpectedServiceTag);
                };
                let Data::ServiceTag { key } = &heap.object(tag.id)?.data else {
                    return Err(Error::ExpectedServiceTag);
                };
                if key == effect {
                    return Ok(Some(implementation));
                }
            }
            _ => return Err(Error::ExpectedLayer),
        }
    }
    Ok(None)
}

pub(crate) fn layer_succeed(heap: &mut Heap, scratch: &mut Vec<Value>) -> Result<Value, Error> {
    let Value::Object(tag) = &scratch[0] else {
        return Err(Error::ExpectedServiceTag);
    };
    if !matches!(heap.object(tag.id)?.data, Data::ServiceTag { .. }) {
        return Err(Error::ExpectedServiceTag);
    }
    let mut inputs = mem::take(scratch).into_iter();
    Ok(heap.allocate(Data::Layer(LayerData::Succeed {
        tag: inputs.next().expect("tag"),
        implementation: inputs.next().expect("implementation"),
    })))
}

pub(crate) fn layer_merge(heap: &mut Heap, scratch: &mut Vec<Value>) -> Result<Value, Error> {
    ensure_layer(heap, &scratch[0])?;
    ensure_layer(heap, &scratch[1])?;
    let mut inputs = mem::take(scratch).into_iter();
    Ok(heap.allocate(Data::Layer(LayerData::Merge {
        left: inputs.next().expect("left layer"),
        right: inputs.next().expect("right layer"),
    })))
}

pub(crate) fn eval_layer_ctor(
    heap: &mut Heap,
    op: LayerIntrinsic,
    scratch: &mut Vec<Value>,
) -> Result<Value, Error> {
    let data = match op {
        LayerIntrinsic::LayerEmpty => LayerData::Empty,
        LayerIntrinsic::LayerSucceedContext => {
            let ctx = scratch.pop().expect("context");
            expect_context_map(heap, &ctx)?;
            LayerData::Context(ctx)
        }
        LayerIntrinsic::LayerFromBuild => {
            let build = scratch.pop().expect("build fn");
            let Value::Bool(memo) = scratch.pop().expect("memo bool") else {
                heap.drop_value(build)?;
                return Err(Error::ExpectedBoolean);
            };
            LayerData::Dynamic { memo, build }
        }
        LayerIntrinsic::LayerFresh => {
            let inner = scratch.pop().expect("layer");
            LayerData::Fresh(inner)
        }
        LayerIntrinsic::LayerOrDie => {
            let inner = scratch.pop().expect("layer");
            LayerData::OrDie(inner)
        }
        LayerIntrinsic::LayerMergeAll => {
            let rest = scratch.pop().expect("rest layers");
            let first = scratch.pop().expect("first layer");
            let mut all = vec![first];
            all.extend(clone_list_values(heap, &rest)?);
            heap.drop_value(rest)?;
            LayerData::MergeAll(all)
        }
        _ => unreachable!("non-constructor layer intrinsic"),
    };
    Ok(heap.allocate(Data::Layer(data)))
}

pub(crate) fn eval_static_layer_to_context(heap: &mut Heap, layer: Value) -> Result<Value, Error> {
    let mut map: BTreeMap<String, Value> = BTreeMap::new();
    let mut stack = vec![layer];
    while let Some(v) = stack.pop() {
        let Value::Object(tok) = &v else {
            heap.drop_value(v)?;
            heap.drop_values(stack)?;
            heap.drop_values(map.into_values().collect())?;
            return Err(Error::ExpectedLayer);
        };
        let action = match &heap.object(tok.id)?.data {
            Data::Layer(LayerData::Empty) => 0,
            Data::Layer(LayerData::Succeed { .. }) => 1,
            Data::Layer(LayerData::Context(_)) => 2,
            Data::Layer(LayerData::Merge { .. }) => 3,
            _ => {
                heap.drop_value(v)?;
                heap.drop_values(stack)?;
                heap.drop_values(map.into_values().collect())?;
                return Err(Error::ExpectedLayer);
            }
        };
        match action {
            0 => heap.drop_value(v)?,
            1 => {
                let (key, imp_rep) = {
                    let Data::Layer(LayerData::Succeed {
                        tag,
                        implementation,
                    }) = &heap.object(tok.id)?.data
                    else {
                        unreachable!()
                    };
                    (
                        service_tag_key(heap, tag)?.to_owned(),
                        implementation.representation(),
                    )
                };
                let imp = heap.dup(&imp_rep)?;
                heap.drop_value(v)?;
                if let Some(old) = map.insert(key, imp) {
                    heap.drop_value(old)?;
                }
            }
            2 => {
                let ctx_rep = {
                    let Data::Layer(LayerData::Context(c)) = &heap.object(tok.id)?.data else {
                        unreachable!()
                    };
                    c.representation()
                };
                let ctx_map = clone_context_map(heap, &ctx_rep)?;
                heap.drop_value(v)?;
                for (k, imp) in ctx_map {
                    if k == HOST_CTX_KEY
                        && let Some(prev) = map.remove(HOST_CTX_KEY)
                    {
                        let combined = concat_host_context_lists(heap, prev, imp)?;
                        map.insert(k, combined);
                    } else if let Some(old) = map.insert(k, imp) {
                        heap.drop_value(old)?;
                    }
                }
            }
            3 => {
                let (l_rep, r_rep) = {
                    let Data::Layer(LayerData::Merge { left, right }) = &heap.object(tok.id)?.data
                    else {
                        unreachable!()
                    };
                    (left.representation(), right.representation())
                };
                let right = heap.dup(&r_rep)?;
                let left = heap.dup(&l_rep)?;
                heap.drop_value(v)?;
                stack.push(right);
                stack.push(left);
            }
            _ => unreachable!(),
        }
    }
    Ok(heap.allocate(Data::Context(map)))
}
