//! Unit tests of the rules: one accepted and one rejected program each.

mod disagreements;
mod runtime;

use std::fmt::Debug;
use std::rc::Rc;
use std::slice;

use super::trace::key;
use super::{Checker, TypeError};
use crate::decls::Decls;
use crate::syntax::{Arm, Con, Const, Lit, Pat, Path, Prim, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

fn ty(name: &str) -> Type {
    Type::con("Prelude", name)
}

fn int(n: i32) -> Term {
    Term::Lit(Lit::Int(n))
}

fn var(x: &str) -> Term {
    Term::Var(x.into())
}

fn let_(x: &str, e1: Term, e2: Term) -> Term {
    Term::Let(x.into(), None, Rc::new(e1), Rc::new(e2))
}

fn let_sig(x: &str, s: Scheme, e1: Term, e2: Term) -> Term {
    Term::Let(x.into(), Some(Rc::new(s)), Rc::new(e1), Rc::new(e2))
}

fn mono(body: Type) -> Scheme {
    Scheme {
        vars: Vec::new(),
        body,
        outlives: Vec::new(),
    }
}

/// `∀a. body`.
fn poly(body: Type) -> Scheme {
    Scheme {
        vars: vec![("a".into(), Kind::Type)],
        body,
        outlives: Vec::new(),
    }
}

fn a() -> Type {
    Type::Var(Var::Named("a".into()))
}

fn rt() -> Stage {
    Stage::Runtime(Row::none(), Row::none())
}

fn task(e: Term) -> Term {
    Term::Task(Rc::new(e))
}

fn arm(pat: Pat, body: Term) -> Arm {
    Arm {
        pat,
        guard: None,
        body,
    }
}

fn match_(e: Term, arms: Vec<Arm>) -> Term {
    Term::Match(Rc::new(e), arms.into())
}

fn tuple(xs: Vec<Term>) -> Term {
    Term::Tuple(xs.into())
}

fn record(fields: &[(&str, Term)]) -> Term {
    Term::Record(
        fields
            .iter()
            .map(|(l, x)| ((*l).into(), x.clone()))
            .collect(),
    )
}

fn konst(c: Const, args: Vec<Term>) -> Term {
    Term::apps(Term::konst(c), args)
}

/// `Ref.get (Ref.make (λx. x))`: a runtime step whose result is polymorphic
/// if anything generalizes it.
fn cell_id() -> Term {
    konst(
        Const::RefGet,
        vec![konst(Const::RefMake, vec![Term::lam("x", var("x"))])],
    )
}

const T: &str = "T";

fn con(name: &str, ty: &str, arity: usize) -> Rc<Con> {
    Rc::new(Con {
        ty: Path::new(T, ty),
        name: name.into(),
        arity,
        dicts: 0,
    })
}

/// `A 1.0`, of `type error A = A Float`.
fn a_err() -> Term {
    Term::app(
        Term::konst_con(con("A", "A", 1)),
        Term::Lit(Lit::Float(1.0)),
    )
}

fn b_err() -> Term {
    Term::app(Term::konst_con(con("B", "B", 1)), Term::string("x"))
}

fn raise(e: Term) -> Term {
    konst(Const::Raise, vec![e])
}

fn log() -> Path {
    Path::new(T, "Log")
}

/// Errors `A Float` and `B String`, service `Log` with
/// `log : String ->{} Unit`, and `type Job = Loud (Unit ->{} String) |
/// Quiet (Unit -> String)`.
fn decls() -> Decls {
    let mut d = Decls::default();
    let ctor = |p: Type, r: &str| mono(Type::fun(p, Stage::Pure, Type::con(T, r)));
    for (e, payload) in [("A", "Float"), ("B", "String")] {
        let p = Path::new(T, e);
        d.constructors.insert(p.clone(), ctor(ty(payload), e));
        d.variants.insert(p.clone(), vec![(e.into(), 1)]);
        d.errors.insert(p);
    }
    let unit_to = |s: Stage| Type::fun(ty("Unit"), s, ty("String"));
    d.constructors
        .insert(Path::new(T, "Loud"), ctor(unit_to(rt()), "Job"));
    d.constructors
        .insert(Path::new(T, "Quiet"), ctor(unit_to(Stage::Pure), "Job"));
    d.variants.insert(
        Path::new(T, "Job"),
        vec![("Loud".into(), 1), ("Quiet".into(), 1)],
    );
    d.services.insert(
        log(),
        vec![(
            "log".into(),
            mono(Type::fun(ty("String"), rt(), ty("Unit"))),
        )],
    );
    d
}

fn infer(e: &Term) -> Result<(Type, Stage), TypeError> {
    Checker::new(&decls()).infer_top(e)
}

fn check(e: &Term, s: &Scheme) -> Result<(), TypeError> {
    Checker::new(&decls()).check_top(e, s)
}

#[track_caller]
fn rejected_by(result: Result<impl Debug, TypeError>, rule: &str) {
    match result {
        Err(e) => assert_eq!(e.rule, rule, "{e}"),
        Ok(t) => panic!("accepted at {t:?}, expected ({rule}) to reject it"),
    }
}

/// An effect that is `Eff none none τ`.
fn eff(t: Type) -> Type {
    Type::Eff(Row::none(), Row::none(), Rc::new(t))
}
include!("tests/expressions.rs");
include!("tests/effects_patterns.rs");
include!("tests/bounds_provenance.rs");
