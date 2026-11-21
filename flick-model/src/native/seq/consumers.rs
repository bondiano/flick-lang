//! Sequence consumers (`to_list`, `fold`, `any`, `all`, `find`, `first`,
//! `each_effect`, `fold_effect`).

use std::rc::Rc;

use crate::syntax::{Lit, Term};

use super::super::{cond, model};
use super::{match_step, none, some};

pub(in crate::native) fn to_list(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::apps(
        model("seqToListAcc"),
        [Term::unit(), args[0].clone()],
    ))
}

pub(in crate::native) fn to_list_acc(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (rev, xs) = (args[0].clone(), args[1].clone());
    let done = Term::app(model("seqRevToList"), rev.clone());
    let pushed = Term::Tuple(Rc::from([Term::Var("$x".into()), rev]));
    let next = Term::apps(model("seqToListAcc"), [pushed, Term::Var("$rest".into())]);
    Ok(match_step(xs, done, "$x", "$rest", next))
}

pub(in crate::native) fn rev_to_list(args: &[Term], _: &mut String) -> Result<Term, String> {
    let mut out = Vec::new();
    let mut cur = &args[0];
    while let Term::Tuple(pair) = cur {
        let [head, tail] = &**pair else {
            return Err(format!("malformed sequence accumulator: {cur:?}"));
        };
        out.push(head.clone());
        cur = tail;
    }
    if !matches!(cur, Term::Lit(Lit::Unit)) {
        return Err(format!("malformed sequence accumulator tail: {cur:?}"));
    }
    out.reverse();
    Ok(Term::List(out.into()))
}

pub(in crate::native) fn fold(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::apps(
        model("seqFoldStep"),
        [args[0].clone(), args[1].clone(), args[2].clone()],
    ))
}

pub(in crate::native) fn fold_effect(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::apps(
        model("seqFoldStep"),
        [args[0].clone(), args[1].clone(), args[2].clone()],
    ))
}

pub(in crate::native) fn fold_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, acc, xs) = (args[0].clone(), args[1].clone(), args[2].clone());
    let next = Term::apps(
        model("seqFoldStep"),
        [
            f.clone(),
            Term::apps(f, [acc.clone(), Term::Var("$x".into())]),
            Term::Var("$rest".into()),
        ],
    );
    Ok(match_step(xs, acc, "$x", "$rest", next))
}

pub(in crate::native) fn any(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::apps(
        model("seqAnyStep"),
        [args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn any_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (p, xs) = (args[0].clone(), args[1].clone());
    let next = Term::apps(model("seqAnyStep"), [p.clone(), Term::Var("$rest".into())]);
    Ok(match_step(
        xs,
        Term::bool(false),
        "$x",
        "$rest",
        cond(Term::app(p, Term::Var("$x".into())), Term::bool(true), next),
    ))
}

pub(in crate::native) fn all(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::apps(
        model("seqAllStep"),
        [args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn all_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (p, xs) = (args[0].clone(), args[1].clone());
    let next = Term::apps(model("seqAllStep"), [p.clone(), Term::Var("$rest".into())]);
    Ok(match_step(
        xs,
        Term::bool(true),
        "$x",
        "$rest",
        cond(
            Term::app(p, Term::Var("$x".into())),
            next,
            Term::bool(false),
        ),
    ))
}

pub(in crate::native) fn find(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::apps(
        model("seqFindStep"),
        [args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn find_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (p, xs) = (args[0].clone(), args[1].clone());
    let next = Term::apps(model("seqFindStep"), [p.clone(), Term::Var("$rest".into())]);
    Ok(match_step(
        xs,
        none(),
        "$x",
        "$rest",
        cond(
            Term::app(p, Term::Var("$x".into())),
            some(Term::Var("$x".into())),
            next,
        ),
    ))
}

pub(in crate::native) fn first(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(match_step(
        args[0].clone(),
        none(),
        "$x",
        "$rest",
        some(Term::Var("$x".into())),
    ))
}

pub(in crate::native) fn each_effect(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(Term::apps(
        model("seqEachStep"),
        [args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn each_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, xs) = (args[0].clone(), args[1].clone());
    let body = Term::Let(
        "_".into(),
        None,
        Rc::new(Term::app(f.clone(), Term::Var("$x".into()))),
        Rc::new(Term::apps(
            model("seqEachStep"),
            [f, Term::Var("$rest".into())],
        )),
    );
    Ok(match_step(xs, Term::unit(), "$x", "$rest", body))
}
