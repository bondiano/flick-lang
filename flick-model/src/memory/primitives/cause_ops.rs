//! Counted `Cause` and `Failure` pure primitive implementations.

use flick_core::ir::memory::OutcomeIntrinsic;

use super::list_ops::slice_reps;
use super::outcome_ops::{
    ReasonKind, cause_of_single, decode_fiber_id, expect_cause_reasons, expect_reason, matches_kind,
};
use super::{Data, Error, Heap, Value, charge};
use crate::memory::causes::{CauseData, CauseToken, Reason, StoredReason};

#[path = "cause_ops/query.rs"]
mod query;

pub(in crate::memory) use query::first_defect_message;

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
        OutcomeIntrinsic::CauseFindFail => {
            query::cause_find_reason(heap, &args[0], ReasonKind::Fail)
        }
        OutcomeIntrinsic::CauseFindError => query::cause_find_error(heap, &args[0], false),
        OutcomeIntrinsic::CauseFindErrorOption => query::cause_find_error(heap, &args[0], true),
        OutcomeIntrinsic::CauseFindDie => query::cause_find_reason(heap, &args[0], ReasonKind::Die),
        OutcomeIntrinsic::CauseFindDefect => query::cause_find_defect(heap, &args[0]),
        OutcomeIntrinsic::CauseFindInterrupt => {
            query::cause_find_reason(heap, &args[0], ReasonKind::Interrupt)
        }
        OutcomeIntrinsic::CauseInterruptors => query::cause_interruptors(heap, &args[0], false),
        OutcomeIntrinsic::CauseFilterInterruptors => {
            query::cause_interruptors(heap, &args[0], true)
        }
        OutcomeIntrinsic::CausePretty => super::cause_pretty::cause_pretty(heap, &args[0], fuel),
        OutcomeIntrinsic::CausePrettyErrors => {
            super::cause_pretty::cause_pretty_errors(heap, &args[0], fuel)
        }
        OutcomeIntrinsic::CauseNoneFrame => heap.construct("Prelude.Option.None", vec![]),
        OutcomeIntrinsic::CauseReplaceFirstFail => {
            query::cause_replace_first_fail(heap, &args[0], &args[1], fuel)
        }
        OutcomeIntrinsic::CauseAnnotate => {
            query::cause_annotate(heap, &args[0], &args[1], &args[2], fuel)
        }
        OutcomeIntrinsic::CauseStackTrace => query::cause_stack_trace(heap, &args[0]),
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
    Ok(heap.allocate(Data::Cause(CauseData::plain(stored))))
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
    if dedup {
        if expect_cause_reasons(heap, left)?.is_empty() {
            charge(fuel, 1)?;
            return heap.dup(right);
        }
        if expect_cause_reasons(heap, right)?.is_empty() {
            charge(fuel, 1)?;
            return heap.dup(left);
        }
    }
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
    Ok(heap.allocate(Data::Cause(CauseData::plain(stored))))
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

/// G7g (`docs/formal/runtime.md#an-interruption-that-skips-a-handler-erases-fail-effect`):
/// drop every `Fail` reason from `prev` and append `Interrupt { fiber_id }` if absent.
pub(in crate::memory) fn strip_fail_and_add_interrupt(
    heap: &mut Heap,
    prev: Option<&CauseToken>,
    fiber_id: Option<usize>,
    fuel: &mut u64,
) -> Result<CauseToken, Error> {
    let mut stored: Vec<StoredReason> = Vec::new();
    let mut has_int = false;
    if let Some(tok) = prev {
        let pv = Value::Object(super::ObjectToken {
            id: tok.object_id(),
        });
        let snaps = snapshot_reasons(heap, &pv)?;
        charge(fuel, snaps.len().max(1))?;
        for (kind, val, fid) in snaps {
            match kind {
                ReasonKind::Fail => {}
                ReasonKind::Die => match heap.dup(val.as_ref().expect("die")) {
                    Ok(v) => stored.push(StoredReason::Die(v)),
                    Err(err) => {
                        drop_stored(heap, stored)?;
                        return Err(err);
                    }
                },
                ReasonKind::Interrupt => {
                    if fid == fiber_id {
                        has_int = true;
                    }
                    stored.push(StoredReason::Interrupt { fiber_id: fid });
                }
            }
        }
    } else {
        charge(fuel, 1)?;
    }
    if !has_int {
        stored.push(StoredReason::Interrupt { fiber_id });
    }
    let out = heap.allocate(Data::Cause(CauseData::plain(stored)));
    CauseToken::from_cause_value(heap, out)
}
