//! Tier-neutral Stage 4 cooperative runtime constants and counted value helpers.

use std::collections::BTreeMap;

use crate::tier0::{Data, Error, ExitData, Heap, Value};

/// Default deterministic virtual clock start time (2025-06-15T17:46:40Z in ns).
pub const DEFAULT_VIRTUAL_NOW_NS: u64 = 1_750_000_000_000_000_000;

/// Default instruction quantum before cooperative fiber preemption.
pub const FIBER_QUANTUM: u32 = 256;

/// Virtual time increment applied per non-TestClock quantum when timers exist.
pub const QUANTUM_VIRTUAL_NS: u64 = 1_000_000;

/// Fiber-ref key holding the active `Layer.MemoMap`.
pub(crate) const MEMO_REF_KEY: &str = "effect/Layer/CurrentMemoMap";

/// Context map key holding bridged JS host context handles.
pub(crate) const HOST_CTX_KEY: &str = "flick/bridge/HostContext";

/// Duplicates every element of a flat `$list` or `ListSlice` into a new `Vec`.
pub(crate) fn clone_list_values(heap: &mut Heap, list_val: &Value) -> Result<Vec<Value>, Error> {
    let (backing_id, offset, len) = heap.list_view(list_val)?;
    let reps: Vec<Value> = {
        let Data::Tagged { fields, .. } = &heap.object(backing_id)?.data else {
            unreachable!("list_view verified tagged list")
        };
        fields[offset..offset + len]
            .iter()
            .map(Value::representation)
            .collect()
    };
    let mut out = Vec::with_capacity(reps.len());
    for rep in reps {
        match heap.dup(&rep) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    Ok(out)
}

/// Consumes an `Option` value, returning `Ok(Some(inner))` or `Ok(None)`.
pub(crate) fn extract_opt_value(heap: &mut Heap, opt: Value) -> Result<Option<Value>, Error> {
    let Value::Object(tok) = &opt else {
        heap.drop_value(opt)?;
        return Err(Error::ExpectedOption);
    };
    let res = match &heap.object(tok.id)?.data {
        Data::Tagged { tag, .. } if tag == "Prelude.Option.None" => None,
        Data::Tagged { tag, fields, .. } if tag == "Prelude.Option.Some" && fields.len() == 1 => {
            Some(heap.dup(&fields[0].representation())?)
        }
        _ => {
            heap.drop_value(opt)?;
            return Err(Error::ExpectedOption);
        }
    };
    heap.drop_value(opt)?;
    Ok(res)
}

/// Consumes an `Exit` value and duplicates its inner payload into `ExitData`.
pub(crate) fn extract_exit_data(heap: &mut Heap, val: Value) -> Result<ExitData, Error> {
    let Value::Object(tok) = &val else {
        heap.drop_value(val)?;
        return Err(Error::ExpectedExit);
    };
    let copied = match &heap.object(tok.id)?.data {
        Data::Exit(ExitData::Success(v)) => ExitData::Success(heap.dup(&v.representation())?),
        Data::Exit(ExitData::Failure(v)) => ExitData::Failure(heap.dup(&v.representation())?),
        _ => {
            heap.drop_value(val)?;
            return Err(Error::ExpectedExit);
        }
    };
    heap.drop_value(val)?;
    Ok(copied)
}

/// Decodes an `Option FiberId` value without consuming it.
pub(crate) fn decode_opt_fiber_id(heap: &Heap, val: &Value) -> Result<Option<u64>, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::ExpectedOption);
    };
    let Data::Tagged { tag, fields, .. } = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedOption);
    };
    if tag == "Prelude.Option.None" {
        Ok(None)
    } else if tag == "Prelude.Option.Some" && fields.len() == 1 {
        Ok(Some(decode_fiber_float(&fields[0])?))
    } else {
        Err(Error::ExpectedOption)
    }
}

/// Decodes a non-negative fiber ID stored as `Float` or `Int`.
pub(crate) fn decode_fiber_float(val: &Value) -> Result<u64, Error> {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "non-negative fiber id float"
    )]
    match val {
        Value::Float(f) => Ok(f.max(0.0) as u64),
        Value::Int(i) => Ok((*i).max(0) as u64),
        _ => Err(Error::ExpectedFloat),
    }
}

/// Reads an optional boolean record field, defaulting to `false` when absent.
pub(crate) fn record_bool_field(heap: &Heap, rec: &Value, name: &str) -> bool {
    heap.named_field(rec, name)
        .ok()
        .is_some_and(|v| matches!(v, Value::Bool(true)))
}

/// Wraps a host context handle in a single-entry `Data::Context` map.
pub(crate) fn wrap_host_context_handle(heap: &mut Heap, handle: Value) -> Result<Value, Error> {
    let list = heap.construct("$list", vec![handle])?;
    let mut map = BTreeMap::new();
    map.insert(HOST_CTX_KEY.to_owned(), list);
    Ok(heap.allocate(Data::Context(map)))
}

/// Concatenates two bridged host-context handle lists, consuming both inputs.
pub(crate) fn concat_host_context_lists(
    heap: &mut Heap,
    first: Value,
    second: Value,
) -> Result<Value, Error> {
    let mut items = Vec::new();
    for list in [first, second] {
        let count = match heap.list_view(&list) {
            Ok((_, _, c)) => c,
            Err(err) => {
                let _ = heap.drop_value(list);
                let _ = heap.drop_values(items);
                return Err(err);
            }
        };
        for i in 0..count {
            match heap.project(&list, i) {
                Ok(v) => items.push(v),
                Err(err) => {
                    let _ = heap.drop_value(list);
                    let _ = heap.drop_values(items);
                    return Err(err);
                }
            }
        }
        heap.drop_value(list)?;
    }
    heap.construct("$list", items)
}
