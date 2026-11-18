//! The checker on statements built by hand: what it accepts, and each way
//! a certificate fails to be one.

use std::rc::Rc;

use flick_ir::Lit;

use super::{
    Cert, Ctor, Data, DataDecl, Equation, Failure, Goal, Literal, Pat, Rule, Statement, Term, Ty,
    prove,
};
use crate::source::{SourceMap, Span};

/// A program without definitions.
struct Nothing;

impl super::Program for Nothing {
    fn item(&self, _: &str, _: &str) -> Option<&flick_ir::Item> {
        None
    }

    fn instance(
        &self,
        _: &str,
        _: &str,
    ) -> Option<(&flick_ir::InstanceInfo, &[(String, flick_ir::Expr)])> {
        None
    }
}

fn span() -> Span {
    Span::new(SourceMap::default().add("t.fl", ""), 0, 0)
}

fn var(x: &str) -> Term {
    Term::Var(x.into())
}

fn app(f: Term, a: Term) -> Term {
    Term::App(Rc::new(f), Rc::new(a))
}

fn lam(x: &str, b: Term) -> Term {
    Term::Lam(x.into(), Rc::new(b))
}

fn ctor(name: &str, arity: usize) -> Rc<Ctor> {
    Rc::new(Ctor {
        module: "Prelude".into(),
        ty: "Option".into(),
        name: name.into(),
        arity,
    })
}

fn some(t: Term) -> Term {
    app(Term::Con(ctor("Some", 1)), t)
}

fn none() -> Term {
    Term::Con(ctor("None", 0))
}

fn option(a: Ty) -> Ty {
    Ty::Data {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![a],
    }
}

fn data() -> Data {
    let mut data = Data::new();
    data.insert(
        ("Prelude".into(), "Option".into()),
        DataDecl {
            params: vec!["a".into()],
            constructors: vec![
                ("None".into(), vec![]),
                ("Some".into(), vec![Ty::Rigid("a".into())]),
            ],
            recursive: false,
        },
    );
    data
}

fn cert(kind: Rule) -> Cert {
    Cert { kind, span: span() }
}

fn refl() -> Cert {
    cert(Rule::Refl)
}

fn rigid(a: &str) -> Ty {
    Ty::Rigid(a.into())
}

fn fun(a: Ty, b: Ty) -> Ty {
    Ty::Fun(Box::new(a), Box::new(b))
}

fn equal(params: Vec<(&str, Ty)>, ty: Ty, left: Term, right: Term) -> Statement {
    Statement {
        name: "p".into(),
        span: span(),
        params: params.into_iter().map(|(x, t)| (x.into(), t)).collect(),
        premises: Vec::new(),
        goal: Goal::Equal(Equation { ty, left, right }),
    }
}

fn code(found: &Result<(), Failure>) -> &'static str {
    match found {
        Ok(()) => "ok",
        Err(Failure::Disproved { .. }) => "T080",
        Err(Failure::Unproved { .. }) => "T081",
        Err(Failure::Refused(_)) => "T082",
        Err(Failure::Invalid { .. }) => "T083",
        Err(Failure::Exhausted { .. }) => "T084",
    }
}

#[test]
fn types_are_the_same_only_when_they_are() {
    let same = |a, b| Statement {
        name: "p".into(),
        span: span(),
        params: Vec::new(),
        premises: Vec::new(),
        goal: Goal::SameType(a, b),
    };
    assert_eq!(
        code(&prove(
            &same(rigid("a"), rigid("a")),
            &refl(),
            &data(),
            &Nothing
        )),
        "ok"
    );
    // two variables may stand for different types: not proved, not false
    assert_eq!(
        code(&prove(
            &same(rigid("a"), rigid("b")),
            &refl(),
            &data(),
            &Nothing
        )),
        "T081"
    );
    assert_eq!(
        code(&prove(
            &same(Ty::Bool, option(Ty::Bool)),
            &refl(),
            &data(),
            &Nothing
        )),
        "T080"
    );
    let symm = cert(Rule::Symm(Box::new(refl())));
    assert_eq!(
        code(&prove(
            &same(rigid("a"), rigid("a")),
            &symm,
            &data(),
            &Nothing
        )),
        "T083"
    );
}

#[test]
fn functions_are_equal_by_their_values() {
    let f = fun(rigid("a"), rigid("b"));
    let eta = equal(
        vec![("f", f.clone())],
        f.clone(),
        lam("x", app(var("f"), var("x"))),
        var("f"),
    );
    // without the rule the two are different terms
    assert_eq!(code(&prove(&eta, &refl(), &data(), &Nothing)), "T081");
    let funext = cert(Rule::Funext("y".into(), Box::new(refl())));
    assert_eq!(code(&prove(&eta, &funext, &data(), &Nothing)), "ok");
    // a function's values at different arguments are not one value
    let pred = fun(Ty::Bool, Ty::Bool);
    let t = |b| Term::Lit(Literal(Lit::Bool(b)));
    let constant = equal(
        vec![("f", pred)],
        Ty::Bool,
        app(var("f"), t(true)),
        app(var("f"), t(false)),
    );
    assert_eq!(code(&prove(&constant, &refl(), &data(), &Nothing)), "T081");
    // `funext` proves only functions equal
    let at_bool = equal(vec![("x", Ty::Bool)], Ty::Bool, var("x"), var("x"));
    assert_eq!(code(&prove(&at_bool, &funext, &data(), &Nothing)), "T083");
}

#[test]
fn a_split_proves_each_case() {
    // match v with None -> None | Some x -> Some x  =  v
    let rebuilt = Term::Match(
        Rc::new(var("v")),
        vec![
            super::term::Arm {
                pat: Pat::Con(ctor("None", 0), vec![]),
                body: none(),
            },
            super::term::Arm {
                pat: Pat::Con(ctor("Some", 1), vec![Pat::Bind("x".into())]),
                body: some(var("x")),
            },
        ]
        .into(),
    );
    let goal = equal(
        vec![("v", option(rigid("a")))],
        option(rigid("a")),
        rebuilt,
        var("v"),
    );
    assert_eq!(code(&prove(&goal, &refl(), &data(), &Nothing)), "T081");
    let arm = |name, arity, binders: Vec<Pat>| (Pat::Con(ctor(name, arity), binders), refl());
    let split = |arms| cert(Rule::Case("v".into(), arms));
    let both = split(vec![
        arm("None", 0, vec![]),
        arm("Some", 1, vec![Pat::Bind("y".into())]),
    ]);
    assert_eq!(code(&prove(&goal, &both, &data(), &Nothing)), "ok");
    // every case, once
    let missing = split(vec![arm("None", 0, vec![])]);
    assert_eq!(code(&prove(&goal, &missing, &data(), &Nothing)), "T083");
    let twice = split(vec![
        arm("None", 0, vec![]),
        arm("None", 0, vec![]),
        arm("Some", 1, vec![Pat::Wild]),
    ]);
    assert_eq!(code(&prove(&goal, &twice, &data(), &Nothing)), "T083");
    // a split of what is no variable of the statement
    let stranger = cert(Rule::Case("w".into(), vec![arm("None", 0, vec![])]));
    assert_eq!(code(&prove(&goal, &stranger, &data(), &Nothing)), "T083");
}

#[test]
fn different_values_are_disproved() {
    let goal = equal(
        vec![("x", rigid("a"))],
        option(rigid("a")),
        some(var("x")),
        none(),
    );
    assert_eq!(code(&prove(&goal, &refl(), &data(), &Nothing)), "T080");
}

#[test]
fn trans_needs_both_of_its_steps() {
    let ty = option(rigid("a"));
    let goal = equal(
        vec![("x", rigid("a"))],
        ty,
        some(var("x")),
        app(lam("y", some(var("y"))), var("x")),
    );
    let middle = some(var("x"));
    let good = cert(Rule::Trans(
        middle.clone(),
        Box::new(refl()),
        Box::new(refl()),
    ));
    assert_eq!(code(&prove(&goal, &good, &data(), &Nothing)), "ok");
    let wrong = cert(Rule::Trans(none(), Box::new(refl()), Box::new(refl())));
    assert_eq!(code(&prove(&goal, &wrong, &data(), &Nothing)), "T080");
}

#[test]
fn cong_reads_its_proof_from_a_premise() {
    let a = rigid("a");
    let mut goal = equal(
        vec![("x", a.clone()), ("y", a.clone())],
        option(a.clone()),
        some(var("x")),
        some(var("y")),
    );
    let cong = |p| cert(Rule::Cong("z".into(), some(var("z")), Box::new(p)));
    // `refl` says nothing of which equation it proves
    assert_eq!(
        code(&prove(&goal, &cong(refl()), &data(), &Nothing)),
        "T083"
    );
    goal.premises.push((
        "h".into(),
        Equation {
            ty: a,
            left: var("x"),
            right: var("y"),
        },
    ));
    assert_eq!(
        code(&prove(
            &goal,
            &cong(cert(Rule::Fact("h".into()))),
            &data(),
            &Nothing
        )),
        "ok"
    );
    // the premise proves `x = y`, not `y = x`
    let flipped = cert(Rule::Symm(Box::new(cert(Rule::Fact("h".into())))));
    assert_eq!(
        code(&prove(&goal, &cong(flipped), &data(), &Nothing)),
        "T081"
    );
    assert_eq!(
        code(&prove(
            &goal,
            &cert(Rule::Fact("absent".into())),
            &data(),
            &Nothing
        )),
        "T083"
    );
}

#[test]
fn types_outside_the_domain_are_refused() {
    let unknown = equal(
        vec![("x", Ty::Refused("its type is not fully known".into()))],
        Ty::Bool,
        var("x"),
        var("x"),
    );
    assert_eq!(code(&prove(&unknown, &refl(), &data(), &Nothing)), "T082");
    let mut recursive = data();
    recursive
        .get_mut(&("Prelude".into(), "Option".into()))
        .expect("declared")
        .recursive = true;
    let goal = equal(
        vec![("v", option(Ty::Bool))],
        option(Ty::Bool),
        var("v"),
        var("v"),
    );
    assert_eq!(code(&prove(&goal, &refl(), &recursive, &Nothing)), "T082");
    // a constructor of a type the proof does not know
    let goal = equal(vec![], option(Ty::Bool), none(), none());
    assert_eq!(code(&prove(&goal, &refl(), &Data::new(), &Nothing)), "T082");
}

#[test]
fn a_proof_that_runs_out_proves_nothing() {
    // nine doublings: `f` applied 512 times, past the depth of a normal form
    let f = fun(Ty::Bool, Ty::Bool);
    let twice = lam("g", lam("y", app(var("g"), app(var("g"), var("y")))));
    let mut nested = var("f");
    for _ in 0..9 {
        nested = app(var("t"), nested);
    }
    let left = app(app(lam("t", nested), twice), var("x"));
    let goal = equal(
        vec![("f", f), ("x", Ty::Bool)],
        Ty::Bool,
        left,
        app(var("f"), var("x")),
    );
    assert_eq!(code(&prove(&goal, &refl(), &data(), &Nothing)), "T084");
    // the same, every time
    assert_eq!(code(&prove(&goal, &refl(), &data(), &Nothing)), "T084");
}
