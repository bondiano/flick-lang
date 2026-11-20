#[test]
fn current_actual_form_and_checker_issue_are_required() {
    test_checker::with_checker(|checker| {
        define(checker, vec![member("F", None)]);
        let contract = Type::con("Probe", "S");
        let witness = checker
            .materialize_existential_witness(&contract, "W")
            .expect("named witness");
        test_checker::with_checker(|other| {
            define(other, vec![member("F", None)]);
            assert!(other.validate_existential_witness(&witness).is_err());
            assert!(other.env.type_def("W", "F").is_none());
        });
        checker.env.define_type(
            "S",
            TypeDef {
                kind: Kind::Module,
                form: Form::ModuleSignature {
                    signature: Box::new(ModuleSignature::Structure {
                        requirements: Vec::new(),
                    }),
                },
                variance: Vec::new(),
            },
        );
        assert!(
            checker
                .materialize_existential_witness(&contract, "Other")
                .is_err(),
            "Module kind alone is not existential authority"
        );
        checker.env.define_type(
            "S",
            TypeDef {
                kind: Kind::Type,
                form: Form::ModuleContract {
                    requirements: Vec::new(),
                },
                variance: Vec::new(),
            },
        );
        assert!(
            checker
                .materialize_existential_witness(&contract, "WrongKind")
                .is_err()
        );
    });
}

#[test]
fn aliases_remain_live_without_bumping_the_raw_contract_revision() {
    test_checker::with_checker(|checker| {
        checker.env.define_type(
            "A",
            TypeDef {
                kind: Kind::Type,
                form: Form::Alias {
                    params: Vec::new(),
                    body: Type::Tuple(Vec::new()),
                    opaque: false,
                },
                variance: Vec::new(),
            },
        );
        define(checker, vec![member("F", Some(Type::con("Probe", "A")))]);
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("named witness");
        let changed = Type::Record(Row::closed(vec![(
            Elem::named("x"),
            Type::Tuple(Vec::new()),
        )]));
        checker.env.define_type(
            "A",
            TypeDef {
                kind: Kind::Type,
                form: Form::Alias {
                    params: Vec::new(),
                    body: changed.clone(),
                    opaque: false,
                },
                variance: Vec::new(),
            },
        );
        let current = checker
            .validate_existential_witness(&witness)
            .expect("live canonical alias");
        assert!(Arc::ptr_eq(&current.0.revision, &witness.0.revision));
        assert_eq!(checker.witness_head(&Type::con("W", "F")), changed);
        assert!(!checker.env.is_witness_manifest_alias("Probe", "A"));
    });
}

#[test]
fn late_completion_refreshes_impl_descendants_and_alias_provenance() {
    test_checker::with_checker(|checker| {
        let child = |manifest| R::Module {
            name: "Child".into(),
            binder: "child".into(),
            signature: Box::new(ModuleSignature::Structure {
                requirements: vec![member("G", manifest)],
            }),
        };
        define(
            checker,
            vec![
                member("F", Some(Type::Tuple(Vec::new()))),
                service(Vec::new()),
                child(Some(Type::Tuple(Vec::new()))),
            ],
        );
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("initial view");
        assert!(checker.env.is_witness_manifest_alias("W", "F"));
        assert!(checker.env.is_witness_manifest_alias("W.Child", "G"));
        let operation = Operation {
            name: "tick".into(),
            ty: Type::pure_fun(Type::Tuple(Vec::new()), Type::Tuple(Vec::new())),
            class: OpClass::Tail,
        };
        define(
            checker,
            vec![
                member("F", None),
                member("H", Some(Type::Tuple(Vec::new()))),
                service(vec![operation]),
                child(None),
            ],
        );
        let current = checker
            .validate_existential_witness(&witness)
            .expect("completed declaration");
        assert!(!Arc::ptr_eq(&current.0.revision, &witness.0.revision));
        assert!(!checker.env.is_witness_manifest_alias("W", "F"));
        assert!(!checker.env.is_witness_manifest_alias("W.Child", "G"));
        assert!(checker.env.is_witness_manifest_alias("W", "H"));
        assert!(
            !checker.env.is_witness_manifest_alias("W", "Clock.Impl"),
            "derived alias is not a manifest type member"
        );
        let Form::Alias { body, .. } = &checker
            .env
            .type_def("W", "Clock.Impl")
            .expect("canonical companion")
            .form
        else {
            panic!("implementation alias")
        };
        let Type::Record(fields) = body else {
            panic!("record")
        };
        assert_eq!(fields.entries[0].0, Elem::named("tick"));
        assert!(
            checker.validate_existential_witness(&witness).is_ok(),
            "older issued handles refresh, not fail as stale"
        );
    });
}

#[test]
fn failed_full_preflight_changes_no_owned_slot_marker_or_revision() {
    test_checker::with_checker(|checker| {
        define(
            checker,
            vec![
                member("F", Some(Type::Tuple(Vec::new()))),
                service(Vec::new()),
            ],
        );
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("initial view");
        let before = checker.env.type_def("W", "F").cloned();
        let implementation = checker.env.type_def("W", "Clock.Impl").cloned();
        let class = R::Nominal {
            name: "C".into(),
            category: ModuleNominalKind::Class,
            identity: None,
            definition: Some(Box::new(TypeDef {
                kind: Kind::Constraint,
                form: Form::Class {
                    params: Vec::new(),
                    supers: Vec::new(),
                    fundeps: Vec::new(),
                    methods: Vec::new(),
                    defaults: Vec::new(),
                    default_sigs: Vec::new(),
                    derive: false,
                },
                variance: Vec::new(),
            })),
            methods: Vec::new(),
        };
        define(
            checker,
            vec![
                member("F", None),
                class,
                member("C$Dict", Some(Type::Tuple(Vec::new()))),
            ],
        );
        assert!(checker.validate_existential_witness(&witness).is_err());
        assert_eq!(checker.env.type_def("W", "F").cloned(), before);
        assert_eq!(
            checker.env.type_def("W", "Clock.Impl").cloned(),
            implementation
        );
        assert!(checker.env.is_witness_manifest_alias("W", "F"));
        assert!(checker.env.type_def("W", "C").is_none());
        assert!(Arc::ptr_eq(
            &checker.named_witnesses.templates[&witness.0.contract].revision,
            &witness.0.revision
        ));
        assert_eq!(checker.named_witnesses.issued["W"].len(), 1);
    });
}

#[test]
fn solving_and_rolling_back_live_roots_does_not_freeze_or_revise_them() {
    test_checker::with_checker(|checker| {
        let kind = checker.kinds.fresh();
        let ty = checker.store.fresh_of(&Kind::Type);
        define(
            checker,
            vec![
                R::Type {
                    name: "F".into(),
                    params: vec![Binder {
                        name: "a".into(),
                        kind: kind.clone(),
                    }],
                    manifest: None,
                },
                R::Value {
                    name: "value".into(),
                    ty: ty.clone(),
                },
            ],
        );
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("live inference is allowed");
        let result: Result<(), ()> = checker.inference_trial(|checker| {
            checker
                .kinds
                .unify(&kind, &Kind::Symbol)
                .expect("kind solution");
            checker
                .unify(&ty, &Type::Tuple(Vec::new()))
                .expect("type solution");
            let solved = checker
                .validate_existential_witness(&witness)
                .expect("current live view");
            assert!(Arc::ptr_eq(&solved.0.revision, &witness.0.revision));
            Err(())
        });
        assert!(result.is_err());
        assert_eq!(checker.kinds.resolve_deep(&kind), kind);
        let current = checker
            .validate_existential_witness(&witness)
            .expect("retry after rollback");
        assert!(Arc::ptr_eq(&current.0.revision, &witness.0.revision));
        assert_eq!(checker.zonk(&ty), ty);
    });
}

#[test]
fn alpha_equal_source_carriers_are_repaired_even_when_the_raw_revision_is_equal() {
    test_checker::with_checker(|checker| {
        let body = || Type::Exists {
            scope: Scope::default(),
            binder: "K".into(),
            contract: Box::new(Type::con("Probe", "Inner")),
            body: Box::new(Type::Tuple(Vec::new())),
        };
        checker.env.define_type(
            "Inner",
            TypeDef {
                kind: Kind::Module,
                form: Form::ModuleContract {
                    requirements: Vec::new(),
                },
                variance: Vec::new(),
            },
        );
        define(checker, vec![member("F", Some(body()))]);
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("initial view");
        define(checker, vec![member("F", Some(body()))]);
        let current = checker
            .validate_existential_witness(&witness)
            .expect("fresh source handles repaired");
        assert!(
            Arc::ptr_eq(&current.0.revision, &witness.0.revision),
            "public Eq can select only the revision bucket"
        );
        assert!(
            !Arc::ptr_eq(&current.0, &witness.0),
            "current carriers require a fresh exact issued pair"
        );
    });
}

#[test]
fn equal_semantic_owned_definitions_do_not_replace_exact_issued_carriers() {
    test_checker::with_checker(|checker| {
        let existential = Type::Exists {
            scope: Scope::default(),
            binder: "K".into(),
            contract: Box::new(Type::con("Probe", "Inner")),
            body: Box::new(Type::Tuple(Vec::new())),
        };
        checker.env.define_type(
            "Inner",
            TypeDef {
                kind: Kind::Module,
                form: Form::ModuleContract {
                    requirements: Vec::new(),
                },
                variance: Vec::new(),
            },
        );
        define(checker, vec![member("F", Some(existential))]);
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("initial view");
        let before = checker.env.type_def("W", "F").expect("issued slot").clone();
        let mut forged = before.clone();
        let Form::Alias { body, .. } = &mut forged.form else {
            panic!("alias")
        };
        let Type::Exists { scope, .. } = body else {
            panic!("exists")
        };
        *scope = Scope::default();
        assert_eq!(
            before, forged,
            "public semantic equality intentionally erases lexical handles"
        );
        let module = mem::replace(&mut checker.env.module, "W".into());
        checker.env.define_type("F", forged.clone());
        checker.env.module = module;
        assert!(checker.validate_existential_witness(&witness).is_err());
        assert!(
            carrier::definition_stamps(
                checker
                    .env
                    .type_def("W", "F")
                    .expect("unchanged forged slot")
            ) == carrier::definition_stamps(&forged)
        );
        assert!(Arc::ptr_eq(
            &checker.named_witnesses.templates[&witness.0.contract].revision,
            &witness.0.revision
        ));
    });
}

#[test]
fn refresh_never_installs_fixed_foreign_declarations_and_rejects_extra_owned_slots() {
    test_checker::with_checker(|checker| {
        let foreign = R::Nominal {
            name: "Fixed".into(),
            category: ModuleNominalKind::Service,
            identity: Some(Entity {
                module: "Foreign".into(),
                namespace: Namespace::Type,
                name: "Clock".into(),
            }),
            definition: Some(Box::new(TypeDef {
                kind: Kind::Service,
                form: Form::Service {
                    params: Vec::new(),
                    operations: Vec::new(),
                    effect: false,
                },
                variance: Vec::new(),
            })),
            methods: Vec::new(),
        };
        define(
            checker,
            vec![member("F", Some(Type::Tuple(Vec::new()))), foreign.clone()],
        );
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("fixed identity view");
        assert!(
            checker.env.type_def("Foreign", "Clock").is_none(),
            "snapshot authority is not global definition installation"
        );
        define(checker, vec![member("F", None), foreign]);
        let current = checker
            .validate_existential_witness(&witness)
            .expect("owned refresh");
        assert!(checker.env.type_def("Foreign", "Clock").is_none());
        assert!(checker.env.type_def("Foreign", "Clock.Impl").is_none());
        let before = checker.env.type_def("W", "F").cloned();
        let module = mem::replace(&mut checker.env.module, "W.Child".into());
        checker.env.define_type(
            "Unexpected",
            TypeDef {
                kind: Kind::Type,
                form: Form::NOMINAL,
                variance: Vec::new(),
            },
        );
        checker.env.module = module;
        assert!(checker.validate_existential_witness(&current).is_err());
        assert_eq!(checker.env.type_def("W", "F").cloned(), before);
        assert!(
            checker.env.type_def("W.Child", "Unexpected").is_some(),
            "foreign contamination is not erased by a prefix cleanup"
        );
    });
}
