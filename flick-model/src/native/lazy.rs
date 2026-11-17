//! `lazy_make` and `lazy_force` (`docs/formal/lazy.md` §3) as terms over a
//! `Ref` cell: on the core's one thread an initializer runs at most once and
//! its outcome is kept. A cell holds `(0, task)` while pending, `(1, ())`
//! while running and `(2, outcome)` once done; an outcome is `(0, value)` or
//! `(1, error)`. A defect stops the machine, so none is kept.

use std::rc::Rc;

use crate::syntax::{Arm, Const, Lit, Pat, Term};

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
        tagged(0, args[0].clone()),
    ))
}

/// `Ref.make (2, (0, value))`: a cell already done with `value`.
pub(super) fn lazy_done(args: &[Term], _: &mut String) -> Result<Term, String> {
    let done = tagged(2, tagged(0, args[0].clone()));
    Ok(Term::app(Term::konst(Const::RefMake), done))
}

/// The outcome of a pending cell, after running its task; the kept one of a
/// done cell; a defect for a running one, which only its own initializer can
/// force on one thread.
pub(super) fn lazy_force(args: &[Term], _: &mut String) -> Result<Term, String> {
    let cell = || Term::Var("$cell".into());
    let set = |value: Term| Term::apps(Term::konst(Const::RefSet), [cell(), value]);
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
    let attempt = Term::Try(
        Rc::new(tagged(0, Term::Await(Rc::new(Term::Var("$task".into()))))),
        Rc::from([arm(
            Pat::Var("$failure".into()),
            tagged(1, Term::Var("$failure".into())),
        )]),
    );
    let run = seq(
        set(tagged(1, Term::Lit(Lit::Unit))),
        Term::Let(
            "$outcome".into(),
            None,
            Rc::new(attempt),
            Rc::new(seq(
                set(tagged(2, Term::Var("$outcome".into()))),
                out(Term::Var("$outcome".into())),
            )),
        ),
    );
    let state = Term::app(Term::konst(Const::RefGet), cell());
    let force = Term::Match(
        Rc::new(state),
        Rc::from([
            arm(tag_pat(Some(0), "$task"), run),
            arm(tag_pat(Some(2), "$kept"), out(Term::Var("$kept".into()))),
            arm(
                Pat::Wild,
                Term::Die("lazy forced during its own initialization".into()),
            ),
        ]),
    );
    Ok(Term::Let(
        "$cell".into(),
        None,
        Rc::new(args[0].clone()),
        Rc::new(force),
    ))
}
