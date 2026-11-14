/// A signature's bare data with hidden storage (`regions.md` §1.4): each
/// parameter, a callback's included, gets a fresh capture row, the result
/// what the parameters before it reach, as `Scheme.elide` of the core.
#[test]
fn bare_data_in_a_signature_reads_its_hidden_rows() {
    let job = || Type::con(T, "Job");
    let callback = Type::fun(job(), Stage::Pure, ty("Int"));
    let s = mono(Type::fun(
        job(),
        Stage::Pure,
        Type::fun(callback, Stage::Pure, job()),
    ));
    let read = Checker::new(&decls()).live_scheme(&s);
    let rows: Vec<_> = (read.vars.iter())
        .map(|(n, k)| (n.to_string(), k.clone()))
        .collect();
    let c = |i: usize| Var::Named(format!("captures${i}").as_str().into());
    assert_eq!(
        rows,
        [
            ("captures$0".to_owned(), Kind::Captures),
            ("captures$1".to_owned(), Kind::Captures),
        ]
    );
    let held = Caps::var(c(0)).union(&Caps::var(c(1)));
    let expected = Type::fun(
        Type::Captured(Rc::new(job()), Caps::var(c(0))),
        Stage::Pure,
        Type::fun(
            Type::fun(
                Type::Captured(Rc::new(job()), Caps::var(c(1))),
                Stage::Pure,
                ty("Int"),
            ),
            Stage::Pure,
            Type::Captured(Rc::new(job()), held),
        ),
    );
    assert_eq!(read.body, expected);
    // a constant and a written qualifier read no row
    let constant = mono(job());
    assert_eq!(Checker::new(&decls()).live_scheme(&constant).body, job());
}

/// (Provide) of an instance of a service with arguments: a handler finds it
/// by its name, so the rest of the body may hold no other instance of it. A
/// variable is taken without the name, or the instance is named beside it;
/// one taken without the name is not given it back by `Store Int`.
#[test]
fn a_provided_instance_leaves_no_variable_another() {
    let store = Path::parse("App.Store");
    let instance = |t: &str| {
        let mut r = Row::none();
        r.elems.insert(store.clone(), Rc::from([ty(t)]));
        r
    };
    let s = Var::Named("s".into());
    let open = |less: &[Path]| {
        let mut r = Row::none();
        r.add_var(&s, &less.iter().cloned().collect());
        r
    };
    let d = decls();
    let mut c = Checker::new(&d);
    rejected_by(c.minus(&open(&[]), &instance("Int")), "provide");
    let lacking = open(slice::from_ref(&store));
    assert_eq!(
        c.minus(&lacking, &instance("Int")).ok(),
        Some(lacking.clone())
    );
    // `{Store Int | s - Store}` holds `s` only where `s` lacks `Store`
    let mut named = instance("Int");
    named.add_var(&s, &[store.clone()].into_iter().collect());
    assert!(!open(&[]).within(&named));
    assert!(lacking.within(&named));
}

#[test]
fn provided_facts_are_dispatched_after_the_common_scope_owner_is_known() {
    let work = Row::var(Var::Named("r".into()));
    let stage = |owner: Type| {
        let mut row = work.clone();
        row.elems
            .insert(Path::new("Prelude", "Scope"), Rc::from([owner.clone()]));
        let Type::Fun(_, stage, _) =
            Type::fun(ty("Unit"), Stage::Runtime(row, Row::none()), ty("Unit"))
                .with_provided(&work, &owner)
        else {
            unreachable!()
        };
        stage
    };
    let owner = Type::Var(Var::Named("outer".into()));
    let declared = stage(owner);
    let d = decls();
    let mut checker = Checker::new(&d);
    checker.give_provided(&Type::fun(ty("Unit"), declared.clone(), ty("Unit")));
    let inferred = stage(checker.fresh());
    assert_eq!(checker.stage_within(&inferred, &declared), Ok(()));

    let other = stage(Type::Var(Var::Named("inner".into())));
    rejected_by(checker.stage_within(&other, &other), "⊆");
}

#[test]
fn a_signed_body_assumes_its_bound_but_an_unbounded_caller_does_not() {
    let bounded = Type::Bounded(Rc::new(a()), Caps::default());
    let signature = poly(Type::fun(bounded.clone(), Stage::Pure, bounded));
    let mut d = decls();
    d.values
        .insert(Path::new("App", "bounded"), signature.clone());
    let forward = Term::lam(
        "value",
        Term::app(Term::Global(Path::new("App", "bounded")), var("value")),
    );
    assert!(Checker::new(&d).check_top(&forward, &signature).is_ok());
    let unbounded = poly(Type::fun(a(), Stage::Pure, a()));
    rejected_by(Checker::new(&d).check_top(&forward, &unbounded), "Inst");
}

#[test]
fn bound_facts_are_scoped_and_obligations_keep_only_their_original_facts() {
    let signature = poly(Type::Bounded(Rc::new(a()), Caps::default()));
    let d = decls();
    let mut checker = Checker::new(&d);
    let before = checker.givens();
    let (_, rigid) = checker.skolemize(&signature);
    let held = Type::Var(Var::Named(rigid[0].clone()));
    checker.hold(slice::from_ref(&held), &[], &Caps::default(), "Inst");
    checker.forget_givens(before);
    assert_eq!(
        checker.finish(),
        Ok(()),
        "the earlier obligation keeps its fact"
    );
    checker.hold(&[held], &[], &Caps::default(), "Inst");
    rejected_by(checker.finish(), "Inst");
}

#[test]
fn bound_facts_do_not_chain_through_another_rigid_atom() {
    let signature = Scheme::new(
        vec![("a".into(), Kind::Type), ("b".into(), Kind::Type)],
        Type::Tuple(Rc::from([
            Type::Bounded(Rc::new(a()), Caps::atom(Var::Named("b".into()))),
            Type::Bounded(Rc::new(Type::Var(Var::Named("b".into()))), Caps::default()),
        ])),
    );
    let d = decls();
    let mut checker = Checker::new(&d);
    let (_, rigid) = checker.skolemize(&signature);
    let held = Type::Tuple(
        rigid
            .into_iter()
            .map(|name| Type::Var(Var::Named(name)))
            .collect(),
    );
    checker.hold(&[held], &[], &Caps::default(), "Inst");
    rejected_by(checker.finish(), "Inst");
}

#[test]
fn a_delayed_provided_owner_must_match_a_fact_from_its_generation_site() {
    let d = decls();
    let work = Row::var(Var::Named("r".into()));
    let signature = |owner| Type::fun(ty("Unit"), rt(), ty("Unit")).with_provided(&work, &owner);
    let outer = Type::Var(Var::Named("outer".into()));
    for case in 0..4 {
        let mut checker = Checker::new(&d);
        let before = checker.givens();
        if case != 3 {
            checker.give_provided(&signature(outer.clone()));
        }
        let owner = checker.fresh();
        let Type::Fun(_, stage, _) = signature(owner.clone()) else {
            unreachable!()
        };
        assert_eq!(checker.stage_within(&stage, &rt()), Ok(()));
        if case == 3 {
            checker.give_provided(&signature(outer.clone()));
        }
        if case != 3 {
            checker.forget_givens(before);
        }
        if case != 2 {
            let solved = if case == 1 {
                Type::Var(Var::Named("inner".into()))
            } else {
                outer.clone()
            };
            checker
                .unify(&owner, &solved)
                .expect("later call fixes its owner");
        }
        if case == 0 {
            assert_eq!(checker.finish(), Ok(()));
        } else {
            rejected_by(checker.finish(), "⊆");
        }
    }
}

#[test]
fn raw_aliases_observe_erased_source_schemes_without_becoming_source_exports() {
    let source = Path::new("App", "source");
    let raw = Path::new("App", "$raw");
    let bounded = Type::Bounded(Rc::new(a()), Caps::default());
    let signature = poly(Type::fun(bounded.clone(), Stage::Pure, bounded));
    let bare = poly(Type::fun(a(), Stage::Pure, a()));
    let mut d = decls();
    d.values.insert(source.clone(), signature.clone());
    d.raw_aliases.insert(raw.clone(), source.clone());
    assert_eq!(d.value(&raw), Some(&signature));
    *d.values.get_mut(&source).expect("source declaration") = bare.clone();
    assert_eq!(d.value(&raw), Some(&bare));
    assert!(
        !d.values.contains_key(&raw),
        "private names do not enter GenSig"
    );
}

#[test]
fn generalization_keeps_an_owner_with_an_open_outlives_obligation_monomorphic() {
    let d = decls();
    let mut checker = Checker::new(&d);
    let owner = Type::Var(Var::Named("s".into()));
    let captures = Caps::var(Var::Named("c".into()));
    let mut row = Row::none();
    row.elems
        .insert(Path::new("Prelude", "Scope"), Rc::from([owner.clone()]));
    let signature = Scheme {
        vars: vec![("s".into(), Kind::Region), ("c".into(), Kind::Captures)],
        body: Type::fun(
            ty("Unit"),
            Stage::Runtime(row, Row::none()),
            Type::app(Type::con("Fiber", "Owner"), owner.clone()),
        ),
        outlives: vec![(captures, owner)],
    };
    checker.level += 1;
    let partial = checker.instantiate(&signature);
    checker.level -= 1;
    let generalized = checker.generalize(&partial);
    assert!(
        generalized.vars.is_empty(),
        "open owner was quantified: {generalized:?}"
    );
}

#[test]
fn a_capture_row_bound_does_not_prove_an_empty_bound() {
    let bounded = |caps| Type::Bounded(Rc::new(a()), caps);
    let mut d = decls();
    d.values.insert(
        Path::new("App", "sink"),
        poly(Type::fun(bounded(Caps::default()), Stage::Pure, ty("Unit"))),
    );
    let forward = Term::lam(
        "value",
        Term::app(Term::Global(Path::new("App", "sink")), var("value")),
    );
    for empty in [false, true] {
        let captures = if empty {
            Caps::default()
        } else {
            Caps::var(Var::Named("c".into()))
        };
        let signature = Scheme::new(
            vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
            Type::fun(bounded(captures), Stage::Pure, ty("Unit")),
        );
        let result = Checker::new(&d).check_top(&forward, &signature);
        if empty {
            assert_eq!(result, Ok(()));
        } else {
            rejected_by(result, "Inst");
        }
    }
}

#[test]
fn a_capture_row_bound_is_allowed_by_the_same_capture_row() {
    let d = decls();
    let mut checker = Checker::new(&d);
    let signature = Scheme::new(
        vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
        Type::Bounded(Rc::new(a()), Caps::var(Var::Named("c".into()))),
    );
    let (_, rigid) = checker.skolemize(&signature);
    checker.hold(
        &[Type::Var(Var::Named(rigid[0].clone()))],
        &[],
        &Caps::var(Var::Named(rigid[1].clone())),
        "Inst",
    );
    assert_eq!(checker.finish(), Ok(()));
}

#[test]
fn a_capture_row_bound_grows_an_open_allowance_without_inventing_a_type_atom() {
    let d = decls();
    let mut checker = Checker::new(&d);
    let signature = Scheme::new(
        vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
        Type::Bounded(Rc::new(a()), Caps::var(Var::Named("c".into()))),
    );
    let (_, rigid) = checker.skolemize(&signature);
    let allowed = checker.fresh_caps();
    checker.hold(
        &[Type::Var(Var::Named(rigid[0].clone()))],
        &[],
        &allowed,
        "Inst",
    );
    assert_eq!(checker.finish(), Ok(()));
    assert_eq!(
        checker.zonk_caps(&allowed),
        Caps::var(Var::Named(rigid[1].clone()))
    );
}

/// Reading a signature retains the written capacity of its outer parameter
/// qualifier. It is an allowance of the stage, not a use of opaque `a`.
#[test]
fn signature_elision_retains_an_outer_written_parameter_capacity() {
    let captures = Caps::var(Var::Named("c".into()));
    let parameter = Type::Bounded(Rc::new(a()), captures.clone());
    let signature = Scheme::new(
        vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
        Type::fun(parameter.clone(), rt(), ty("Unit")),
    );
    let mut services = Row::none();
    services.elems.insert(
        Path::new("Prelude", "Live"),
        Rc::from([Type::Captured(
            Rc::new(Type::fun(ty("Unit"), Stage::Pure, ty("Unit"))),
            captures,
        )]),
    );
    assert_eq!(
        Checker::new(&decls()).live_scheme(&signature).body,
        Type::fun(parameter, Stage::Runtime(services, Row::none()), ty("Unit"))
    );
}

#[test]
fn signature_elision_allows_forwarding_to_a_capacity_qualified_callable() {
    let captures = Caps::var(Var::Named("c".into()));
    let signature = Scheme::new(
        vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
        Type::fun(
            Type::Bounded(Rc::new(a()), captures.clone()),
            rt(),
            ty("Unit"),
        ),
    );
    let target = Path::new("App", "qualified");
    let mut qualified = signature.clone();
    qualified.body = Type::Captured(Rc::new(qualified.body), captures);
    let mut d = decls();
    d.values.insert(target.clone(), qualified);
    let forward = Term::lam("value", Term::app(Term::Global(target), var("value")));
    assert_eq!(Checker::new(&d).check_top(&forward, &signature), Ok(()));
}

/// A capacity below a parameter's forall remains bound there. In particular,
/// it is not a free Live requirement of the stage that receives the callback.
#[test]
fn signature_elision_does_not_leak_a_nested_forall_capacity_or_type_atom() {
    let optional = || Type::app(Type::con("Prelude", "Option"), a());
    let callback = Type::Forall(Rc::new(Scheme::new(
        vec![("v".into(), Kind::Captures)],
        Type::fun(
            Type::Bounded(Rc::new(optional()), Caps::var(Var::Named("v".into()))),
            Stage::Pure,
            optional(),
        ),
    )));
    let signature = poly(Type::fun(callback, rt(), ty("Unit")));
    assert_eq!(
        Checker::new(&decls()).live_scheme(&signature).body,
        signature.body
    );
}

#[test]
fn signature_elision_does_not_allow_an_unrelated_closed_region() {
    let closed = Var::Named("closed".into());
    let target = Path::new("App", "closed");
    let mut d = decls();
    d.values.insert(
        target.clone(),
        mono(Type::Captured(
            Rc::new(Type::fun(ty("Unit"), rt(), ty("Unit"))),
            Caps::atom(closed.clone()),
        )),
    );
    let signature = Scheme::new(
        vec![("a".into(), Kind::Type), ("c".into(), Kind::Captures)],
        Type::fun(
            Type::Bounded(Rc::new(a()), Caps::var(Var::Named("c".into()))),
            rt(),
            ty("Unit"),
        ),
    );
    let body = Term::lam("value", Term::app(Term::Global(target), Term::unit()));
    let mut checker = Checker::new(&d);
    checker.regions.kinds.insert(closed);
    rejected_by(checker.check_top(&body, &signature), "Live");
}

#[test]
fn signature_elision_does_not_require_live_for_a_capture_free_scalar() {
    let parameter = Type::Bounded(Rc::new(ty("Int")), Caps::var(Var::Named("c".into())));
    let signature = Scheme::new(
        vec![("c".into(), Kind::Captures)],
        Type::fun(parameter, rt(), ty("Unit")),
    );
    let Type::Fun(_, stage, _) = Checker::new(&decls()).live_scheme(&signature).body else {
        panic!("a function signature remains a function");
    };
    assert_eq!(stage, rt());
}
