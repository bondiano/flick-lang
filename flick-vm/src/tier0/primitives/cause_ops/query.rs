//! Counted `Cause` query, annotation, and failure-replacement primitives.

use std::collections::BTreeSet;

use flick_core::ir::Lit;

use super::super::outcome_ops::{ReasonKind, describe_defect, expect_cause_reasons, matches_kind};
use super::super::{Data, Error, Heap, ObjectId, Value, charge};
use super::{drop_stored, snapshot_reasons};
use crate::tier0::causes::{CauseData, StoredReason};

pub(super) fn cause_find_reason(
    heap: &mut Heap,
    cause: &Value,
    kind: ReasonKind,
) -> Result<Value, Error> {
    let found = expect_cause_reasons(heap, cause)?
        .iter()
        .find(|r| matches_kind(r, kind))
        .map(|r| match r {
            StoredReason::Fail(v) => (ReasonKind::Fail, Some(v.representation()), None),
            StoredReason::Die(v) => (ReasonKind::Die, Some(v.representation()), None),
            StoredReason::Interrupt { fiber_id } => (ReasonKind::Interrupt, None, *fiber_id),
        });
    if let Some((k, v, fiber_id)) = found {
        let owned = match k {
            ReasonKind::Fail => StoredReason::Fail(heap.dup(&v.expect("fail"))?),
            ReasonKind::Die => StoredReason::Die(heap.dup(&v.expect("die"))?),
            ReasonKind::Interrupt => StoredReason::Interrupt { fiber_id },
        };
        let r_val = heap.allocate(Data::CauseReason(owned));
        heap.construct("Prelude.Result.Ok", vec![r_val])
    } else {
        let c = heap.dup(cause)?;
        heap.construct("Prelude.Result.Err", vec![c])
    }
}

pub(super) fn cause_find_error(
    heap: &mut Heap,
    cause: &Value,
    as_option: bool,
) -> Result<Value, Error> {
    let found = expect_cause_reasons(heap, cause)?
        .iter()
        .find_map(|r| match r {
            StoredReason::Fail(e) => Some(e.representation()),
            _ => None,
        });
    match (found, as_option) {
        (Some(e), true) => {
            let owned = heap.dup(&e)?;
            heap.construct("Prelude.Option.Some", vec![owned])
        }
        (None, true) => heap.construct("Prelude.Option.None", vec![]),
        (Some(e), false) => {
            let owned = heap.dup(&e)?;
            heap.construct("Prelude.Result.Ok", vec![owned])
        }
        (None, false) => {
            let c = heap.dup(cause)?;
            heap.construct("Prelude.Result.Err", vec![c])
        }
    }
}

pub(super) fn cause_find_defect(heap: &mut Heap, cause: &Value) -> Result<Value, Error> {
    let found = expect_cause_reasons(heap, cause)?
        .iter()
        .find_map(|r| match r {
            StoredReason::Die(d) => Some(describe_defect(heap, d)),
            _ => None,
        })
        .transpose()?;
    if let Some(msg) = found {
        let s = heap.literal(&Lit::String(msg))?;
        heap.construct("Prelude.Result.Ok", vec![s])
    } else {
        let c = heap.dup(cause)?;
        heap.construct("Prelude.Result.Err", vec![c])
    }
}

pub(super) fn cause_interruptors(
    heap: &mut Heap,
    cause: &Value,
    filter: bool,
) -> Result<Value, Error> {
    let rs = expect_cause_reasons(heap, cause)?;
    let has_any = rs
        .iter()
        .any(|r| matches!(r, StoredReason::Interrupt { .. }));
    if filter && !has_any {
        let c = heap.dup(cause)?;
        return heap.construct("Prelude.Result.Err", vec![c]);
    }
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for r in rs {
        if let StoredReason::Interrupt { fiber_id: Some(id) } = *r
            && id != 0
            && seen.insert(id)
        {
            #[expect(clippy::cast_precision_loss, reason = "fiber ids are small")]
            ids.push(Value::Float(id as f64));
        }
    }
    let list = heap.construct("$list", ids)?;
    if filter {
        heap.construct("Prelude.Result.Ok", vec![list])
    } else {
        Ok(list)
    }
}

pub(super) fn cause_stack_trace(heap: &mut Heap, cause: &Value) -> Result<Value, Error> {
    let Value::Object(tok) = cause else {
        return Err(Error::ExpectedCause);
    };
    let Data::Cause(data) = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedCause);
    };
    match data.trace.as_ref().map(Value::representation) {
        Some(rep) => {
            let dup = heap.dup(&rep)?;
            heap.construct("Prelude.Option.Some", vec![dup])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

pub(super) fn cause_annotate(
    heap: &mut Heap,
    overwrite_val: &Value,
    frame: &Value,
    cause: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let Value::Bool(overwrite) = *overwrite_val else {
        return Err(Error::ExpectedBoolean);
    };
    let Value::Object(tok) = cause else {
        return Err(Error::ExpectedCause);
    };
    let old_trace_rep = {
        let Data::Cause(data) = &heap.object(tok.id)?.data else {
            return Err(Error::ExpectedCause);
        };
        data.trace.as_ref().map(Value::representation)
    };
    if !overwrite && old_trace_rep.is_some() {
        return heap.dup(cause);
    }
    let snap = snapshot_reasons(heap, cause)?;
    charge(fuel, snap.len())?;
    let mut out = Vec::with_capacity(snap.len());
    for (kind, val, fiber_id) in snap {
        let next = match kind {
            ReasonKind::Fail => heap.dup(&val.expect("fail")).map(StoredReason::Fail),
            ReasonKind::Die => heap.dup(&val.expect("die")).map(StoredReason::Die),
            ReasonKind::Interrupt => Ok(StoredReason::Interrupt { fiber_id }),
        };
        match next {
            Ok(r) => out.push(r),
            Err(err) => {
                drop_stored(heap, out)?;
                return Err(err);
            }
        }
    }
    let new_trace = match heap.dup(frame) {
        Ok(f) => f,
        Err(err) => {
            drop_stored(heap, out)?;
            return Err(err);
        }
    };
    Ok(heap.allocate(Data::Cause(CauseData {
        reasons: out,
        trace: Some(new_trace),
    })))
}

pub(super) fn cause_replace_first_fail(
    heap: &mut Heap,
    new_err: &Value,
    cause: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let snap = snapshot_reasons(heap, cause)?;
    charge(fuel, snap.len())?;
    let mut out = Vec::with_capacity(snap.len());
    let mut replaced = false;
    for (kind, val, fiber_id) in snap {
        let next = match kind {
            ReasonKind::Fail if !replaced => {
                replaced = true;
                heap.dup(new_err).map(StoredReason::Fail)
            }
            ReasonKind::Fail => heap.dup(&val.expect("fail")).map(StoredReason::Fail),
            ReasonKind::Die => heap.dup(&val.expect("die")).map(StoredReason::Die),
            ReasonKind::Interrupt => Ok(StoredReason::Interrupt { fiber_id }),
        };
        match next {
            Ok(r) => out.push(r),
            Err(err) => {
                drop_stored(heap, out)?;
                return Err(err);
            }
        }
    }
    Ok(heap.allocate(Data::Cause(CauseData::plain(out))))
}

pub(in crate::tier0) fn first_defect_message(
    heap: &Heap,
    cause_id: ObjectId,
) -> Result<Option<String>, Error> {
    let Data::Cause(data) = &heap.object(cause_id)?.data else {
        return Err(Error::ExpectedCause);
    };
    for r in &data.reasons {
        if let StoredReason::Die(d) = r {
            return Ok(Some(describe_defect(heap, d)?));
        }
    }
    Ok(None)
}
