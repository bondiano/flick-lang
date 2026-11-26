#[test]
fn paired_semantic_label_root_follows_late_solution_but_literal_twin_does_not() {
    use crate::types::{Atom, Effects};
    test_checker::with_checker(|checker| {
        define(checker, vec![member("E", None)]);
        let a = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "A")
            .expect("A");
        let outer = checker
            .register_existential_scope(Scope::default(), &a)
            .expect("A scope");
        checker.store.enter_context(outer);
        let root = checker.store.fresh_of(&Kind::Type);
        let Type::Meta(meta) = root.clone() else {
            panic!("scoped root")
        };
        assert!(meta.contextual().is_some(), "scoped semantic label root");
        let raw = Type::Row(Row::closed(vec![(
            Elem::Provided(Box::new(Elem::Error(Atom::Meta(meta)))),
            Type::Tuple(Vec::new()),
        )]));
        checker.env.define_type(
            "Literal",
            TypeDef::of(
                Kind::Module,
                Form::ModuleContract {
                    requirements: vec![R::Value {
                        name: "value".into(),
                        ty: Type::Record(Row {
                            entries: Vec::new(),
                            tail: Some(Box::new(raw.clone())),
                            written: Written::default(),
                        }),
                    }],
                },
            ),
        );
        let literal_pair = checker
            .materialize_existential_witness(&Type::con("Probe", "Literal"), "Fields")
            .expect("literal twin");
        let literal = checker
            .register_existential_scope(Scope::default(), &literal_pair)
            .expect("literal role registration");
        checker.env.define_type(
            "Dependent",
            TypeDef::of(
                Kind::Module,
                Form::ModuleContract {
                    requirements: vec![R::Value {
                        name: "value".into(),
                        ty: Type::Fun {
                            param: Box::new(Type::Tuple(Vec::new())),
                            effects: Some(Box::new(Effects::new(Type::empty_row(), raw))),
                            result: Box::new(Type::Tuple(Vec::new())),
                        },
                    }],
                },
            ),
        );
        let b = checker
            .materialize_existential_witness(&Type::con("Probe", "Dependent"), "B")
            .expect("B");
        let context = checker
            .register_existential_scope(Scope::default(), &b)
            .expect("B scope");
        let thinned = context.restrict(&context.ids().skip(1).collect());
        assert_eq!(thinned.len(), 1);
        checker
            .store
            .set(root.meta_root().expect("root"), Type::con("A", "E"));
        assert!(
            checker.current_witness_context(&thinned).is_err(),
            "late scoped dependency cannot become global when its coordinate was removed"
        );
        assert!(checker.align_witness_contexts(&thinned, &thinned).is_err());
        let literal = checker
            .current_witness_context(&literal)
            .expect("literal keys do not observe semantic atom roots");
        assert_eq!(literal.restrict(&literal.ids().skip(1).collect()).len(), 1);
        let current = checker
            .current_witness_context(&context)
            .expect("live label evidence");
        assert_eq!(
            current.restrict(&current.ids().skip(1).collect()).len(),
            0,
            "a solved semantic label is a necessary dependency even with closed Unit payload"
        );
    });
}

#[test]
fn paired_alignment_refreshes_late_completion_and_keeps_canonical_alias_reads_live() {
    test_checker::with_checker(|checker| {
        checker.env.define_type(
            "Alias",
            TypeDef::of(
                Kind::Type,
                Form::Alias {
                    params: Vec::new(),
                    body: Type::Tuple(Vec::new()),
                    opaque: false,
                },
            ),
        );
        define(
            checker,
            vec![
                member("F", Some(Type::con("Probe", "Alias"))),
                service(Vec::new()),
            ],
        );
        let a = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "A")
            .expect("A");
        let x = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "X")
            .expect("X");
        let ca = checker
            .register_existential_scope(Scope::default(), &a)
            .expect("A scope");
        let cx = checker
            .register_existential_scope(Scope::default(), &x)
            .expect("X scope");
        checker
            .align_witness_contexts(&ca, &cx)
            .expect("current actuals align");
        let changed = Type::Record(Row::closed(vec![(
            Elem::named("answer"),
            Type::Tuple(Vec::new()),
        )]));
        checker.env.define_type(
            "Alias",
            TypeDef::of(
                Kind::Type,
                Form::Alias {
                    params: Vec::new(),
                    body: changed.clone(),
                    opaque: false,
                },
            ),
        );
        let current = checker
            .validate_existential_witness(&a)
            .expect("same raw template");
        assert!(Arc::ptr_eq(&current.0.revision, &a.0.revision));
        checker
            .align_witness_contexts(&ca, &cx)
            .expect("canonical aliases remain live, not snapshotted");
        assert_eq!(checker.witness_head(&Type::con("A", "F")), changed);
        assert_eq!(checker.witness_head(&Type::con("X", "F")), changed);
        define(
            checker,
            vec![
                member("New", None),
                service(vec![Operation {
                    name: "tick".into(),
                    ty: Type::pure_fun(Type::Tuple(Vec::new()), Type::Tuple(Vec::new())),
                    class: OpClass::Tail,
                }]),
            ],
        );
        checker
            .align_witness_contexts(&ca, &cx)
            .expect("late owned footprint refreshed before alignment");
        assert!(checker.env.type_def("A", "F").is_none());
        assert!(checker.env.type_def("X", "F").is_none());
        assert!(checker.env.type_def("A", "New").is_some());
        assert_eq!(
            checker.env.type_def("A", "Clock.Impl"),
            checker.env.type_def("X", "Clock.Impl")
        );
        assert!(!checker.env.is_witness_manifest_alias("A", "F"));
        assert!(!checker.env.is_witness_manifest_alias("X", "F"));
    });
}

#[test]
fn paired_late_descendants_are_in_the_live_scope_support_footprint() {
    test_checker::with_checker(|checker| {
        define(checker, Vec::new());
        let a = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "A")
            .expect("A");
        let outer = checker
            .register_existential_scope(Scope::default(), &a)
            .expect("A scope");
        checker.store.enter_context(outer);
        define(
            checker,
            vec![R::Module {
                name: "Child".into(),
                binder: "child".into(),
                signature: Box::new(ModuleSignature::Structure {
                    requirements: vec![member("T", None)],
                }),
            }],
        );
        checker.env.define_type(
            "Dependent",
            TypeDef::of(
                Kind::Module,
                Form::ModuleContract {
                    requirements: vec![R::Value {
                        name: "value".into(),
                        ty: Type::con("A.Child", "T"),
                    }],
                },
            ),
        );
        checker
            .validate_existential_witness(&a)
            .expect("late child installed atomically");
        let b = checker
            .materialize_existential_witness(&Type::con("Probe", "Dependent"), "B")
            .expect("B");
        let context = checker
            .register_existential_scope(Scope::default(), &b)
            .expect("current owned child edges");
        assert_eq!(
            context.restrict(&context.ids().skip(1).collect()).len(),
            0,
            "a child added by current raw completion must belong to the coordinate support"
        );
    });
}

fn provided_atom_carrier(root: &Type, role: usize) -> Type {
    use crate::types::Atom;
    let Type::Meta(meta) = root else {
        panic!("scoped atom root")
    };
    assert!(meta.contextual().is_some(), "scoped atom root");
    let atom = Atom::Meta(meta.clone());
    let label = match role {
        0 => Elem::Capture(atom),
        1 => Elem::Error(atom),
        2 => Elem::Service(atom),
        _ => unreachable!("three declared atom roles"),
    };
    Type::Without {
        row: Box::new(Type::empty_row()),
        gone: vec![Elem::Provided(Box::new(label))],
    }
}

#[test]
fn paired_provided_atom_carriers_check_canonical_source_target_leases_before_semantic_equality() {
    use crate::types::module_contract::context::Embedding;
    test_checker::with_checker(|checker| {
        define(checker, vec![member("T", None)]);
        let a = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "A")
            .expect("A");
        let outer = checker
            .register_existential_scope(Scope::default(), &a)
            .expect("A scope");
        checker.store.enter_context(outer.clone());
        let roles = [Kind::Region, Kind::Type, Kind::Service];
        let outer_roots: Vec<_> = roles
            .iter()
            .map(|kind| checker.store.fresh_of(kind))
            .collect();
        let b = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "B")
            .expect("B");
        let scope = Scope::default();
        let trial = checker.store.checkpoint();
        let aborted = checker
            .register_existential_scope(scope, &b)
            .expect("trial scope");
        checker.store.enter_context(aborted.clone());
        let old: Vec<_> = roles
            .iter()
            .map(|kind| checker.store.fresh_of(kind))
            .collect();
        checker.store.rollback(trial);
        let fresh = checker
            .register_existential_scope(scope, &b)
            .expect("retry scope");
        checker.store.enter_context(fresh.clone());
        let current: Vec<_> = roles
            .iter()
            .map(|kind| checker.store.fresh_of(kind))
            .collect();
        let algebra =
            Embedding::align(&aborted, &fresh).expect("semantic shape is equal, not authority");
        assert_eq!(aborted, fresh);
        assert!(checker.align_witness_contexts(&aborted, &aborted).is_err());
        checker
            .align_witness_contexts(&fresh, &fresh)
            .expect("fresh proof use");
        for role in 0..roles.len() {
            let old = provided_atom_carrier(&old[role], role);
            let retargeted = algebra.transport(&old);
            assert_eq!(old, retargeted, "leases are not public type identity");
            assert!(
                checker
                    .validate_stamps(carrier::stamps_of_type(&old))
                    .is_err()
            );
            assert!(
                checker
                    .validate_stamps(carrier::stamps_of_type(&retargeted))
                    .is_err()
            );
            assert!(
                checker
                    .validate_stamps(carrier::stamps_of_type(&provided_atom_carrier(
                        &current[role],
                        role
                    )))
                    .is_ok()
            );
            assert!(
                checker
                    .validate_stamps(carrier::stamps_of_type(&provided_atom_carrier(
                        &outer_roots[role],
                        role
                    )))
                    .is_ok()
            );
        }
    });
}

#[test]
fn paired_namespace_reservation_preflights_independent_descendant_views() {
    test_checker::with_checker(|checker| {
        define(checker, vec![member("T", None)]);
        let contract = Type::con("Probe", "S");
        let a = checker
            .materialize_existential_witness(&contract, "A")
            .expect("A");
        assert!(
            checker
                .materialize_existential_witness(&contract, "A.Child")
                .is_err(),
            "a second independent issuer must not write inside the first exact owned footprint"
        );
        checker
            .validate_existential_witness(&a)
            .expect("failed child registration leaves original footprint authoritative");
        assert!(checker.env.type_def("A.Child", "T").is_none());
        checker
            .materialize_existential_witness(&contract, "A1")
            .expect("atomic path boundary, not a role/name-prefix ban");
    });
}

#[test]
fn paired_external_constructor_kind_rejects_aborted_target_before_resolution() {
    test_checker::with_checker(|checker| {
        define(checker, vec![member("T", None)]);
        let a = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "A")
            .expect("A");
        let outer = checker
            .register_existential_scope(Scope::default(), &a)
            .expect("outer");
        let root_kind = checker.kinds.fresh();
        checker
            .kinds
            .unify(&root_kind, &Kind::arrow(Kind::Type, Kind::Type))
            .expect("pretrial solution");
        let Some(root) = root_kind.legacy_meta_root() else {
            panic!("global pretrial root")
        };
        let kind = Kind::Meta(MetaOccurrence::kind(
            root,
            checker.store.witness_authority(),
            Embedding::align(&super::Context::default(), &outer).expect("outer target"),
        ));
        checker.store.enter_context(outer.clone());
        checker.kinds.enter_context(outer.clone());
        let b = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "B")
            .expect("B");
        let types = checker.store.checkpoint();
        let kinds = checker.kinds.checkpoint();
        let scope = Scope::default();
        let trial = checker
            .register_existential_scope(scope, &b)
            .expect("trial B");
        let stale = Embedding::align(&outer, &trial)
            .expect("weakening")
            .transport_kind_checked(&kind)
            .expect("kind transport");
        checker.store.rollback(types);
        checker.kinds.rollback(kinds);
        let wrapper = |kind| TypeDef {
            kind,
            form: Form::NOMINAL,
            variance: Vec::new(),
            native: None,
        };
        checker.env.define_type("Wrapper", wrapper(stale.clone()));
        checker.env.define_type(
            "Dependent",
            TypeDef::of(
                Kind::Module,
                Form::ModuleContract {
                    requirements: vec![R::Value {
                        name: "value".into(),
                        ty: Type::app(Type::con("Probe", "Wrapper"), Type::Tuple(Vec::new())),
                    }],
                },
            ),
        );
        let dependent = checker
            .materialize_existential_witness(&Type::con("Probe", "Dependent"), "D")
            .expect("dependent carrier");
        let failure = checker
            .register_existential_scope(Scope::default(), &dependent)
            .expect_err(
                "stale external kind target must be checked before resolving its solved root",
            );
        assert!(!failure.is_pending());
        checker.env.define_type("Wrapper", wrapper(kind.clone()));
        let valid = checker
            .register_existential_scope(Scope::default(), &dependent)
            .expect("pretrial outer kind remains valid");
        checker.env.define_type("Wrapper", wrapper(stale));
        assert!(checker.current_witness_context(&valid).is_err());
        assert!(checker.align_witness_contexts(&valid, &valid).is_err());
        checker.env.define_type("Wrapper", wrapper(kind.clone()));
        assert!(checker.current_witness_context(&valid).is_ok());
        let fresh = checker
            .register_existential_scope(scope, &b)
            .expect("fresh retry B");
        let current = Embedding::align(&outer, &fresh)
            .expect("fresh weakening")
            .transport_kind_checked(&kind)
            .expect("fresh kind");
        checker.env.define_type("Wrapper", wrapper(current));
        let fresh_dependent = checker
            .materialize_existential_witness(&Type::con("Probe", "Dependent"), "D2")
            .expect("independent fresh dependent");
        checker
            .register_existential_scope(Scope::default(), &fresh_dependent)
            .expect("fresh retry kind target valid");
    });
}

#[test]
fn paired_late_type_dependency_cannot_escape_a_thinned_telescope() {
    for scoped in [false, true] {
        test_checker::with_checker(|checker| {
            define(checker, vec![member("T", None)]);
            let a = checker
                .materialize_existential_witness(&Type::con("Probe", "S"), "A")
                .expect("A");
            let outer = checker
                .register_existential_scope(Scope::default(), &a)
                .expect("A scope");
            checker.store.enter_context(outer);
            let root = checker.store.fresh_of(&Kind::Type);
            checker.env.define_type(
                "Fixed",
                TypeDef::of(
                    Kind::Type,
                    Form::NOMINAL,
                ),
            );
            checker.env.define_type(
                "Dependent",
                TypeDef::of(
                    Kind::Module,
                    Form::ModuleContract {
                        requirements: vec![R::Value {
                            name: "value".into(),
                            ty: root.clone(),
                        }],
                    },
                ),
            );
            let b = checker
                .materialize_existential_witness(&Type::con("Probe", "Dependent"), "B")
                .expect("B");
            let context = checker
                .register_existential_scope(Scope::default(), &b)
                .expect("B scope");
            let thinned = context.restrict(&context.ids().skip(1).collect());
            assert_eq!(thinned.len(), 1);
            checker.store.set(
                root.meta_root().expect("root"),
                if scoped {
                    Type::con("A", "T")
                } else {
                    Type::con("Probe", "Fixed")
                },
            );
            assert!(
                checker.current_witness_context(&context).is_ok(),
                "full ordered prefix remains available"
            );
            assert_eq!(checker.current_witness_context(&thinned).is_err(), scoped);
            assert_eq!(
                checker.align_witness_contexts(&thinned, &thinned).is_err(),
                scoped
            );
        });
    }
}
