//! Hand-written cases of regions by use: holding without use is accepted
//! and runs; using after close stays rejected, also through a
//! callback row, a generic helper or a local signature.

mod live_support;

use std::rc::Rc;

use flick_model::eval::Outcome;
use flick_model::generate::Program;
use flick_model::syntax::{Const, Term};
use flick_model::types::{Kind, Row, Scheme, Stage, Type, Var};

use live_support::{core, g, open_in, probe, run, thunk_program, v, verdict};

/// A closure that holds a capability and never uses it leaves its `scope`:
/// the model accepts it, and the machine runs it without a use after close.
#[test]
fn holding_without_use_is_accepted_and_safe() {
    let keep = Term::lam(
        "x",
        Term::Let("_".into(), None, Rc::new(v("h")), Rc::new(Term::unit())),
    );
    let leaked = open_in("s", keep);
    let e = Term::Let(
        "k".into(),
        None,
        Rc::new(leaked),
        Rc::new(Term::app(v("k"), Term::unit())),
    );
    let (live, outcome) = probe("hold", &thunk_program(e));
    assert!(live.is_ok());
    assert!(!matches!(outcome, Outcome::Stuck(_)));
}

/// The same holds for a delayed task, not only an arrow.
#[test]
fn a_task_that_only_holds_a_handle_can_run_after_close() {
    let held = Term::Task(Rc::new(Term::Let(
        "_".into(),
        None,
        Rc::new(v("h")),
        Rc::new(Term::unit()),
    )));
    let p = thunk_program(Term::Let(
        "job".into(),
        None,
        Rc::new(open_in("s", held)),
        Rc::new(Term::Await(Rc::new(v("job")))),
    ));
    assert!(verdict(&p).is_ok());
    assert!(matches!(run(&p), Outcome::Value(_)));
}

/// Passing a callback to an API that promises an empty row cannot erase
/// Live, even when the callback is passed while its region is still open.
#[test]
fn a_closed_callback_row_cannot_hide_live() {
    let unit = Type::con("Prelude", "Unit");
    let empty = Stage::Runtime(Row::none(), Row::none());
    let sig = Scheme::new(
        Vec::new(),
        Type::fun(
            Type::fun(unit.clone(), empty.clone(), unit.clone()),
            Stage::Pure,
            Type::fun(unit.clone(), empty, unit),
        ),
    );
    let e = Term::Let(
        "run".into(),
        Some(Rc::new(sig)),
        Rc::new(Term::lam("f", Term::lam("x", Term::app(v("f"), v("x"))))),
        Rc::new(open_in(
            "s",
            Term::apps(
                v("run"),
                [Term::lam("x", Term::app(g("use"), v("h"))), Term::unit()],
            ),
        )),
    );
    let result = verdict(&thunk_program(e));
    assert!(
        matches!(&result, Err(e) if e.starts_with("(Live)")),
        "{result:?}"
    );
}

/// A generic helper applies `use` to a capability later: the closure it
/// returns needs `Live s` through its row variable, so it cannot leave the
/// `scope`, and run, it would use the capability after close.
#[test]
fn a_generic_helper_keeps_live_in_its_row() {
    let (arg, row) = (
        Type::Var(Var::Named("a".into())),
        Row::var(Var::Named("r".into())),
    );
    let unit = Type::con("Prelude", "Unit");
    let call = Stage::Runtime(row.clone(), Row::none());
    let sig = Scheme::new(
        vec![("a".into(), Kind::Type), ("r".into(), Kind::Services)],
        Type::fun(
            Type::fun(arg.clone(), call.clone(), unit.clone()),
            Stage::Pure,
            Type::fun(arg, Stage::Pure, Type::fun(unit.clone(), call, unit)),
        ),
    );
    let body = Term::lam(
        "f",
        Term::lam("x", Term::lam("u", Term::app(v("f"), v("x")))),
    );
    let leaked = open_in("s", Term::apps(v("later"), [g("use"), v("h")]));
    let term = Term::Let(
        "later".into(),
        Some(Rc::new(sig)),
        Rc::new(body),
        Rc::new(Term::Let(
            "k".into(),
            None,
            Rc::new(leaked),
            Rc::new(Term::app(v("k"), Term::unit())),
        )),
    );
    let p = thunk_program(term);
    let (live, outcome) = probe("later", &p);
    assert!(live.is_err());
    assert!(core(&p).iter().any(|e| e.starts_with("FLICK-G002")));
    assert!(matches!(outcome, Outcome::Stuck(w) if w.starts_with("use after")));
}

/// A use of a local signed function sees the qualifiers its later stages
/// get from the parameters before them, as its body does (`regions.md`
/// §1.4): `later use h` holds `h`, so it cannot leave the `scope`. The
/// same for `let rec`.
#[test]
fn a_local_signature_keeps_its_stage_qualifiers() {
    let (arg, row) = (
        Type::Var(Var::Named("a".into())),
        Row::var(Var::Named("r".into())),
    );
    let unit = Type::con("Prelude", "Unit");
    let call = Stage::Runtime(row.clone(), Row::none());
    let sig = Scheme::new(
        vec![("a".into(), Kind::Type), ("r".into(), Kind::Services)],
        Type::fun(
            Type::fun(arg.clone(), call.clone(), unit.clone()),
            Stage::Pure,
            Type::fun(arg, Stage::Pure, Type::fun(unit.clone(), call, unit)),
        ),
    );
    let body = || {
        Term::lam(
            "f",
            Term::lam("x", Term::lam("u", Term::app(v("f"), v("x")))),
        )
    };
    let rest = |inside: bool| {
        let partial = Term::apps(v("later"), [g("use"), v("h")]);
        if inside {
            open_in("s", Term::app(partial, Term::unit()))
        } else {
            Term::Let(
                "k".into(),
                None,
                Rc::new(open_in("s", partial)),
                Rc::new(Term::app(v("k"), Term::unit())),
            )
        }
    };
    for recursive in [false, true] {
        for inside in [false, true] {
            let sig = Some(Rc::new(sig.clone()));
            let term = if recursive {
                Term::LetRec("later".into(), sig, Rc::new(body()), Rc::new(rest(inside)))
            } else {
                Term::Let("later".into(), sig, Rc::new(body()), Rc::new(rest(inside)))
            };
            let p = thunk_program(term);
            let live = verdict(&p);
            let flick_core = core(&p);
            if inside {
                assert!(live.is_ok(), "rec {recursive}: {live:?}");
                assert!(flick_core.is_empty(), "rec {recursive}: {flick_core:?}");
                assert!(!matches!(run(&p), Outcome::Stuck(_)));
            } else {
                assert!(live.is_err(), "rec {recursive}: accepted a use after close");
                assert!(flick_core.iter().any(|e| e.starts_with("FLICK-G002")));
                assert!(matches!(run(&p), Outcome::Stuck(w) if w.starts_with("use after")));
            }
        }
    }
}

/// The same helper used inside the `scope` is accepted.
#[test]
fn a_generic_helper_inside_its_scope_is_accepted() {
    let (a, r) = (
        Type::Var(Var::Named("a".into())),
        Row::var(Var::Named("r".into())),
    );
    let unit = Type::con("Prelude", "Unit");
    let call = Stage::Runtime(r.clone(), Row::none());
    let sig = Scheme::new(
        vec![("a".into(), Kind::Type), ("r".into(), Kind::Services)],
        Type::fun(
            Type::fun(a.clone(), call.clone(), unit.clone()),
            Stage::Pure,
            Type::fun(a, Stage::Pure, Type::fun(unit.clone(), call, unit)),
        ),
    );
    let body = Term::lam(
        "f",
        Term::lam("x", Term::lam("u", Term::app(v("f"), v("x")))),
    );
    let inside = open_in(
        "s",
        Term::app(Term::apps(v("later"), [g("use"), v("h")]), Term::unit()),
    );
    let e = Term::Let(
        "later".into(),
        Some(Rc::new(sig)),
        Rc::new(body),
        Rc::new(inside),
    );
    let (live, outcome) = probe("later inside", &thunk_program(e));
    assert!(live.is_ok());
    assert!(!matches!(outcome, Outcome::Stuck(_)));
}

/// Moving an opaque `a` through a cell does not use a region of `a`.
/// Its capture row is solved only later, so the Live obligation must
/// distinguish type atoms from region atoms after solving as well.
#[test]
fn generic_ref_round_trip_does_not_use_its_contents() {
    let a = Type::Var(Var::Named("a".into()));
    let p = Program {
        scheme: Scheme::new(
            vec![("a".into(), Kind::Type)],
            Type::fun(a.clone(), Stage::Runtime(Row::none(), Row::none()), a),
        ),
        term: Term::lam(
            "x",
            Term::Let(
                "cell".into(),
                None,
                Rc::new(Term::app(Term::konst(Const::RefMake), v("x"))),
                Rc::new(Term::Let(
                    "_".into(),
                    None,
                    Rc::new(Term::apps(Term::konst(Const::RefSet), [v("cell"), v("x")])),
                    Rc::new(Term::app(Term::konst(Const::RefGet), v("cell"))),
                )),
            ),
        ),
    };
    assert!(verdict(&p).is_ok(), "{:?}", verdict(&p));
}
