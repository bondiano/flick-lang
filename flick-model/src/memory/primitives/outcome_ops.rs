//! Counted `Exit` pure primitive implementations and shared outcome helpers.

use flick_core::ir::Lit;
use flick_core::ir::memory::OutcomeIntrinsic;

use super::cause_ops::{append_cause_reasons, drop_stored, run_cause};
use super::list_ops::slice_reps;
use super::{Data, Error, ExitData, Heap, Value, charge};
use crate::memory::causes::{Reason, StoredReason};

pub(super) fn run(
    heap: &mut Heap,
    op: OutcomeIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        OutcomeIntrinsic::ExitVoid => Ok(heap.allocate(Data::Exit(ExitData::Success(Value::Unit)))),
        OutcomeIntrinsic::CauseEmpty => Ok(heap.allocate(Data::Cause(Vec::new()))),
        OutcomeIntrinsic::ExitValue => exit_value(heap, &args[0]),
        OutcomeIntrinsic::ExitFailure => exit_failure(heap, &args[0]),
        OutcomeIntrinsic::ExitSucceed => {
            let v = heap.dup(&args[0])?;
            Ok(heap.allocate(Data::Exit(ExitData::Success(v))))
        }
        OutcomeIntrinsic::ExitFail => {
            let cause = cause_of_single(heap, Reason::Fail(&args[0]), fuel)?;
            Ok(heap.allocate(Data::Exit(ExitData::Failure(cause))))
        }
        OutcomeIntrinsic::ExitFailCause => {
            expect_cause_reasons(heap, &args[0])?;
            let c = heap.dup(&args[0])?;
            Ok(heap.allocate(Data::Exit(ExitData::Failure(c))))
        }
        OutcomeIntrinsic::ExitDie => {
            let cause = cause_of_single(heap, Reason::Die(&args[0]), fuel)?;
            Ok(heap.allocate(Data::Exit(ExitData::Failure(cause))))
        }
        OutcomeIntrinsic::ExitInterrupt => {
            let fiber_id = decode_fiber_id(heap, &args[0])?;
            let cause = cause_of_single(heap, Reason::Interrupt { fiber_id }, fuel)?;
            Ok(heap.allocate(Data::Exit(ExitData::Failure(cause))))
        }
        OutcomeIntrinsic::ExitHasFails => exit_has(heap, &args[0], ReasonKind::Fail),
        OutcomeIntrinsic::ExitHasDies => exit_has(heap, &args[0], ReasonKind::Die),
        OutcomeIntrinsic::ExitFilterSuccess => exit_filter(heap, &args[0], true),
        OutcomeIntrinsic::ExitFilterValue => exit_filter_value(heap, &args[0]),
        OutcomeIntrinsic::ExitFilterCause => exit_filter_cause(heap, &args[0]),
        OutcomeIntrinsic::ExitFindError => exit_find_error(heap, &args[0], false),
        OutcomeIntrinsic::ExitFindErrorOption => exit_find_error(heap, &args[0], true),
        OutcomeIntrinsic::ExitFindDefect => exit_find_defect(heap, &args[0]),
        OutcomeIntrinsic::ExitAsVoid => exit_as_void(heap, &args[0]),
        OutcomeIntrinsic::ExitAsVoidAll => exit_as_void_all(heap, &args[0], fuel),
        _ => run_cause(heap, op, args, fuel),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReasonKind {
    Fail,
    Die,
    Interrupt,
}

pub(super) fn matches_kind(r: &StoredReason, kind: ReasonKind) -> bool {
    matches!(
        (r, kind),
        (StoredReason::Fail(_), ReasonKind::Fail)
            | (StoredReason::Die(_), ReasonKind::Die)
            | (StoredReason::Interrupt { .. }, ReasonKind::Interrupt)
    )
}

pub(super) fn expect_cause_reasons<'a>(
    heap: &'a Heap,
    val: &Value,
) -> Result<&'a [StoredReason], Error> {
    let Value::Object(tok) = val else {
        return Err(Error::ExpectedCause);
    };
    let Data::Cause(reasons) = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedCause);
    };
    Ok(reasons)
}

pub(super) fn expect_reason<'a>(heap: &'a Heap, val: &Value) -> Result<&'a StoredReason, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::ExpectedCause);
    };
    let Data::CauseReason(r) = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedCause);
    };
    Ok(r)
}

fn expect_exit<'a>(heap: &'a Heap, val: &Value) -> Result<&'a ExitData, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::ExpectedExit);
    };
    let Data::Exit(data) = &heap.object(tok.id)?.data else {
        return Err(Error::ExpectedExit);
    };
    Ok(data)
}

pub(super) fn cause_of_single(
    heap: &mut Heap,
    reason: Reason<'_>,
    fuel: &mut u64,
) -> Result<Value, Error> {
    Ok(heap.cause(&[reason], fuel)?.into_value())
}

pub(super) fn decode_fiber_id(heap: &Heap, opt: &Value) -> Result<Option<usize>, Error> {
    let tag = heap.tag(opt)?;
    if tag == "Prelude.Option.Some" {
        let field = heap.field(opt, 0)?;
        let n = match *field {
            Value::Float(f) if f >= 0.0 && f.is_finite() =>
            {
                #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Some(f as usize)
            }
            Value::Int(i) => usize::try_from(i).ok(),
            _ => None,
        };
        Ok(n.filter(|&id| id != 0))
    } else {
        Ok(None)
    }
}

pub(in crate::memory) fn describe_defect(heap: &Heap, val: &Value) -> Result<String, Error> {
    if let Value::Object(tok) = val {
        match &heap.object(tok.id)?.data {
            Data::Defect(spec) => return Ok(spec.message()),
            Data::String(s) => return Ok(strip_js_error_prefix(s)),
            Data::Utf16(u) => return Ok(strip_js_error_prefix(&String::from_utf16_lossy(u))),
            Data::Tagged { tag, .. } => {
                return Ok(tag.rsplit('.').next().unwrap_or(tag).to_owned());
            }
            _ => {}
        }
    }
    super::show_value(heap, val)
}

fn strip_js_error_prefix(s: &str) -> String {
    let head = s
        .split("\n    at ")
        .next()
        .unwrap_or(s)
        .trim_end_matches('\n');
    if let Some((name, msg)) = head.split_once(": ")
        && matches!(
            name,
            "Error"
                | "TypeError"
                | "RangeError"
                | "SyntaxError"
                | "ReferenceError"
                | "URIError"
                | "EvalError"
                | "AggregateError"
        )
    {
        return msg.to_owned();
    }
    head.to_owned()
}

fn exit_value(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let rep = match expect_exit(heap, val)? {
        ExitData::Success(v) => Some(v.representation()),
        ExitData::Failure(_) => None,
    };
    match rep {
        Some(r) => {
            let owned = heap.dup(&r)?;
            heap.construct("Prelude.Option.Some", vec![owned])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn exit_failure(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let rep = match expect_exit(heap, val)? {
        ExitData::Success(_) => None,
        ExitData::Failure(c) => Some(c.representation()),
    };
    match rep {
        Some(r) => {
            let owned = heap.dup(&r)?;
            heap.construct("Prelude.Option.Some", vec![owned])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

fn exit_has(heap: &Heap, val: &Value, kind: ReasonKind) -> Result<Value, Error> {
    let has = match expect_exit(heap, val)? {
        ExitData::Success(_) => false,
        ExitData::Failure(cause) => expect_cause_reasons(heap, cause)?
            .iter()
            .any(|r| matches_kind(r, kind)),
    };
    Ok(Value::Bool(has))
}

fn exit_filter(heap: &mut Heap, val: &Value, want_success: bool) -> Result<Value, Error> {
    let is_ok = matches!(expect_exit(heap, val)?, ExitData::Success(_)) == want_success;
    let dup = heap.dup(val)?;
    heap.construct(
        if is_ok {
            "Prelude.Result.Ok"
        } else {
            "Prelude.Result.Err"
        },
        vec![dup],
    )
}

fn exit_filter_value(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let rep = match expect_exit(heap, val)? {
        ExitData::Success(v) => Some(v.representation()),
        ExitData::Failure(_) => None,
    };
    if let Some(r) = rep {
        let owned = heap.dup(&r)?;
        heap.construct("Prelude.Result.Ok", vec![owned])
    } else {
        let owned = heap.dup(val)?;
        heap.construct("Prelude.Result.Err", vec![owned])
    }
}

fn exit_filter_cause(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let rep = match expect_exit(heap, val)? {
        ExitData::Success(_) => None,
        ExitData::Failure(c) => Some(c.representation()),
    };
    if let Some(r) = rep {
        let owned = heap.dup(&r)?;
        heap.construct("Prelude.Result.Ok", vec![owned])
    } else {
        let owned = heap.dup(val)?;
        heap.construct("Prelude.Result.Err", vec![owned])
    }
}

fn exit_find_error(heap: &mut Heap, val: &Value, as_option: bool) -> Result<Value, Error> {
    let found = match expect_exit(heap, val)? {
        ExitData::Success(_) => None,
        ExitData::Failure(cause) => {
            expect_cause_reasons(heap, cause)?
                .iter()
                .find_map(|r| match r {
                    StoredReason::Fail(e) => Some(e.representation()),
                    _ => None,
                })
        }
    };
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
            let owned = heap.dup(val)?;
            heap.construct("Prelude.Result.Err", vec![owned])
        }
    }
}

fn exit_find_defect(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    let found = match expect_exit(heap, val)? {
        ExitData::Success(_) => None,
        ExitData::Failure(cause) => expect_cause_reasons(heap, cause)?
            .iter()
            .find_map(|r| match r {
                StoredReason::Die(d) => Some(describe_defect(heap, d)),
                _ => None,
            })
            .transpose()?,
    };
    if let Some(msg) = found {
        let s = heap.literal(&Lit::String(msg))?;
        heap.construct("Prelude.Result.Ok", vec![s])
    } else {
        let owned = heap.dup(val)?;
        heap.construct("Prelude.Result.Err", vec![owned])
    }
}

fn exit_as_void(heap: &mut Heap, val: &Value) -> Result<Value, Error> {
    match expect_exit(heap, val)? {
        ExitData::Success(_) => Ok(heap.allocate(Data::Exit(ExitData::Success(Value::Unit)))),
        ExitData::Failure(_) => heap.dup(val),
    }
}

fn exit_as_void_all(heap: &mut Heap, list_val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    charge(fuel, reps.len())?;
    let mut stored = Vec::new();
    let mut had_failure = false;
    for rep in &reps {
        let cause_rep = match expect_exit(heap, rep)? {
            ExitData::Success(_) => None,
            ExitData::Failure(c) => Some(c.representation()),
        };
        if let Some(c) = cause_rep {
            had_failure = true;
            if let Err(err) = append_cause_reasons(heap, &c, &mut stored) {
                drop_stored(heap, stored)?;
                return Err(err);
            }
        }
    }
    if !had_failure {
        return Ok(heap.allocate(Data::Exit(ExitData::Success(Value::Unit))));
    }
    let cause = heap.allocate(Data::Cause(stored));
    Ok(heap.allocate(Data::Exit(ExitData::Failure(cause))))
}
