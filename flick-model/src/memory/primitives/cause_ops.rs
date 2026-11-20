//! Counted `Cause` and `Failure` pure primitive implementations.

use std::collections::BTreeSet;

use flick_core::ir::Lit;
use flick_core::ir::memory::OutcomeIntrinsic;

use super::list_ops::slice_reps;
use super::outcome_ops::{
    ReasonKind, cause_of_single, decode_fiber_id, describe_defect, expect_cause_reasons,
    expect_reason, matches_kind,
};
use super::{Data, Error, Heap, ObjectId, Value, charge};
use crate::memory::causes::{CauseToken, Reason, StoredReason};

pub(super) fn run_cause(
    heap: &mut Heap,
    op: OutcomeIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    match op {
        OutcomeIntrinsic::CauseFail => cause_of_single(heap, Reason::Fail(&args[0]), fuel),
        OutcomeIntrinsic::CauseDie => cause_of_single(heap, Reason::Die(&args[0]), fuel),
        OutcomeIntrinsic::CauseInterrupt => {
            let fiber_id = decode_fiber_id(heap, &args[0])?;
            cause_of_single(heap, Reason::Interrupt { fiber_id }, fuel)
        }
        OutcomeIntrinsic::CauseFromReasons => cause_from_reasons(heap, &args[0], fuel),
        OutcomeIntrinsic::CauseReasons => cause_reasons_list(heap, &args[0], fuel),
        OutcomeIntrinsic::CauseMakeFailReason => {
            let v = heap.dup(&args[0])?;
            Ok(heap.allocate(Data::CauseReason(StoredReason::Fail(v))))
        }
        OutcomeIntrinsic::CauseMakeDieReason => {
            let v = heap.dup(&args[0])?;
            Ok(heap.allocate(Data::CauseReason(StoredReason::Die(v))))
        }
        OutcomeIntrinsic::CauseMakeInterruptReason => {
            let fiber_id = decode_fiber_id(heap, &args[0])?;
            Ok(heap.allocate(Data::CauseReason(StoredReason::Interrupt { fiber_id })))
        }
        OutcomeIntrinsic::CauseIsFailReason => Ok(Value::Bool(matches_kind(
            expect_reason(heap, &args[0])?,
            ReasonKind::Fail,
        ))),
        OutcomeIntrinsic::CauseIsDieReason => Ok(Value::Bool(matches_kind(
            expect_reason(heap, &args[0])?,
            ReasonKind::Die,
        ))),
        OutcomeIntrinsic::CauseIsInterruptReason => Ok(Value::Bool(matches_kind(
            expect_reason(heap, &args[0])?,
            ReasonKind::Interrupt,
        ))),
        OutcomeIntrinsic::CauseHasInterruptsOnly => {
            let rs = expect_cause_reasons(heap, &args[0])?;
            Ok(Value::Bool(
                !rs.is_empty()
                    && rs
                        .iter()
                        .all(|r| matches!(r, StoredReason::Interrupt { .. })),
            ))
        }
        OutcomeIntrinsic::CauseHasFails => Ok(Value::Bool(
            expect_cause_reasons(heap, &args[0])?
                .iter()
                .any(|r| matches_kind(r, ReasonKind::Fail)),
        )),
        OutcomeIntrinsic::CauseHasDies => Ok(Value::Bool(
            expect_cause_reasons(heap, &args[0])?
                .iter()
                .any(|r| matches_kind(r, ReasonKind::Die)),
        )),
        OutcomeIntrinsic::CauseHasInterrupts => Ok(Value::Bool(
            expect_cause_reasons(heap, &args[0])?
                .iter()
                .any(|r| matches_kind(r, ReasonKind::Interrupt)),
        )),
        OutcomeIntrinsic::CauseCombine => cause_combine(heap, &args[1], &args[0], fuel),
        OutcomeIntrinsic::CauseFindFail => cause_find_reason(heap, &args[0], ReasonKind::Fail),
        OutcomeIntrinsic::CauseFindError => cause_find_error(heap, &args[0], false),
        OutcomeIntrinsic::CauseFindErrorOption => cause_find_error(heap, &args[0], true),
        OutcomeIntrinsic::CauseFindDie => cause_find_reason(heap, &args[0], ReasonKind::Die),
        OutcomeIntrinsic::CauseFindDefect => cause_find_defect(heap, &args[0]),
        OutcomeIntrinsic::CauseFindInterrupt => {
            cause_find_reason(heap, &args[0], ReasonKind::Interrupt)
        }
        OutcomeIntrinsic::CauseInterruptors => cause_interruptors(heap, &args[0], false),
        OutcomeIntrinsic::CauseFilterInterruptors => cause_interruptors(heap, &args[0], true),
        OutcomeIntrinsic::CausePretty => cause_pretty(heap, &args[0], fuel),
        OutcomeIntrinsic::CausePrettyErrors => cause_pretty_errors(heap, &args[0], fuel),
        OutcomeIntrinsic::CauseNoneFrame => heap.construct("Prelude.Option.None", vec![]),
        OutcomeIntrinsic::CauseReplaceFirstFail => {
            cause_replace_first_fail(heap, &args[0], &args[1], fuel)
        }
        _ => unreachable!("handled in outcome_ops::run"),
    }
}

pub(super) fn drop_stored(heap: &mut Heap, stored: Vec<StoredReason>) -> Result<(), Error> {
    heap.drop_values(
        stored
            .into_iter()
            .filter_map(StoredReason::into_value)
            .collect(),
    )
}

type SnappedReason = (ReasonKind, Option<Value>, Option<usize>);

fn snapshot_reasons(heap: &Heap, cause: &Value) -> Result<Vec<SnappedReason>, Error> {
    Ok(expect_cause_reasons(heap, cause)?
        .iter()
        .map(|r| match r {
            StoredReason::Fail(v) => (ReasonKind::Fail, Some(v.representation()), None),
            StoredReason::Die(v) => (ReasonKind::Die, Some(v.representation()), None),
            StoredReason::Interrupt { fiber_id } => (ReasonKind::Interrupt, None, *fiber_id),
        })
        .collect())
}

pub(super) fn append_cause_reasons(
    heap: &mut Heap,
    cause: &Value,
    out: &mut Vec<StoredReason>,
) -> Result<(), Error> {
    for (kind, val, fiber_id) in snapshot_reasons(heap, cause)? {
        let item = match kind {
            ReasonKind::Fail => StoredReason::Fail(heap.dup(&val.expect("fail val"))?),
            ReasonKind::Die => StoredReason::Die(heap.dup(&val.expect("die val"))?),
            ReasonKind::Interrupt => StoredReason::Interrupt { fiber_id },
        };
        out.push(item);
    }
    Ok(())
}

fn cause_from_reasons(heap: &mut Heap, list_val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    charge(fuel, reps.len())?;
    let mut stored = Vec::with_capacity(reps.len());
    for rep in &reps {
        let snapped = match expect_reason(heap, rep)? {
            StoredReason::Fail(v) => (ReasonKind::Fail, Some(v.representation()), None),
            StoredReason::Die(v) => (ReasonKind::Die, Some(v.representation()), None),
            StoredReason::Interrupt { fiber_id } => (ReasonKind::Interrupt, None, *fiber_id),
        };
        let owned = match snapped.0 {
            ReasonKind::Fail => heap.dup(&snapped.1.expect("fail")).map(StoredReason::Fail),
            ReasonKind::Die => heap.dup(&snapped.1.expect("die")).map(StoredReason::Die),
            ReasonKind::Interrupt => Ok(StoredReason::Interrupt {
                fiber_id: snapped.2,
            }),
        };
        match owned {
            Ok(r) => stored.push(r),
            Err(err) => {
                drop_stored(heap, stored)?;
                return Err(err);
            }
        }
    }
    Ok(heap.allocate(Data::Cause(stored)))
}

fn cause_reasons_list(heap: &mut Heap, cause: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let snap = snapshot_reasons(heap, cause)?;
    charge(fuel, snap.len())?;
    let mut out = Vec::with_capacity(snap.len());
    let mut seen_fail = false;
    for (kind, val, fiber_id) in snap {
        let owned = match kind {
            ReasonKind::Fail if !seen_fail => {
                seen_fail = true;
                heap.dup(&val.expect("fail")).map(StoredReason::Fail)
            }
            ReasonKind::Fail | ReasonKind::Die => {
                heap.dup(&val.expect("die")).map(StoredReason::Die)
            }
            ReasonKind::Interrupt => Ok(StoredReason::Interrupt { fiber_id }),
        };
        match owned {
            Ok(r) => out.push(heap.allocate(Data::CauseReason(r))),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

fn same_stored_reason(heap: &Heap, a: &StoredReason, b: &StoredReason) -> bool {
    match (a, b) {
        (StoredReason::Fail(x), StoredReason::Fail(y)) => {
            (x.object_id().is_some() && x.object_id() == y.object_id())
                || super::equal(heap, x, y).unwrap_or(false)
        }
        (StoredReason::Interrupt { fiber_id: x }, StoredReason::Interrupt { fiber_id: y }) => {
            x == y
        }
        (StoredReason::Die(x), StoredReason::Die(y)) => {
            if x.object_id().is_some() && x.object_id() == y.object_id() {
                return true;
            }
            let is_thrown = |v: &Value| {
                v.object_id()
                    .and_then(|id| heap.object(id).ok())
                    .is_some_and(|o| matches!(o.data, Data::Defect(_)))
            };
            if is_thrown(x) || is_thrown(y) {
                return false;
            }
            super::equal(heap, x, y).unwrap_or(false)
        }
        _ => false,
    }
}

fn combine_cause_values(
    heap: &mut Heap,
    left: &Value,
    right: &Value,
    dedup: bool,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let mut snaps = snapshot_reasons(heap, left)?;
    snaps.extend(snapshot_reasons(heap, right)?);
    charge(fuel, snaps.len().max(1))?;
    let mut stored: Vec<StoredReason> = Vec::with_capacity(snaps.len());
    for (kind, val, fiber_id) in snaps {
        let probe = match kind {
            ReasonKind::Fail => StoredReason::Fail(val.as_ref().expect("fail").representation()),
            ReasonKind::Die => StoredReason::Die(val.as_ref().expect("die").representation()),
            ReasonKind::Interrupt => StoredReason::Interrupt { fiber_id },
        };
        if dedup
            && stored
                .iter()
                .any(|existing| same_stored_reason(heap, existing, &probe))
        {
            continue;
        }
        let owned = match kind {
            ReasonKind::Fail => heap
                .dup(val.as_ref().expect("fail"))
                .map(StoredReason::Fail),
            ReasonKind::Die => heap.dup(val.as_ref().expect("die")).map(StoredReason::Die),
            ReasonKind::Interrupt => Ok(StoredReason::Interrupt { fiber_id }),
        };
        match owned {
            Ok(item) => stored.push(item),
            Err(err) => {
                drop_stored(heap, stored)?;
                return Err(err);
            }
        }
    }
    Ok(heap.allocate(Data::Cause(stored)))
}

fn cause_combine(
    heap: &mut Heap,
    self_val: &Value,
    that_val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    combine_cause_values(heap, self_val, that_val, true, fuel)
}

pub(in crate::memory) fn concat_causes(
    heap: &mut Heap,
    left: &CauseToken,
    right: &CauseToken,
    fuel: &mut u64,
) -> Result<CauseToken, Error> {
    let lv = Value::Object(super::ObjectToken {
        id: left.object_id(),
    });
    let rv = Value::Object(super::ObjectToken {
        id: right.object_id(),
    });
    let out = combine_cause_values(heap, &lv, &rv, false, fuel)?;
    CauseToken::from_cause_value(heap, out)
}

pub(in crate::memory) fn join_causes(
    heap: &mut Heap,
    left: &CauseToken,
    right: &CauseToken,
    fuel: &mut u64,
) -> Result<CauseToken, Error> {
    let lv = Value::Object(super::ObjectToken {
        id: left.object_id(),
    });
    let rv = Value::Object(super::ObjectToken {
        id: right.object_id(),
    });
    let out = combine_cause_values(heap, &lv, &rv, true, fuel)?;
    CauseToken::from_cause_value(heap, out)
}

fn cause_find_reason(heap: &mut Heap, cause: &Value, kind: ReasonKind) -> Result<Value, Error> {
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

fn cause_find_error(heap: &mut Heap, cause: &Value, as_option: bool) -> Result<Value, Error> {
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

fn cause_find_defect(heap: &mut Heap, cause: &Value) -> Result<Value, Error> {
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

fn cause_interruptors(heap: &mut Heap, cause: &Value, filter: bool) -> Result<Value, Error> {
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

fn cause_pretty(heap: &mut Heap, cause: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let rs = expect_cause_reasons(heap, cause)?;
    charge(fuel, rs.len())?;
    let mut lines = Vec::new();
    for r in rs {
        match r {
            StoredReason::Fail(e) => lines.push(super::show_value(heap, e)?),
            StoredReason::Die(d) => lines.push(describe_defect(heap, d)?),
            StoredReason::Interrupt { .. } => {}
        }
    }
    let out = if lines.is_empty() {
        "InterruptError: All fibers interrupted without error".into()
    } else {
        lines.join("\n")
    };
    heap.literal(&Lit::String(out))
}

fn cause_pretty_errors(heap: &mut Heap, cause: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let rs = expect_cause_reasons(heap, cause)?;
    charge(fuel, rs.len())?;
    let mut texts = Vec::new();
    for r in rs {
        match r {
            StoredReason::Fail(e) => texts.push(format!("Error: {}", super::show_value(heap, e)?)),
            StoredReason::Die(d) => texts.push(format!("Error: {}", describe_defect(heap, d)?)),
            StoredReason::Interrupt { .. } => {
                texts.push("InterruptError: Fiber interrupted".into());
            }
        }
    }
    let mut vals = Vec::with_capacity(texts.len());
    for t in texts {
        match heap.literal(&Lit::String(t)) {
            Ok(v) => vals.push(v),
            Err(err) => {
                heap.drop_values(vals)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", vals)
}

fn cause_replace_first_fail(
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
    Ok(heap.allocate(Data::Cause(out)))
}

pub(in crate::memory) fn first_defect_message(
    heap: &Heap,
    cause_id: ObjectId,
) -> Result<Option<String>, Error> {
    let Data::Cause(rs) = &heap.object(cause_id)?.data else {
        return Err(Error::ExpectedCause);
    };
    for r in rs {
        if let StoredReason::Die(d) = r {
            return Ok(Some(describe_defect(heap, d)?));
        }
    }
    Ok(None)
}
