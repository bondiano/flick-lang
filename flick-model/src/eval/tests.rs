//! Unit tests of the steps of §3.3–3.5, each on a term whose outcome the
//! rules fix.

use std::rc::Rc;

use flick_core::numeric::{Arith, Num};

use super::{Machine, Outcome, Program};
use crate::syntax::{Arm, Con, Const, Lit, Pat, Path, Prim, Term};

fn int(n: i32) -> Term {
    Term::Lit(Lit::Int(n))
}

fn var(x: &str) -> Term {
    Term::Var(x.into())
}

fn let_(x: &str, e1: Term, e2: Term) -> Term {
    Term::Let(x.into(), None, Rc::new(e1), Rc::new(e2))
}

fn konst(c: Const, args: Vec<Term>) -> Term {
    Term::apps(Term::konst(c), args)
}

fn arith(op: Arith, a: Term, b: Term) -> Term {
    Term::apps(Term::prim(Prim::Arith(op, Num::Int)), [a, b])
}

fn arm(pat: Pat, guard: Option<Term>, body: Term) -> Arm {
    Arm { pat, guard, body }
}

/// `await (task e)`: `e` run on the runtime stage.
fn run_task(e: Term) -> Term {
    Term::Await(Rc::new(Term::Task(Rc::new(e))))
}

fn run(e: Term) -> Outcome {
    Machine::new(&Program::default(), 100_000).run(e)
}

#[track_caller]
fn value(e: Term) -> Lit {
    match run(e) {
        Outcome::Value(Term::Lit(l)) => l,
        other => panic!("not a literal: {other:?}"),
    }
}

#[test]
fn beta() {
    assert_eq!(
        value(Term::app(
            Term::lam("x", arith(Arith::Add, var("x"), int(1))),
            int(1)
        )),
        Lit::Int(2)
    );
}

/// `let rec` unfolds through `fix` until the base case.
#[test]
fn fix_unfolds() {
    let body = Term::Match(
        Rc::new(var("n")),
        Rc::from([
            arm(Pat::Lit(Lit::Int(0)), None, Term::string("done")),
            arm(
                Pat::Wild,
                None,
                Term::app(var("f"), arith(Arith::Sub, var("n"), int(1))),
            ),
        ]),
    );
    let e = Term::LetRec(
        "f".into(),
        None,
        Rc::new(Term::lam("n", body)),
        Rc::new(Term::app(var("f"), int(3))),
    );
    assert_eq!(value(e), Lit::String("done".into()));
}

/// An arm whose guard is false falls through to the next.
#[test]
fn guard_falls_through() {
    let arms = [
        arm(Pat::Var("x".into()), Some(Term::bool(false)), int(1)),
        arm(Pat::Wild, None, int(2)),
    ];
    assert_eq!(
        value(Term::Match(Rc::new(int(0)), arms.into())),
        Lit::Int(2)
    );
}

#[test]
fn ref_cell() {
    let set = konst(Const::RefSet, vec![var("r"), int(2)]);
    let e = let_(
        "r",
        konst(Const::RefMake, vec![int(1)]),
        let_("_", set, konst(Const::RefGet, vec![var("r")])),
    );
    assert_eq!(value(run_task(e)), Lit::Int(2));
}

/// A task is a description: each `await` runs its body again.
#[test]
fn task_runs_at_each_await() {
    let bump = konst(
        Const::RefSet,
        vec![
            var("c"),
            arith(Arith::Add, konst(Const::RefGet, vec![var("c")]), int(1)),
        ],
    );
    let twice = let_(
        "t",
        Term::Task(Rc::new(bump)),
        let_(
            "_",
            Term::Await(Rc::new(var("t"))),
            let_(
                "_",
                Term::Await(Rc::new(var("t"))),
                konst(Const::RefGet, vec![var("c")]),
            ),
        ),
    );
    assert_eq!(
        value(run_task(let_(
            "c",
            konst(Const::RefMake, vec![int(0)]),
            twice
        ))),
        Lit::Int(2)
    );
}

/// `A` of `type error A = A Int`.
fn a() -> Rc<Con> {
    Rc::new(Con {
        ty: Path::new("T", "A"),
        name: "A".into(),
        arity: 1,
        dicts: 0,
    })
}

/// A raise unwinds to the `try` whose arm matches; one no `try` takes
/// ends the run.
#[test]
fn raise_and_try() {
    let raise = konst(Const::Raise, vec![Term::app(Term::konst_con(a()), int(7))]);
    let arms = [arm(
        Pat::Con(a(), Rc::from([Pat::Var("x".into())])),
        None,
        var("x"),
    )];
    assert_eq!(
        value(run_task(Term::Try(Rc::new(raise.clone()), arms.into()))),
        Lit::Int(7)
    );
    assert!(matches!(run(run_task(raise)), Outcome::Raise(_)));
}

/// The innermost `provide` of a service is the one an operation finds.
#[test]
fn inner_provide_wins() {
    let log = Path::new("T", "Log");
    let layer = |s: &str| {
        let implementation =
            Term::Record(Rc::from([("log".into(), Term::lam("_", Term::string(s)))]));
        konst(Const::LayerSucceed(log.clone()), vec![implementation])
    };
    let op = Term::Field(
        Rc::new(konst(Const::Ask(log.clone()), vec![Term::unit()])),
        "log".into(),
    );
    let call = Term::app(op, Term::unit());
    let inner = Term::Provide(Rc::new(layer("inner")), Rc::new(call));
    let e = Term::Provide(Rc::new(layer("outer")), Rc::new(inner));
    assert_eq!(value(run_task(e)), Lit::String("inner".into()));
}

/// `todo` is a defect, not a value.
#[test]
fn todo_dies() {
    assert!(matches!(run(Term::Die("todo".into())), Outcome::Die(_)));
}
