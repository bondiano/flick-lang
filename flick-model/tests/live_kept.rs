//! Work an owner keeps and the qualifier of what it runs (rule 4 of
//! `formal/Formal/Live.lean`): the live core asks the qualifier of a child
//! or a finalizer to outlive its owner and to be part of the work's row.

mod live_support;

use std::rc::Rc;

use flick_model::eval::Outcome;
use flick_model::syntax::{Name, Path, Term};
use flick_model::types::{Caps, Row, Scheme, Type, Var};

use live_support::{core, g, probe, thunk_program, v};

fn unit() -> Type {
    Type::con("Prelude", "Unit")
}

fn scope_row(o: &str) -> Row {
    let mut r = Row::none();
    r.elems.insert(
        Path::new("Prelude", "Scope"),
        Rc::from([Type::Var(Var::Named(o.into()))]),
    );
    r
}

/// `(Eff {Scope o} none Unit)^{q}`.
fn work(o: &str, q: &str) -> Type {
    Type::Captured(
        Rc::new(Type::Eff(scope_row(o), Row::none(), Rc::new(unit()))),
        Caps::atom(Var::Named(q.into())),
    )
}

fn scope(s: &str, body: Term) -> Term {
    Term::Scope(s.into(), Rc::new(body))
}

fn let_sig(x: &str, t: Type, e1: Term, e2: Term) -> Term {
    Term::Let(
        x.into(),
        Some(Rc::new(Scheme::new(Vec::new(), t))),
        Rc::new(e1),
        Rc::new(e2),
    )
}

fn spawn(o: &str, t: Term) -> Term {
    let call = Term::apps(
        Term::Global(Path::new("Fiber", "spawn")),
        [v(o), g("report"), t],
    );
    Term::Let("_".into(), None, Rc::new(call), Rc::new(Term::unit()))
}

fn hold(h: &str) -> Term {
    Term::Let(Name::from("_"), None, Rc::new(v(h)), Rc::new(Term::unit()))
}

/// A child whose qualifier names a region older than its owner: the live
/// core accepts it, its `spawn` taking that region into the child's row
/// (`Const.ok` of `.spawn`, rule 4), as the elided signature of
/// `Fiber.spawn` takes it into the row of the call.
#[test]
fn a_child_may_be_qualified_by_an_older_region() {
    let a = scope(
        "p",
        scope(
            "o",
            let_sig(
                "t",
                work("o", "p"),
                Term::Task(Rc::new(Term::unit())),
                spawn("o", v("t")),
            ),
        ),
    );
    let (captures, live, outcome) = probe("outer qualifier", &thunk_program(a));
    assert!(captures.is_ok(), "{captures:?}");
    assert!(live.is_ok(), "{live:?}");
    assert!(matches!(outcome, Outcome::Value(_)));
}

/// A child qualified by a region younger than its owner is rejected, whether
/// or not it holds a capability of it: the live core asks the qualifier to
/// outlive the owner (`Outl Θ ι.C ι.s` of `Const.ok`), not only the regions
/// the child uses.
#[test]
fn a_child_qualified_by_a_younger_region_is_rejected() {
    let open = Term::app(g("open_scoped"), Term::string("x"));
    let held = scope(
        "o",
        scope(
            "s",
            Term::Let(
                "h".into(),
                None,
                Rc::new(open),
                Rc::new(let_sig(
                    "t",
                    work("o", "s"),
                    Term::Task(Rc::new(hold("h"))),
                    spawn("o", v("t")),
                )),
            ),
        ),
    );
    let bare = scope(
        "o",
        scope(
            "s",
            let_sig(
                "t",
                work("o", "s"),
                Term::Task(Rc::new(Term::unit())),
                spawn("o", v("t")),
            ),
        ),
    );
    for (name, e) in [("inner held", held), ("inner qualifier", bare)] {
        let p = thunk_program(e);
        let (_, live, _) = probe(name, &p);
        assert!(
            matches!(&live, Err(e) if e.starts_with("(Outlives)") && e.contains("FLICK-G003")),
            "{name}: {live:?}"
        );
        assert!(
            core(&p).iter().any(|e| e.starts_with("FLICK-G003")),
            "{name}"
        );
    }
}
