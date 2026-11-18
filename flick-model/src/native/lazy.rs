//! `lazy_make` and `lazy_force` (`docs/formal/lazy.md` §3) as terms over a
//! `Ref` cell: on the core's one thread an initializer runs at most once per
//! attempt and its outcome is kept. A cell holds `(0, task)` while pending,
//! `(1, task)` while running and `(2, outcome)` once done; an outcome is
//! `(0, value)` or `(1, error)`. The task runs in the frame `settle ℓ`, which
//! records how it ends: a value or an error is the outcome, a handler's
//! signal makes the cell pending again. A defect stops the machine, so none
//! is kept.

use std::rc::Rc;

use crate::syntax::{Arm, Const, Lit, Pat, Term};

const PENDING: i32 = 0;
const RUNNING: i32 = 1;
const DONE: i32 = 2;

fn int(n: i32) -> Term {
    Term::Lit(Lit::Int(n))
}

fn tagged(tag: i32, payload: Term) -> Term {
    Term::Tuple(Rc::from([int(tag), payload]))
}

fn tag_pat(tag: Option<i32>, payload: &str) -> Pat {
    let tag = tag.map_or(Pat::Wild, |n| Pat::Lit(Lit::Int(n)));
    Pat::Tuple(Rc::from([tag, Pat::Var(payload.into())]))
}

fn arm(pat: Pat, body: Term) -> Arm {
    Arm {
        pat,
        guard: None,
        body,
    }
}

fn seq(first: Term, rest: Term) -> Term {
    Term::Let("_".into(), None, Rc::new(first), Rc::new(rest))
}

/// `Ref.make (0, task)`: nothing runs yet.
pub(super) fn lazy_make(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::app(
        Term::konst(Const::RefMake),
        tagged(PENDING, args[0].clone()),
    ))
}

/// `Ref.make (2, (0, value))`: a cell already done with `value`.
pub(super) fn lazy_done(args: &[Term], _: &mut String) -> Result<Term, String> {
    let done = tagged(DONE, tagged(0, args[0].clone()));
    Ok(Term::app(Term::konst(Const::RefMake), done))
}

/// The state of a running cell whose task ended with `ended`: the outcome,
/// or pending again after a handler's signal. `None` for a cell that does not
/// run.
pub(crate) fn settled(state: &Term, ended: Result<&Term, &Term>) -> Option<Term> {
    let Term::Tuple(state) = state else {
        return None;
    };
    let [Term::Lit(Lit::Int(RUNNING)), task] = &**state else {
        return None;
    };
    Some(match ended {
        Ok(v) => tagged(DONE, tagged(0, v.clone())),
        Err(Term::Signal(_)) => tagged(PENDING, task.clone()),
        Err(e) => tagged(DONE, tagged(1, e.clone())),
    })
}

/// A pending cell runs its task in `settle ℓ`; a done one gives its kept
/// outcome; a running one is a defect, which only its own initializer can
/// force on one thread.
pub(super) fn lazy_force(args: &[Term], _: &mut String) -> Result<Term, String> {
    let Term::Loc(l) = args[0] else {
        return Err(format!("`await` of {:?}, which is no lazy", args[0]));
    };
    let task = || Term::Var("$task".into());
    let out = |outcome: Term| {
        Term::Match(
            Rc::new(outcome),
            Rc::from([
                arm(tag_pat(Some(0), "$value"), Term::Var("$value".into())),
                arm(
                    tag_pat(None, "$error"),
                    Term::Raise(Rc::new(Term::Var("$error".into()))),
                ),
            ]),
        )
    };
    let start = Term::apps(
        Term::konst(Const::RefSet),
        [args[0].clone(), tagged(RUNNING, task())],
    );
    let run = seq(
        start,
        Term::Settle(l, Rc::new(Term::Await(Rc::new(task())))),
    );
    let state = Term::app(Term::konst(Const::RefGet), args[0].clone());
    Ok(Term::Match(
        Rc::new(state),
        Rc::from([
            arm(tag_pat(Some(PENDING), "$task"), run),
            arm(tag_pat(Some(DONE), "$kept"), out(Term::Var("$kept".into()))),
            arm(
                Pat::Wild,
                Term::Die("lazy forced during its own initialization".into()),
            ),
        ]),
    ))
}
