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

/// (Let-Sig), also under an annotation with `∀`: a `∀` over a runtime step is `FLICK-T013`.
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

/// An opaque alias that names itself, `opaque type alias Stream = Option
/// (Float, Stream)`, inside its module: two of it are equal by their
/// arguments, and zonking a type does not unfold it (`tests/model/opaque.rs`
/// checks its source).
#[test]
fn a_recursive_opaque_alias_is_checked() {
    let mut d = decls();
    let stream = Type::con(T, "Stream");
    let tuple = Type::Tuple(Rc::from([ty("Float"), stream.clone()]));
    d.opaque.insert(
        Path::new(T, "Stream"),
        (Vec::new(), Type::app(ty("Option"), tuple)),
    );
    let mut c = Checker::new(&d);
    c.module = T.into();
    let id = mono(Type::fun(stream.clone(), Stage::Pure, stream));
    let checked = c.check_top(&Term::lam("s", var("s")), &id);
    assert!(checked.is_ok(), "{checked:?}");
}

/// A field of a constructor applied by (App) is typed where it stands: the
/// trace a certificate reads has its node.
#[test]
fn the_fields_of_a_constructor_are_traced() {
    let e = b_err();
    let Term::App(_, field) = &e else {
        unreachable!("a call")
    };
    let trace = Checker::new(&decls())
        .check_top_traced(&e, &mono(Type::con(T, "B")))
        .expect("`B \"x\"` is a `B`");
    assert!(trace.contains_key(&key(field)), "the field has no node");
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
