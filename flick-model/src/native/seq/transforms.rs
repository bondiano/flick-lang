//! Lazy sequence transformers (`map`, `filter`, `flat_map`, `take`, `drop`,
//! `zip`, `enumerate`, `scan`, `chunks`, `windows`).

use std::iter;
use std::rc::Rc;

use crate::syntax::{Arm, Const, Pat, Term};

use super::super::{cond, list, model, prelude};
use super::{
    empty_seq, flat_map_term, int, int_val, match_step, none, seq_next, seq_thunk, some_pair,
    step_call, step_thunk,
};

pub(in crate::native) fn map(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqMapStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn map_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, xs) = (args[0].clone(), args[1].clone());
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(model("seqMapStep"), [f.clone(), Term::Var("$rest".into())]),
    );
    Ok(match_step(
        xs,
        none(),
        "$x",
        "$rest",
        some_pair(Term::app(f, Term::Var("$x".into())), next_seq),
    ))
}

pub(in crate::native) fn map2(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, xs, ys) = (args[0].clone(), args[1].clone(), args[2].clone());
    let mapper = Term::lam(
        "$x",
        Term::app(
            Term::konst(Const::SeqThunk),
            Term::apps(
                model("seqMapStep"),
                [
                    Term::lam(
                        "$y",
                        Term::apps(f, [Term::Var("$x".into()), Term::Var("$y".into())]),
                    ),
                    ys,
                ],
            ),
        ),
    );
    Ok(flat_map_term(mapper, xs))
}

pub(in crate::native) fn filter(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqFilterStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn filter_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (p, xs) = (args[0].clone(), args[1].clone());
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqFilterStep"),
            [p.clone(), Term::Var("$rest".into())],
        ),
    );
    let keep = some_pair(Term::Var("$x".into()), next_seq);
    let skip = step_call("seqFilterStep", vec![p.clone(), Term::Var("$rest".into())]);
    Ok(match_step(
        xs,
        none(),
        "$x",
        "$rest",
        cond(Term::app(p, Term::Var("$x".into())), keep, skip),
    ))
}

pub(in crate::native) fn filter_map(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqFilterMapStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn filter_map_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, xs) = (args[0].clone(), args[1].clone());
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqFilterMapStep"),
            [f.clone(), Term::Var("$rest".into())],
        ),
    );
    let skip = step_call(
        "seqFilterMapStep",
        vec![f.clone(), Term::Var("$rest".into())],
    );
    let matched = Term::Match(
        Rc::new(Term::app(f, Term::Var("$x".into()))),
        Rc::from([
            Arm {
                pat: Pat::Con(prelude("Option", "None", 0), Rc::from([])),
                guard: None,
                body: skip,
            },
            Arm {
                pat: Pat::Con(
                    prelude("Option", "Some", 1),
                    Rc::from([Pat::Var("$y".into())]),
                ),
                guard: None,
                body: some_pair(Term::Var("$y".into()), next_seq),
            },
        ]),
    );
    Ok(match_step(xs, none(), "$x", "$rest", matched))
}

pub(in crate::native) fn flat_map(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(flat_map_term(args[0].clone(), args[1].clone()))
}

pub(in crate::native) fn flat_map_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, outer, inner) = (args[0].clone(), args[1].clone(), args[2].clone());
    let next_from_inner = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqFlatMapStep"),
            [f.clone(), outer.clone(), Term::Var("$ri".into())],
        ),
    );
    let pull_outer = match_step(
        outer,
        none(),
        "$x",
        "$ro",
        step_call(
            "seqFlatMapStep",
            vec![
                f.clone(),
                Term::Var("$ro".into()),
                Term::app(f, Term::Var("$x".into())),
            ],
        ),
    );
    Ok(match_step(
        inner,
        pull_outer,
        "$y",
        "$ri",
        some_pair(Term::Var("$y".into()), next_from_inner),
    ))
}

pub(in crate::native) fn take(args: &[Term], _: &mut String) -> Result<Term, String> {
    let n = int_val(&args[0])?;
    if n <= 0 {
        Ok(empty_seq())
    } else {
        Ok(step_thunk(
            "seqTakeStep",
            vec![args[0].clone(), args[1].clone()],
        ))
    }
}

pub(in crate::native) fn take_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let n = int_val(&args[0])?;
    if n <= 0 {
        return Ok(none());
    }
    let rest_seq = if n == 1 {
        empty_seq()
    } else {
        Term::app(
            Term::konst(Const::SeqThunk),
            Term::apps(
                model("seqTakeStep"),
                [int(n - 1), Term::Var("$rest".into())],
            ),
        )
    };
    Ok(match_step(
        args[1].clone(),
        none(),
        "$x",
        "$rest",
        some_pair(Term::Var("$x".into()), rest_seq),
    ))
}

pub(in crate::native) fn take_while(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqTakeWhileStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn take_while_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (p, xs) = (args[0].clone(), args[1].clone());
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqTakeWhileStep"),
            [p.clone(), Term::Var("$rest".into())],
        ),
    );
    Ok(match_step(
        xs,
        none(),
        "$x",
        "$rest",
        cond(
            Term::app(p, Term::Var("$x".into())),
            some_pair(Term::Var("$x".into()), next_seq),
            none(),
        ),
    ))
}

pub(in crate::native) fn drop(args: &[Term], _: &mut String) -> Result<Term, String> {
    let n = int_val(&args[0])?;
    if n <= 0 {
        Ok(args[1].clone())
    } else {
        Ok(step_thunk(
            "seqDropStep",
            vec![args[0].clone(), args[1].clone()],
        ))
    }
}

pub(in crate::native) fn drop_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let n = int_val(&args[0])?;
    if n <= 0 {
        return Ok(seq_next(args[1].clone()));
    }
    Ok(match_step(
        args[1].clone(),
        none(),
        "$x",
        "$rest",
        step_call("seqDropStep", vec![int(n - 1), Term::Var("$rest".into())]),
    ))
}

pub(in crate::native) fn drop_while(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqDropWhileStep",
        vec![args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn drop_while_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (p, xs) = (args[0].clone(), args[1].clone());
    let skip = step_call(
        "seqDropWhileStep",
        vec![p.clone(), Term::Var("$rest".into())],
    );
    let keep = some_pair(Term::Var("$x".into()), Term::Var("$rest".into()));
    Ok(match_step(
        xs,
        none(),
        "$x",
        "$rest",
        cond(Term::app(p, Term::Var("$x".into())), skip, keep),
    ))
}

pub(in crate::native) fn zip(args: &[Term], _: &mut String) -> Result<Term, String> {
    let pair = Term::lam(
        "$a",
        Term::lam(
            "$b",
            Term::Tuple(Rc::from([Term::Var("$a".into()), Term::Var("$b".into())])),
        ),
    );
    Ok(step_thunk(
        "seqZipWithStep",
        vec![pair, args[0].clone(), args[1].clone()],
    ))
}

pub(in crate::native) fn zip_with(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqZipWithStep",
        vec![args[0].clone(), args[1].clone(), args[2].clone()],
    ))
}

pub(in crate::native) fn zip_with_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, xs, ys) = (args[0].clone(), args[1].clone(), args[2].clone());
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqZipWithStep"),
            [f.clone(), Term::Var("$rx".into()), Term::Var("$ry".into())],
        ),
    );
    let pull_y = match_step(
        ys,
        none(),
        "$y",
        "$ry",
        some_pair(
            Term::apps(f, [Term::Var("$x".into()), Term::Var("$y".into())]),
            next_seq,
        ),
    );
    Ok(match_step(xs, none(), "$x", "$rx", pull_y))
}

pub(in crate::native) fn enumerate(args: &[Term], _: &mut String) -> Result<Term, String> {
    Ok(step_thunk(
        "seqEnumerateStep",
        vec![int(0), args[0].clone()],
    ))
}

pub(in crate::native) fn enumerate_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let idx = int_val(&args[0])?;
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqEnumerateStep"),
            [int(idx.wrapping_add(1)), Term::Var("$rest".into())],
        ),
    );
    let pair = Term::Tuple(Rc::from([int(idx), Term::Var("$x".into())]));
    Ok(match_step(
        args[1].clone(),
        none(),
        "$x",
        "$rest",
        some_pair(pair, next_seq),
    ))
}

pub(in crate::native) fn scan(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, init, xs) = (args[0].clone(), args[1].clone(), args[2].clone());
    let rest = step_thunk("seqScanStep", vec![f, init.clone(), xs]);
    Ok(seq_thunk(Term::lam("_", some_pair(init, rest))))
}

pub(in crate::native) fn scan_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let (f, acc, xs) = (args[0].clone(), args[1].clone(), args[2].clone());
    let next_seq = Term::app(
        Term::konst(Const::SeqThunk),
        Term::apps(
            model("seqScanStep"),
            [
                f.clone(),
                Term::Var("$acc".into()),
                Term::Var("$rest".into()),
            ],
        ),
    );
    let body = Term::Let(
        "$acc".into(),
        None,
        Rc::new(Term::apps(f, [acc, Term::Var("$x".into())])),
        Rc::new(some_pair(Term::Var("$acc".into()), next_seq)),
    );
    Ok(match_step(xs, none(), "$x", "$rest", body))
}

pub(in crate::native) fn chunks(args: &[Term], _: &mut String) -> Result<Term, String> {
    let n = int_val(&args[0])?;
    if n <= 0 {
        Ok(empty_seq())
    } else {
        Ok(step_thunk(
            "seqChunksStep",
            vec![args[0].clone(), Term::List(Rc::from([])), args[1].clone()],
        ))
    }
}

pub(in crate::native) fn chunks_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let n = int_val(&args[0])?;
    let Ok(size) = usize::try_from(n) else {
        return Ok(none());
    };
    if size == 0 {
        return Ok(none());
    }
    let items = list(&args[1])?;
    let on_none = if items.is_empty() {
        none()
    } else {
        some_pair(args[1].clone(), empty_seq())
    };
    let grown = Term::List(
        items
            .iter()
            .cloned()
            .chain(iter::once(Term::Var("$x".into())))
            .collect(),
    );
    let on_some = if items.len() + 1 == size {
        let next_seq = Term::app(
            Term::konst(Const::SeqThunk),
            Term::apps(
                model("seqChunksStep"),
                [int(n), Term::List(Rc::from([])), Term::Var("$rest".into())],
            ),
        );
        some_pair(grown, next_seq)
    } else {
        step_call(
            "seqChunksStep",
            vec![int(n), grown, Term::Var("$rest".into())],
        )
    };
    Ok(match_step(args[2].clone(), on_none, "$x", "$rest", on_some))
}

pub(in crate::native) fn windows(args: &[Term], _: &mut String) -> Result<Term, String> {
    let n = int_val(&args[0])?;
    if n <= 0 {
        Ok(empty_seq())
    } else {
        Ok(step_thunk(
            "seqWindowsStep",
            vec![args[0].clone(), Term::List(Rc::from([])), args[1].clone()],
        ))
    }
}

pub(in crate::native) fn windows_step(args: &[Term], _: &mut String) -> Result<Term, String> {
    let n = int_val(&args[0])?;
    let Ok(size) = usize::try_from(n) else {
        return Ok(none());
    };
    if size == 0 {
        return Ok(none());
    }
    let items = list(&args[1])?;
    let grown = Term::List(
        items
            .iter()
            .cloned()
            .chain(iter::once(Term::Var("$x".into())))
            .collect(),
    );
    let on_some = if items.len() + 1 == size {
        let next_win = if size == 1 {
            Term::List(Rc::from([]))
        } else {
            Term::List(
                items[1..]
                    .iter()
                    .cloned()
                    .chain(iter::once(Term::Var("$x".into())))
                    .collect(),
            )
        };
        let next_seq = Term::app(
            Term::konst(Const::SeqThunk),
            Term::apps(
                model("seqWindowsStep"),
                [int(n), next_win, Term::Var("$rest".into())],
            ),
        );
        some_pair(grown, next_seq)
    } else {
        step_call(
            "seqWindowsStep",
            vec![int(n), grown, Term::Var("$rest".into())],
        )
    };
    Ok(match_step(args[2].clone(), none(), "$x", "$rest", on_some))
}
