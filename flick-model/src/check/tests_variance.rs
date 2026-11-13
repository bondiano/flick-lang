//! `regions.md` §2.2.1: the sign of each parameter of a declared type, read
//! from its constructors, and subtyping of data by it, of records and of
//! tuples field by field.

use std::collections::BTreeMap;
use std::rc::Rc;

use super::Checker;
use crate::decls::Decls;
use crate::syntax::{Path, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};
use crate::variance::Sign;

const T: &str = "T";

fn ty(name: &str) -> Type {
    Type::con("Prelude", name)
}

fn a() -> Type {
    Type::Var(Var::Named("a".into()))
}

fn data(name: &str, arg: Type) -> Type {
    Type::app(Type::con(T, name), arg)
}

fn unit_fn(s: Stage) -> Type {
    Type::fun(ty("Unit"), s, ty("Unit"))
}

fn rt() -> Stage {
    Stage::Runtime(Row::none(), Row::none())
}

fn record(t: Type) -> Type {
    Type::Record(Fields {
        fields: BTreeMap::from([("f".into(), t)]),
        tail: None,
    })
}

/// A constructor `K : ∀a. fields → res` of the type `res` names.
fn ctor(d: &mut Decls, ty: &str, k: &str, fields: Vec<Type>, res: Type) {
    let body = (fields.into_iter().rev()).fold(res, |r, f| Type::fun(f, Stage::Pure, r));
    let vars = vec![("a".into(), Kind::Type)];
    d.constructors
        .insert(Path::new(T, k), Scheme::new(vars, body));
    let p = Path::new(T, ty);
    d.params.insert(p.clone(), vec![Kind::Type]);
    d.variants.entry(p).or_default().push((k.into(), 0));
}

fn decls() -> Decls {
    let mut d = Decls::default();
    ctor(&mut d, "List", "Nil", vec![], data("List", a()));
    ctor(
        &mut d,
        "List",
        "Cons",
        vec![a(), data("List", a())],
        data("List", a()),
    );
    ctor(&mut d, "Box", "Box", vec![record(a())], data("Box", a()));
    ctor(
        &mut d,
        "Pair",
        "Pair",
        vec![Type::Tuple(Rc::from([a(), ty("Int")]))],
        data("Pair", a()),
    );
    ctor(
        &mut d,
        "Sink",
        "Sink",
        vec![Type::fun(a(), Stage::Pure, ty("Unit"))],
        data("Sink", a()),
    );
    let twice = Type::fun(
        Type::fun(a(), Stage::Pure, ty("Unit")),
        Stage::Pure,
        ty("Unit"),
    );
    ctor(&mut d, "Twice", "Twice", vec![twice], data("Twice", a()));
    ctor(
        &mut d,
        "Cell",
        "Cell",
        vec![Type::Ref(Rc::new(a()))],
        data("Cell", a()),
    );
    ctor(
        &mut d,
        "Tag",
        "Pure",
        vec![],
        data("Tag", unit_fn(Stage::Pure)),
    );
    ctor(
        &mut d,
        "Expr",
        "IntE",
        vec![ty("Int")],
        data("Expr", ty("Int")),
    );
    ctor(&mut d, "Expr", "Wrap", vec![a()], data("Expr", a()));
    ctor(&mut d, "Phantom", "Phantom", vec![], data("Phantom", a()));
    ctor(
        &mut d,
        "Over",
        "Over",
        vec![data("Phantom", a())],
        data("Over", a()),
    );
    // an enum, closed, and data that holds a function, which is not
    for (t, k, fields) in [
        ("Color", "Red", vec![]),
        ("Job", "Job", vec![unit_fn(Stage::Pure)]),
    ] {
        let body = (fields.into_iter()).fold(Type::con(T, t), |r, f| Type::fun(f, Stage::Pure, r));
        d.constructors
            .insert(Path::new(T, k), Scheme::new(Vec::new(), body));
        d.variants.insert(Path::new(T, t), vec![(k.into(), 0)]);
    }
    ctor(
        &mut d,
        "ByColor",
        "Paint",
        vec![],
        data("ByColor", Type::con(T, "Color")),
    );
    ctor(&mut d, "ByColor", "Plain", vec![a()], data("ByColor", a()));
    ctor(
        &mut d,
        "ByJob",
        "Run",
        vec![],
        data("ByJob", Type::con(T, "Job")),
    );
    d.finish();
    d
}

#[test]
fn a_parameter_has_the_sign_its_fields_allow() {
    let d = decls();
    let sign = |t: &str| d.signs[&Path::new(T, t)][0];
    for (t, s) in [
        ("List", Sign::Pos),
        ("Box", Sign::Pos),
        ("Pair", Sign::Pos),
        ("Sink", Sign::Neg),
        // a covariant position under two flips is no covariant one
        ("Twice", Sign::Inv),
        ("Cell", Sign::Inv),
        // a constructor fixes it to an arrow, which subtyping moves
        ("Tag", Sign::Inv),
        // to `Int`, which subtyping does not move (A2)
        ("Expr", Sign::Pos),
        ("ByColor", Sign::Pos),
        ("ByJob", Sign::Inv),
        ("Phantom", Sign::Bi),
        ("Over", Sign::Pos),
    ] {
        assert_eq!(sign(t), s, "{t}");
    }
}

/// `fun x -> x` checked at `before -> after`.
fn identity(before: Type, after: Type) -> Result<(), super::TypeError> {
    let s = Scheme::new(Vec::new(), Type::fun(before, Stage::Pure, after));
    Checker::new(&decls()).check_top(&Term::lam("x", Term::Var("x".into())), &s)
}

#[test]
fn covariant_data_records_and_tuples_widen() {
    let (pure, runs) = (unit_fn(Stage::Pure), unit_fn(rt()));
    let wraps: [fn(Type) -> Type; 5] = [
        |t| data("List", t),
        |t| data("Box", t),
        |t| data("Over", t),
        record,
        |t| Type::Tuple(Rc::from([t, ty("Int")])),
    ];
    for wrap in wraps {
        let (p, r) = (wrap(pure.clone()), wrap(runs.clone()));
        assert!(identity(p.clone(), r.clone()).is_ok(), "{p} ≤ {r}");
        assert!(identity(r.clone(), p.clone()).is_err(), "{r} ≤ {p}");
    }
}

#[test]
fn a_contravariant_argument_narrows() {
    let (pure, runs) = (unit_fn(Stage::Pure), unit_fn(rt()));
    assert!(identity(data("Sink", runs.clone()), data("Sink", pure.clone())).is_ok());
    assert!(identity(data("Sink", pure), data("Sink", runs)).is_err());
}

#[test]
fn an_invariant_argument_stays() {
    let (pure, runs) = (unit_fn(Stage::Pure), unit_fn(rt()));
    for t in ["Cell", "Tag", "Twice"] {
        assert!(
            identity(data(t, pure.clone()), data(t, runs.clone())).is_err(),
            "{t}"
        );
        assert!(
            identity(data(t, runs.clone()), data(t, pure.clone())).is_err(),
            "{t}"
        );
    }
}

#[test]
fn a_hidden_row_keeps_the_capture_rows_shown_by_data_arguments() {
    let captures = |name: &str| Caps::var(Var::Named(name.into()));
    let before = Type::Captured(Rc::new(unit_fn(Stage::Pure)), captures("c"));
    let after = Type::Captured(
        Rc::new(unit_fn(Stage::Pure)),
        captures("c").union(&captures("d")),
    );
    let scheme = Scheme::new(
        vec![("c".into(), Kind::Captures), ("d".into(), Kind::Captures)],
        Type::fun(data("Box", before), Stage::Pure, data("Box", after)),
    );
    let result = Checker::new(&decls()).check_top(&Term::lam("x", Term::Var("x".into())), &scheme);
    assert!(result.is_ok(), "{result:?}");
}
