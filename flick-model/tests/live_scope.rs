//! `Live C` and `Scope s` are the same row atoms (`formal/Formal/Live/Mode.lean`,
//! `TypingMode.use`; `Row.scope s = Row.ofCaps s.reach`): a row that has
//! `Scope s` keeps `s` live, so a helper whose row has `Scope s` may use
//! what it acquires into `s`. `Live s` alone gives no `Scope s`.

mod live_support;

use std::rc::Rc;

use flick_model::eval::Outcome;
use flick_model::generate::Program;
use flick_model::syntax::{Path, Term};
use flick_model::types::{Kind, Row, Scheme, Stage, Type, Var};

use live_support::{core, g, open_in, probe, thunk_program, v, verdict};

fn unit() -> Type {
    Type::con("Prelude", "Unit")
}

fn handle(s: &str) -> Type {
    Type::app(Type::con("Gen", "Handle"), Type::Var(Var::Named(s.into())))
}

fn acquire() -> Term {
    Term::app(g("open_scoped"), Term::string("y"))
}

/// `helper : forall s. Unit ->{Scope s} Unit`, which acquires into `s`
/// and uses what it acquired, called in a `scope`.
fn acquiring_helper() -> Program {
    let mut row = Row::none();
    row.elems.insert(
        Path::new("Prelude", "Scope"),
        Rc::from([Type::Var(Var::Named("s".into()))]),
    );
    let sig = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(unit(), Stage::Runtime(row, Row::none()), unit()),
    );
    let body = Term::lam("u", Term::app(g("use"), acquire()));
    let call = open_in("t", Term::app(v("helper"), Term::unit()));
    thunk_program(Term::Let(
        "helper".into(),
        Some(Rc::new(sig)),
        Rc::new(body),
        Rc::new(call),
    ))
}

/// The helper's row has `Scope s` and no `Live s`: its body uses `s`
/// within it, both checkers accept it, and it runs.
#[test]
fn a_helper_with_scope_keeps_its_region_live() {
    let p = acquiring_helper();
    let (live, outcome) = probe("scope helper", &p);
    assert!(live.is_ok(), "{live:?}");
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let errors = core(&p);
    assert!(errors.is_empty(), "{errors:?}");
}

/// `helper : forall s. Handle s ->{} Handle s` acquires into `s`: its row
/// has `Live s` from its parameter, which gives no `Scope s`.
#[test]
fn live_alone_does_not_acquire() {
    let sig = Scheme::new(
        vec![("s".into(), Kind::Region)],
        Type::fun(
            handle("s"),
            Stage::Runtime(Row::none(), Row::none()),
            handle("s"),
        ),
    );
    let body = Term::lam("h", acquire());
    let call = open_in("t", Term::app(g("use"), Term::app(v("helper"), v("h"))));
    let p = thunk_program(Term::Let(
        "helper".into(),
        Some(Rc::new(sig)),
        Rc::new(body),
        Rc::new(call),
    ));
    let live = verdict(&p);
    assert!(live.is_err(), "`Live s` alone acquires");
    let errors = core(&p);
    assert!(
        errors.iter().any(|e| e.starts_with("FLICK-G004")),
        "{errors:?}"
    );
}
