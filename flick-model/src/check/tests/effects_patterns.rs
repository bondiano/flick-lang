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

/// (Task) with a generic value (`regions.md` §2.1, `docs/formal/regions.md#a-bounded-variable-reaches-what-the-variable-reaches`): a task
/// that only holds a value uses nothing of it, so both `a^{≤c}` and a bare
/// `a` may go into a bare `Eff`.
#[test]
fn task_holds_a_type_variable() {
    let c = Var::Named("c".into());
    let body = task(let_("_", var("x"), Term::unit()));
    let lam = Term::lam("x", body);
    let caps = Caps::var(c);
    let bounded = Scheme {
        vars: vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
        body: Type::fun(
            Type::Bounded(Rc::new(a()), caps.clone()),
            Stage::Pure,
            Type::Captured(Rc::new(eff(ty("Unit"))), caps),
        ),
        outlives: Vec::new(),
    };
    assert!(check(&lam, &bounded).is_ok());
    assert!(check(&lam, &poly(Type::fun(a(), Stage::Pure, eff(ty("Unit"))))).is_ok());
}

/// `∀e. Errors e → (Unit → Unit)^{e}`: a qualifier takes what the row
/// given for `e` reaches (`regions.md` §2.1), the rigid `e` of the
/// signature, `{A}`, which reaches nothing, and the variable `f`, which a
/// bare field does not allow.
#[test]
fn an_error_row_variable_is_substituted_in_a_qualifier() {
    let unit_to_unit = Type::fun(ty("Unit"), Stage::Pure, ty("Unit"));
    let errors = |v: &str| Type::Errors(Row::var(Var::Named(v.into())));
    let keep = Scheme {
        vars: vec![("e".into(), Kind::Errors)],
        body: Type::fun(
            errors("e"),
            Stage::Pure,
            Type::Captured(
                Rc::new(unit_to_unit.clone()),
                Caps::atom(Var::Named("e".into())),
            ),
        ),
        outlives: Vec::new(),
    };
    let body = Term::lam("x", Term::lam("_", let_("_", var("x"), Term::unit())));
    assert!(check(&body, &keep).is_ok());
    let run = record(&[("run", Term::app(var("keep"), var("y")))]);
    let use_ = Term::lam("y", let_sig("keep", keep, body, run));
    let field = |param: Type| {
        let fields = Fields {
            fields: [("run".into(), unit_to_unit.clone())].into(),
            tail: None,
        };
        Type::fun(param, Stage::Pure, Type::Record(fields))
    };
    let errors_a = field(Type::Errors(Row::one(Path::new(T, "A"))));
    assert!(check(&use_, &mono(errors_a)).is_ok());
    let errors_f = Scheme {
        vars: vec![("f".into(), Kind::Errors)],
        body: field(errors("f")),
        outlives: Vec::new(),
    };
    rejected_by(check(&use_, &errors_f), "≤-Cap");
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

/// A `try` arm typed by its use (`docs/formal/core.md`): an arm `e` whose use asks for `A` is not a full arm when the body
/// also raises `B`: the use makes the row of `e` `{A}`, which does not hold
/// what the body raises (`raised ⊆ caught`).
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
    rejected_by(check(&body(true), &mono(eff(ty("Unit")))), "⊆");
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

/// The alternatives of an or-pattern bind a name at one type.
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
/// over a function is rejected, an unknown operand becomes `Int` (found by the model properties).
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

#[test]
fn never_has_no_pattern_cases_but_inhabited_types_do() {
    let absurd = Term::lam("x", match_(var("x"), Vec::new()));
    assert!(
        check(
            &absurd,
            &mono(Type::fun(ty("Never"), Stage::Pure, ty("Int")))
        )
        .is_ok()
    );
    rejected_by(
        check(&absurd, &mono(Type::fun(ty("Int"), Stage::Pure, ty("Int")))),
        "Match",
    );
}

/// (Match-Err): a `match` takes a value of `Errors {A, B}` apart by the
/// errors of the row, each arm at its element; an arm missing fails
/// `Exh`, and an error outside the row is no pattern of it.
#[test]
fn a_match_takes_errors_apart_by_their_row() {
    let row = |es: &[&str]| {
        let mut r = Row::none();
        for e in es {
            r.elems.insert(Path::new(T, e), Rc::from([]));
        }
        Type::Errors(r)
    };
    let on = |e: &str| arm(Pat::Con(con(e, e, 1), Rc::from([Pat::Wild])), Term::unit());
    let f = |arms| Term::lam("e", match_(var("e"), arms));
    let of = |es: &[&str]| mono(Type::fun(row(es), Stage::Pure, ty("Unit")));
    assert!(check(&f(vec![on("B"), on("A")]), &of(&["A", "B"])).is_ok());
    rejected_by(check(&f(vec![on("A")]), &of(&["A", "B"])), "Match");
    // `FLICK-T003`
    rejected_by(check(&f(vec![on("A"), on("B")]), &of(&["A"])), "Pattern");
}

/// (Match-Err) of a row not known yet: the patterns of a `match` tell it.
/// Without an arm for every error they close it; with one it stays open
/// for the rest of the program, here what the body of a `try` raises.
#[test]
fn the_arms_tell_a_row_not_known_yet() {
    let on = |e: &str| arm(Pat::Con(con(e, e, 1), Rc::from([Pat::Wild])), Term::unit());
    // `raise x` makes `x : Errors ρ` before the `match` reads it
    let f = |arms| {
        let body = let_("u", raise(var("x")), match_(var("x"), arms));
        let_("f", Term::lam("x", body), Term::unit())
    };
    assert!(check(&f(vec![on("A"), on("B")]), &mono(ty("Unit"))).is_ok());
    let any = arm(Pat::Wild, Term::unit());
    assert!(check(&f(vec![on("A"), any.clone()]), &mono(ty("Unit"))).is_ok());
    let caught = |arms| {
        let x = arm(Pat::Var("x".into()), match_(var("x"), arms));
        task(Term::Try(Rc::new(raise(b_err())), Rc::from([x])))
    };
    assert!(check(&caught(vec![on("A"), any]), &mono(eff(ty("Unit")))).is_ok());
    rejected_by(check(&caught(vec![on("A")]), &mono(eff(ty("Unit")))), "⊆");
}

/// The `x` of a `try` takes a row above its body's, as (Try) with (Sub)
/// types the body: a `match` on it may name an error the body does not
/// raise (`docs/formal/core.md`, "`Errors` of a row with two or more
/// elements").
#[test]
fn a_try_arm_takes_a_row_above_its_body() {
    let on = |e: &str| arm(Pat::Con(con(e, e, 1), Rc::from([Pat::Wild])), Term::unit());
    let x = arm(Pat::Var("x".into()), match_(var("x"), vec![on("A"), on("B")]));
    let t = task(Term::Try(Rc::new(raise(a_err())), Rc::from([x])));
    assert!(check(&t, &mono(eff(ty("Unit")))).is_ok());
}

/// (Errors-In): a term of an error, a value or not, has the type of the
/// errors of a row that names it; a row of one other error is that error,
/// and the rule types a term, not the types built of it.
#[test]
fn a_term_of_an_error_has_the_errors_of_its_row() {
    let row = |es: &[&str]| {
        let mut r = Row::none();
        for e in es {
            r.elems.insert(Path::new(T, e), Rc::from([]));
        }
        Type::Errors(r)
    };
    let both = row(&["A", "B"]);
    assert!(check(&a_err(), &mono(both.clone())).is_ok());
    assert!(check(&let_("y", b_err(), var("y")), &mono(both.clone())).is_ok());
    assert!(check(&b_err(), &mono(row(&["A"]))).is_err());
    let widen = Term::lam("f", var("f"));
    let a_to = |t: Type| Type::fun(ty("Unit"), Stage::Pure, t);
    let of = mono(Type::fun(a_to(Type::con(T, "A")), Stage::Pure, a_to(both)));
    assert!(check(&widen, &of).is_err());
}
