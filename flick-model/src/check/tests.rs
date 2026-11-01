//! Unit tests of the rules: one accepted and one rejected program each.

use std::fmt::Debug;
use std::rc::Rc;

use super::{Checker, TypeError};
use crate::decls::Decls;
use crate::syntax::{Arm, Con, Const, Lit, Pat, Path, Prim, Term};
use crate::types::{Kind, Row, Scheme, Stage, Type, Var};

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
    }
}

/// `∀a. body`.
fn poly(body: Type) -> Scheme {
    Scheme {
        vars: vec![("a".into(), Kind::Type)],
        body,
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

#[test]
fn var_rule() {
    assert_eq!(
        infer(&Term::app(Term::lam("x", var("x")), int(1)))
            .expect("typed")
            .0,
        ty("Int")
    );
    rejected_by(infer(&var("x")), "Var");
}

#[test]
fn app_rule() {
    rejected_by(infer(&Term::app(int(1), int(2))), "App");
}

#[test]
fn let_gen_generalizes_a_pure_right_side() {
    let id = Term::lam("x", var("x"));
    let both = tuple(vec![
        Term::app(var("id"), int(1)),
        Term::app(var("id"), Term::string("a")),
    ]);
    assert_eq!(
        infer(&let_("id", id, both)).expect("typed").0,
        Type::Tuple(Rc::from([ty("Int"), ty("String")]))
    );
}

#[test]
fn let_mono_keeps_the_result_of_a_runtime_step_monomorphic() {
    let once = let_("f", cell_id(), Term::app(var("f"), int(1)));
    assert!(infer(&task(once)).is_ok());
    let both = tuple(vec![
        Term::app(var("f"), int(1)),
        Term::app(var("f"), Term::string("a")),
    ]);
    rejected_by(infer(&task(let_("f", cell_id(), both))), "≡");
}

/// (Let-Sig) and Р1: a `∀` over a runtime step is `FLICK-T013`.
#[test]
fn let_sig_rule() {
    let id = poly(Type::fun(a(), Stage::Pure, a()));
    assert!(
        infer(&let_sig(
            "f",
            id.clone(),
            Term::lam("x", var("x")),
            Term::app(var("f"), int(1))
        ))
        .is_ok()
    );
    rejected_by(
        infer(&task(let_sig(
            "f",
            id,
            cell_id(),
            Term::app(var("f"), int(1)),
        ))),
        "Let-Sig",
    );
}

#[test]
fn rec_and_field() {
    let r = record(&[("a", int(1))]);
    assert_eq!(
        infer(&Term::Field(Rc::new(r.clone()), "a".into()))
            .expect("typed")
            .0,
        ty("Int")
    );
    rejected_by(infer(&Term::Field(Rc::new(r), "b".into())), "Field");
    rejected_by(infer(&record(&[("a", int(1)), ("a", int(2))])), "Rec");
}

#[test]
fn match_needs_exhaustive_arms() {
    let yes = arm(Pat::Lit(Lit::Bool(true)), int(1));
    let no = arm(Pat::Lit(Lit::Bool(false)), int(0));
    assert!(infer(&match_(Term::bool(true), vec![yes.clone(), no])).is_ok());
    rejected_by(infer(&match_(Term::bool(true), vec![yes])), "Match");
}

/// §2.6 and `FLICK-E001`: initialization is pure, and a pure arrow does
/// not take a runtime body.
#[test]
fn pure_stage() {
    let make = Term::lam("x", konst(Const::RefMake, vec![var("x")]));
    let to = |s| mono(Type::fun(ty("Int"), s, Type::Ref(Rc::new(ty("Int")))));
    assert!(check(&make, &to(rt())).is_ok());
    rejected_by(check(&make, &to(Stage::Pure)), "⊑");
    rejected_by(
        check(
            &konst(Const::RefMake, vec![int(1)]),
            &mono(Type::Ref(Rc::new(ty("Int")))),
        ),
        "Module",
    );
}

/// ≤-Fun: a pure function is lifted where a runtime one is expected, not
/// the other way round.
#[test]
fn sub_lifts_a_pure_function() {
    let call = |f: Term, s: Stage| {
        let g = mono(Type::fun(
            Type::fun(ty("Int"), s, ty("Int")),
            rt(),
            ty("Int"),
        ));
        let_(
            "h",
            f,
            let_sig(
                "g",
                g,
                Term::lam("f", Term::app(var("f"), int(1))),
                task(Term::app(var("g"), var("h"))),
            ),
        )
    };
    assert!(infer(&call(Term::lam("x", var("x")), rt())).is_ok());
    let runtime = Term::lam(
        "x",
        konst(Const::RefGet, vec![konst(Const::RefMake, vec![var("x")])]),
    );
    rejected_by(infer(&call(runtime, Stage::Pure)), "⊑");
}

#[test]
fn task_and_await() {
    let await_ = |e: Term| Term::Await(Rc::new(e));
    assert!(check(&task(await_(task(int(1)))), &mono(eff(ty("Int")))).is_ok());
    rejected_by(infer(&task(await_(int(1)))), "≡");
}

/// (Task): the body's errors are within the `Eff`'s.
#[test]
fn task_bounds_the_errors_of_its_body() {
    let errors = Row::one(Path::new(T, "A"));
    let with = mono(Type::Eff(Row::none(), errors, Rc::new(ty("Unit"))));
    assert!(check(&task(raise(a_err())), &with).is_ok());
    rejected_by(check(&task(raise(a_err())), &mono(eff(ty("Unit")))), "⊆");
}

/// (Try): an arm `A x` without `when` takes `A` off the row, one with it
/// does not.
#[test]
fn try_takes_off_what_an_unguarded_arm_covers() {
    let try_ = |guard: Option<Term>| {
        let pat = Pat::Con(con("A", "A", 1), Rc::from([Pat::Var("x".into())]));
        let arms = [Arm {
            pat,
            guard,
            body: Term::unit(),
        }];
        task(Term::Try(Rc::new(raise(a_err())), arms.into()))
    };
    assert!(check(&try_(None), &mono(eff(ty("Unit")))).is_ok());
    rejected_by(
        check(&try_(Some(Term::bool(true))), &mono(eff(ty("Unit")))),
        "⊆",
    );
}

/// Р3: an arm `e` whose use asks for `A` is not a full arm when the body
/// also raises `B`.
#[test]
fn try_arm_variable_is_errors_of_the_body() {
    let payload = Term::lam(
        "e",
        match_(
            var("e"),
            vec![arm(
                Pat::Con(con("A", "A", 1), Rc::from([Pat::Wild])),
                Term::unit(),
            )],
        ),
    );
    let body = |both: bool| {
        let raised = if both {
            match_(
                Term::bool(true),
                vec![
                    arm(Pat::Lit(Lit::Bool(true)), raise(a_err())),
                    arm(Pat::Lit(Lit::Bool(false)), raise(b_err())),
                ],
            )
        } else {
            raise(a_err())
        };
        let handler = arm(Pat::Var("e".into()), Term::app(var("payload"), var("e")));
        task(let_(
            "payload",
            payload.clone(),
            Term::Try(Rc::new(raised), Rc::from([handler])),
        ))
    };
    assert!(check(&body(false), &mono(eff(ty("Unit")))).is_ok());
    rejected_by(check(&body(true), &mono(eff(ty("Unit")))), "≡");
}

/// (Provide), and a record checked as `S.Impl` field by field: the pure
/// `λs. ()` stands for the runtime operation `log`.
#[test]
fn provide_gives_the_services_of_its_layer() {
    let ask = Term::Field(
        Rc::new(konst(Const::Ask(log()), vec![Term::unit()])),
        "log".into(),
    );
    let call = Term::app(ask, Term::string("hi"));
    let layer = konst(
        Const::LayerSucceed(log()),
        vec![record(&[("log", Term::lam("s", Term::unit()))])],
    );
    let provided = task(Term::Provide(Rc::new(layer), Rc::new(call.clone())));
    assert!(check(&provided, &mono(eff(ty("Unit")))).is_ok());
    rejected_by(check(&task(call), &mono(eff(ty("Unit")))), "⊆");
    let extra = record(&[("log", Term::lam("s", Term::unit())), ("more", int(1))]);
    assert!(infer(&konst(Const::LayerSucceed(log()), vec![extra])).is_err());
}

/// Р9: the alternatives of an or-pattern bind a name at one type.
#[test]
fn or_pattern_alternatives_are_equal() {
    let alt = |c: &str| Pat::Con(con(c, "Job", 1), Rc::from([Pat::Var("f".into())]));
    let run = |p: Pat| {
        Term::lam(
            "j",
            match_(var("j"), vec![arm(p, Term::app(var("f"), Term::unit()))]),
        )
    };
    let bools = Pat::Or(Rc::from([
        Pat::Lit(Lit::Bool(true)),
        Pat::Lit(Lit::Bool(false)),
    ]));
    assert!(infer(&match_(Term::bool(true), vec![arm(bools, int(1))])).is_ok());
    rejected_by(
        infer(&run(Pat::Or(Rc::from([alt("Loud"), alt("Quiet")])))),
        "≡",
    );
}

/// `EffOf ρ ρ' τ ≡ Eff ρ ρ' τ`: a variable `f` of kind `Type → Type`
/// solved by `Eff ρ ρ'` (`Functor (Eff r e)`).
#[test]
fn eff_as_a_type_constructor() {
    let d = decls();
    let mut c = Checker::new(&d);
    let f = c.fresh();
    let r = Row::one(log());
    c.unify(
        &Type::app(f.clone(), ty("Int")),
        &Type::Eff(r.clone(), Row::none(), Rc::new(ty("Int"))),
    )
    .expect("unifies");
    assert_eq!(
        c.zonk(&Type::app(f, ty("String"))),
        Type::Eff(r, Row::none(), Rc::new(ty("String")))
    );
}

/// `<` orders numbers and strings, as flick-core's operators: `λf. f < f`
/// over a function is rejected, an unknown operand becomes `Int` (X2).
#[test]
fn comparisons_order_numbers_and_strings_only() {
    let lt = |a: Term, b: Term| Term::apps(Term::prim(Prim::Lt), [a, b]);
    let f = Term::lam("y", int(0));
    rejected_by(infer(&let_("f", f, lt(var("f"), var("f")))), "Prim");
    let (t, _) = infer(&Term::lam("x", lt(var("x"), var("x")))).expect("an Int");
    assert_eq!(t.to_string(), "Int -> Bool");
    assert!(
        infer(&lt(
            Term::Lit(Lit::String("a".into())),
            Term::Lit(Lit::String("b".into()))
        ))
        .is_ok()
    );
}
