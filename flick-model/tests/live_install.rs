//! A service a region indexes keeps the region live in a row (`Within s`
//! of `Gen.fl`, as `Setup s` of a component), so a handler of it is
//! installed only where its region is live: otherwise a handler installed
//! after its `scope` closes runs a body that takes the region for live.

mod live_support;

use std::rc::Rc;

use flick_model::eval::Outcome;
use flick_model::handler::{Handled, Handler, Op, OpClass};
use flick_model::syntax::{Path, Term};

use live_support::{g, open_in, run, thunk_program, v, verdict};

/// `handle Within { get () -> h } { touch () }`: `touch` uses the handle
/// its handler gives through `Within s`, not through a parameter.
fn handled_touch() -> Term {
    let get = Op {
        name: "get".into(),
        class: OpClass::Tail,
        dicts: 0,
        arity: 1,
        covers: true,
        function: Term::lam("_", v("h")),
    };
    Term::Handler(Rc::new(Handler {
        effects: vec![Handled {
            effect: Path::new("Fixture", "Within"),
            ops: vec![get],
        }],
        body: Term::app(g("touch"), Term::unit()),
        ret: None,
        errors: Rc::from([]),
    }))
}

#[test]
fn a_handler_installed_while_its_region_is_live_runs() {
    let p = thunk_program(open_in("s", handled_touch()));
    assert!(verdict(&p).is_ok(), "{:?}", verdict(&p));
    assert!(matches!(run(&p), Outcome::Value(_)), "{:?}", run(&p));
}

/// The closure leaves the `scope` holding `h`, which holding alone allows;
/// it installs the handler after `s` closes, and its body uses `h` there.
#[test]
fn a_handler_installed_after_its_region_closes_is_rejected() {
    let leaked = open_in("s", Term::lam("_", handled_touch()));
    let p = thunk_program(Term::Let(
        "k".into(),
        None,
        Rc::new(leaked),
        Rc::new(Term::app(v("k"), Term::unit())),
    ));
    assert!(verdict(&p).is_err(), "accepted a use after close");
    // what the rule keeps out: the machine reaches the use of a closed region
    assert!(matches!(run(&p), Outcome::Stuck(_)), "{:?}", run(&p));
}
