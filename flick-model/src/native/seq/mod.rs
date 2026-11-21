//! Lazy, repeatable sequences (`packages/core/src/Seq.fl`, `runtime/src/core/seq.ts`,
//! `runtime-go/flickrt/seq.go`) in the reference model.
//!
//! A `Seq a` value is `Term::Apply(Head::Const(Const::SeqThunk), [step])`, where
//! `step : Unit -> Option (a, Seq a)` is a closed pure function (either a λ or a
//! partially applied `$model` step native). Every traversal pulls one element at
//! a time via `Const::SeqNext`, preserving order, supporting repeated iteration
//! of the same `Seq` value, and terminating on bounded consumption of infinite
//! sequences without materializing them.

mod consumers;
mod transforms;

use std::cmp::Ordering;
use std::rc::Rc;

use crate::syntax::{Arm, Const, Head, Lit, Pat, Term};

use super::{data, list, model, prelude};

pub(in crate::native) use consumers::*;
pub(in crate::native) use transforms::*;

fn int(n: i32) -> Term {
    Term::Lit(Lit::Int(n))
}

fn int_val(t: &Term) -> Result<i32, String> {
    match t {
        Term::Lit(Lit::Int(n)) => Ok(*n),
        other => Err(format!("not an Int: {other:?}")),
    }
}

fn none() -> Term {
    data(prelude("Option", "None", 0), Vec::new())
}

fn some(v: Term) -> Term {
    Term::app(Term::konst_con(prelude("Option", "Some", 1)), v)
}

fn some_pair(head: Term, tail: Term) -> Term {
    some(Term::Tuple(Rc::from([head, tail])))
}

fn seq_thunk(step_fn: Term) -> Term {
    Term::Apply(Head::Const(Const::SeqThunk), Rc::from([step_fn]))
}

fn seq_next(s: Term) -> Term {
    Term::app(Term::konst(Const::SeqNext), s)
}

fn empty_seq() -> Term {
    seq_thunk(Term::lam("_", none()))
}

fn partial(export: &str, args: Vec<Term>) -> Term {
    let n = super::find(super::MODEL, export).expect("a `$model` sequence step native");
    debug_assert!(args.len() < n.arity);
    Term::Apply(Head::Const(Const::Native(n)), args.into())
}

fn step_thunk(export: &str, args: Vec<Term>) -> Term {
    seq_thunk(partial(export, args))
}

fn step_call(export: &str, mut args: Vec<Term>) -> Term {
    args.push(Term::unit());
    Term::apps(model(export), args)
}

/// `match SeqNext s with | None -> on_none | Some (head, tail) -> on_some`.
fn match_step(s: Term, on_none: Term, head: &str, tail: &str, on_some: Term) -> Term {
    let none_pat = Pat::Con(prelude("Option", "None", 0), Rc::from([]));
    let some_pat = Pat::Con(
        prelude("Option", "Some", 1),
        Rc::from([Pat::Tuple(Rc::from([
            Pat::Var(head.into()),
            Pat::Var(tail.into()),
        ]))]),
    );
    Term::Match(
        Rc::new(seq_next(s)),
        Rc::from([
            Arm {
                pat: none_pat,
                guard: None,
                body: on_none,
            },
            Arm {
                pat: some_pat,
                guard: None,
                body: on_some,
            },
        ]),
    )
}

/// Converts a `Term::List` value to a lazy `Seq` without copying its elements.
pub(crate) fn from_list_term(xs: Term) -> Term {
    step_thunk("seqListAt", vec![xs, int(0)])
}

/// Builds a lazy `Seq` value from a `Const::SeqSync` thunk.
pub(crate) fn sync_thunk(thunk: Term) -> Term {
    seq_thunk(Term::lam("_", seq_next(Term::app(thunk, Term::unit()))))
}

/// Lazy `flat_map` of `f` over `xs`.
pub(crate) fn flat_map_term(f: Term, xs: Term) -> Term {
    step_thunk("seqFlatMapStep", vec![f, xs, empty_seq()])
}

pub(in crate::native) fn of_list(args: &[Term], _: &mut String) -> Result<Term, String> {
    let _ = list(&args[0])?;
    Ok(from_list_term(args[0].clone()))
}

pub(in crate::native) fn list_at(args: &[Term], _: &mut String) -> Result<Term, String> {
    let items = list(&args[0])?;
    let idx = int_val(&args[1])?;
    let Ok(index) = usize::try_from(idx) else {
        return Ok(none());
    };
    match items.get(index) {
        Some(item) => Ok(some_pair(
            item.clone(),
            step_thunk("seqListAt", vec![args[0].clone(), int(idx + 1)]),
        )),
        None => Ok(none()),
    }
}

pub(in crate::native) fn from(args: &[Term], _: &mut String) -> Result<Term, String> {
    let _ = int_val(&args[0])?;
    Ok(step_thunk("seqFromStep", vec![args[0].clone()]))
}

pub(in crate::native) fn from_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let i = int_val(&args[0])?;
    Ok(some_pair(
        int(i),
        step_thunk("seqFromStep", vec![int(i.wrapping_add(1))]),
    ))
}

pub(in crate::native) fn range(args: &[Term], _: &mut String) -> Result<Term, String> {
    let _ = (int_val(&args[0])?, int_val(&args[1])?);
    Ok(step_thunk(
        "seqRangeStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn range_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (a, b) = (int_val(&args[0])?, int_val(&args[1])?);
    if a < b {
        Ok(some_pair(
            int(a),
            step_thunk("seqRangeStep", vec![int(a + 1), int(b)]),
        ))
    } else {
        Ok(none())
    }
}

pub(in crate::native) fn range_to(args: &[Term], _: &mut String) -> Result<Term, String> {
    let _ = (int_val(&args[0])?, int_val(&args[1])?);
    Ok(step_thunk(
        "seqRangeToStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn range_to_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (a, b) = (int_val(&args[0])?, int_val(&args[1])?);
    match a.cmp(&b) {
        Ordering::Less => Ok(some_pair(
            int(a),
            step_thunk("seqRangeToStep", vec![int(a + 1), int(b)]),
        )),
        Ordering::Equal => Ok(some_pair(int(a), empty_seq())),
        Ordering::Greater => Ok(none()),
    }
}

pub(in crate::native) fn range_by(args: &[Term], _: &mut String) -> Result<Term, String> {
    let _ = (int_val(&args[0])?, int_val(&args[1])?, int_val(&args[2])?);
    Ok(step_thunk(
        "seqRangeByStep",
        vec![args[0].clone(), args[1].clone(), args[2].clone()],
    ))
}

pub(in crate::native) fn range_by_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (a, b, step) = (int_val(&args[0])?, int_val(&args[1])?, int_val(&args[2])?);
    if step == 0 {
        return Ok(none());
    }
    let in_bounds = (step > 0 && a < b) || (step < 0 && a > b);
    if !in_bounds {
        return Ok(none());
    }
    let rest = match a.checked_add(step) {
        Some(next) => step_thunk("seqRangeByStep", vec![int(next), int(b), int(step)]),
        None => empty_seq(),
    };
    Ok(some_pair(int(a), rest))
}

pub(in crate::native) fn iterate(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqIterateStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn iterate_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, x) = (args[0].clone(), args[1].clone());
    // Compute `f x` lazily on the next pull, not when yielding `x`.
    let next = Term::lam(
        "_",
        step_call("seqIterateStep", vec![f.clone(), Term::app(f, x.clone())]),
    );
    Ok(some_pair(x, seq_thunk(next)))
}

pub(in crate::native) fn repeat(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk("seqRepeatStep", vec![args[0].clone()]))
}

pub(in crate::native) fn repeat_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(some_pair(
        args[0].clone(),
        step_thunk("seqRepeatStep", vec![args[0].clone()]),
    ))
}

pub(in crate::native) fn cycle(args: &[Term], _: &mut String) -> Result<Term, String> {
    let items = list(&args[0])?;
    if items.is_empty() {
        Ok(empty_seq())
    } else {
        Ok(step_thunk("seqCycleStep", vec![args[0].clone(), int(0)]))
    }
}

pub(in crate::native) fn cycle_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let items = list(&args[0])?;
    if items.is_empty() {
        return Ok(none());
    }
    let idx = usize::try_from(int_val(&args[1])?).unwrap_or(0) % items.len();
    let next = i32::try_from((idx + 1) % items.len()).unwrap_or(0);
    Ok(some_pair(
        items[idx].clone(),
        step_thunk("seqCycleStep", vec![args[0].clone(), int(next)]),
    ))
}

pub(in crate::native) fn unfold(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqUnfoldStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn unfold_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, seed) = (args[0].clone(), args[1].clone());
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqUnfoldStep"),
            [f.clone(), Term::Var("$next".into())],
        ),
    );
    Ok(Term::Match(
        Rc::new(Term::app(f, seed)),
        Rc::from([
            Arm {
                pat: Pat::Con(prelude("Option", "None", 0), Rc::from([])),
                guard: None,
                body: none(),
            },
            Arm {
                pat: Pat::Con(
                    prelude("Option", "Some", 1),
                    Rc::from([Pat::Tuple(Rc::from([
                        Pat::Var("$a".into()),
                        Pat::Var("$next".into()),
                    ]))]),
                ),
                guard: None,
                body: some_pair(Term::Var("$a".into()), next_seq),
            },
        ]),
    ))
}

pub(in crate::native) fn append(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqAppendStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn append_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (xs, ys) = (args[0].clone(), args[1].clone());
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqAppendStep"),
            [Term::Var("$rest".into()), ys.clone()],
        ),
    );
    Ok(match_step(
        xs,
        seq_next(ys),
        "$x",
        "$rest",
        some_pair(Term::Var("$x".into()), next_seq),
    ))
}
