#[test]
fn paired_registration_rejects_foreign_issued_and_stale_actual_without_scope_writes() {
    test_checker::with_checker(|checker| {
        define(checker, vec![member("T", None)]);
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("issued pair");
        test_checker::with_checker(|other| {
            define(other, vec![member("T", None)]);
            let scope = Scope::default();
            assert!(other.register_existential_scope(scope, &witness).is_err());
            assert!(other.env.type_def("W", "T").is_none());
            let own = other
                .materialize_existential_witness(&Type::con("Probe", "S"), "W")
                .expect("own exact pair");
            other
                .register_existential_scope(scope, &own)
                .expect("failed foreign attempt left no scope or namespace writes");
            assert!(other.store.witness_context().len() == 0);
        });
        let scope = Scope::default();
        let original = checker
            .env
            .type_def("W", "T")
            .cloned()
            .expect("original owned slot");
        let module = mem::replace(&mut checker.env.module, "W".into());
        checker.env.define_type(
            "T",
            TypeDef {
                kind: Kind::Symbol,
                form: Form::NOMINAL,
                variance: Vec::new(),
            },
        );
        checker.env.module = module;
        assert!(checker.register_existential_scope(scope, &witness).is_err());
        assert!(checker.store.witness_context().len() == 0);
        assert_eq!(
            checker
                .env
                .type_def("W", "T")
                .expect("no overwritten conflict")
                .kind,
            Kind::Symbol
        );
        let module = mem::replace(&mut checker.env.module, "W".into());
        checker.env.define_type("T", original);
        checker.env.module = module;
        checker
            .register_existential_scope(scope, &witness)
            .expect("same binder succeeds after restoring authority; no partial scope write");
    });
}

#[test]
fn paired_actual_dependency_walk_distinguishes_record_tail_keys_from_error_labels() {
    use crate::types::Effects;
    test_checker::with_checker(|checker| {
        define(checker, vec![member("E", None)]);
        let parent = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "A")
            .expect("parent pair");
        let outer = checker
            .register_existential_scope(Scope::default(), &parent)
            .expect("registered parent");
        checker.store.enter_context(outer.clone());
        let raw = Type::Row(Row::closed(vec![(
            Elem::named("A.E"),
            Type::Tuple(Vec::new()),
        )]));
        let fields = Type::Record(Row {
            entries: Vec::new(),
            tail: Some(Box::new(raw.clone())),
            written: Written::default(),
        });
        let errors = Type::Fun {
            param: Box::new(Type::Tuple(Vec::new())),
            effects: Some(Box::new(Effects::new(Type::empty_row(), raw))),
            result: Box::new(Type::Tuple(Vec::new())),
        };
        for (name, ty, requires_outer) in [("Fields", fields, false), ("Errors", errors, true)] {
            let contract_name = format!("{name}Contract");
            checker.env.define_type(
                &contract_name,
                TypeDef {
                    kind: Kind::Module,
                    form: Form::ModuleContract {
                        requirements: vec![R::Value {
                            name: "value".into(),
                            ty,
                        }],
                    },
                    variance: Vec::new(),
                },
            );
            let pair = checker
                .materialize_existential_witness(&Type::con("Probe", &contract_name), name)
                .expect("real named contract materialization");
            let context = checker
                .register_existential_scope(Scope::default(), &pair)
                .expect("kinded actual registration");
            let keep = context.ids().skip(1).collect();
            assert_eq!(context.restrict(&keep).len(), usize::from(!requires_outer));
        }
    });
}

#[test]
fn paired_registration_rollback_does_not_revive_an_old_context_on_binder_retry() {
    test_checker::with_checker(|checker| {
        define(checker, vec![member("T", None)]);
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("issued pair");
        let scope = Scope::default();
        let trial = checker.store.checkpoint();
        let aborted = checker
            .register_existential_scope(scope, &witness)
            .expect("trial scope");
        checker.store.rollback(trial);
        assert!(checker.current_witness_context(&aborted).is_err());
        let retried = checker
            .register_existential_scope(scope, &witness)
            .expect("retry binder");
        assert!(checker.current_witness_context(&retried).is_ok());
        assert!(
            checker.current_witness_context(&aborted).is_err(),
            "a new registration of the same lexical binder must not revive speculative evidence"
        );
    });
}

#[test]
fn paired_copy_and_bound_transport_retain_actual_meta_carriers_and_live_support() {
    use crate::types::module_contract::context::{Context, Embedding};
    test_checker::with_checker(|checker| {
        define(checker, vec![member("Token", None)]);
        let a = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "A")
            .expect("A");
        let ca = checker
            .register_existential_scope(Scope::default(), &a)
            .expect("A scope");
        checker.store.enter_context(ca.clone());
        let root = checker.store.fresh_of(&Kind::Type);
        checker.env.define_type(
            "Dependent",
            TypeDef {
                kind: Kind::Module,
                form: Form::ModuleContract {
                    requirements: vec![R::Value {
                        name: "value".into(),
                        ty: root.clone(),
                    }],
                },
                variance: Vec::new(),
            },
        );
        let b = checker
            .materialize_existential_witness(&Type::con("Probe", "Dependent"), "B")
            .expect("B");
        let bound = Scope::default();
        let cab = checker
            .register_existential_scope(bound, &b)
            .expect("B scope");
        checker.store.enter_context(cab.clone());
        let inner = checker.store.fresh_of(&Kind::Type);
        checker.store.enter_context(Context::default());
        let x = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "X")
            .expect("X");
        let cx = checker
            .register_existential_scope(Scope::default(), &x)
            .expect("X scope");
        let outer = Embedding::align(&ca, &cx).expect("same real contract");
        let closed = Type::Exists {
            scope: bound,
            binder: "K".into(),
            contract: Box::new(Type::con("Probe", "Dependent")),
            body: Box::new(inner),
        };
        let projected = outer
            .project_type(&closed, &Kind::Type, &checker.read_types(), &[], &[])
            .expect("bound carrier transport");
        let Type::Exists { body, .. } = projected else {
            panic!("exists")
        };
        let Type::Meta(meta) = *body else {
            panic!("contextual body")
        };
        let transported = meta
            .contextual()
            .expect("contextual test occurrence")
            .renaming()
            .target()
            .clone();
        checker
            .store
            .set(root.meta_root().expect("root"), Type::con("A", "Token"));
        checker
            .align_witness_contexts(&cab, &cab)
            .expect("identical current actuals must align after solving");
        let current = checker
            .current_witness_context(&transported)
            .expect("live transported actual");
        let pairs: Vec<_> = current.issued_origins().cloned().collect();
        let actuals = current
            .actuals(&pairs, &checker.read_types().observing())
            .expect("paired actuals");
        let actual = &actuals[1].2;
        let value = actual
            .find(Namespace::Value, "value")
            .expect("retained actual value");
        assert_eq!(value.scheme, Some(Type::con("X", "Token")));
        let only_bound = current.ids().skip(1).collect();
        assert_eq!(
            current.restrict(&only_bound).len(),
            0,
            "live outer dependency retained"
        );
        let fresh = Scope::default().identity();
        let copied = Embedding::identity(&cab).copy_bound(ca.ids().next().expect("A id"), fresh);
        let copied = checker
            .current_witness_context(copied.target())
            .expect("copied paired actual");
        assert_eq!(copied.ids().next(), Some(fresh));
        assert_eq!(copied.restrict(&copied.ids().skip(1).collect()).len(), 0);
    });
}

#[test]
fn paired_kind_carrier_cannot_reuse_an_aborted_registration_lease() {
    test_checker::with_checker(|checker| {
        define(checker, vec![member("T", None)]);
        let witness = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "W")
            .expect("W");
        let outer_pair = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "Outer")
            .expect("outer pair");
        let outer = checker
            .register_existential_scope(Scope::default(), &outer_pair)
            .expect("outer scope");
        checker.store.enter_context(outer.clone());
        checker.kinds.enter_context(outer.clone());
        let outer_type = checker.store.fresh_of(&Kind::Type);
        let outer_kind = checker.kinds.fresh();
        let scope = Scope::default();
        let types = checker.store.checkpoint();
        let kinds = checker.kinds.checkpoint();
        let context = checker
            .register_existential_scope(scope, &witness)
            .expect("trial scope");
        checker.store.enter_context(context.clone());
        checker.kinds.enter_context(context.clone());
        let old_type = checker.store.fresh_of(&Kind::Type);
        let old_kind = checker.kinds.fresh();
        checker.store.rollback(types);
        checker.kinds.rollback(kinds);
        let fresh = checker
            .register_existential_scope(scope, &witness)
            .expect("retry scope");
        checker.store.enter_context(fresh.clone());
        checker.kinds.enter_context(fresh.clone());
        let fresh_type = checker.store.fresh_of(&Kind::Type);
        let fresh_kind = checker.kinds.fresh();
        assert!(checker.current_witness_context(&outer).is_ok());
        assert!(checker.store.valid_occurrence(&outer_type));
        assert!(checker.kinds.valid_occurrence(&outer_kind));
        assert_eq!(
            context, fresh,
            "registration evidence must not enter structural equality"
        );
        assert!(checker.store.valid_occurrence(&fresh_type));
        assert!(checker.kinds.valid_occurrence(&fresh_kind));
        assert!(!checker.store.valid_occurrence(&old_type));
        let retarget = Embedding::align(&context, &fresh)
            .expect("pure algebra is not current registry authority");
        let retargeted_type = retarget.transport(&old_type);
        assert_eq!(
            old_type, retargeted_type,
            "proof lease is not public type equality"
        );
        assert!(checker.current_witness_context(retarget.source()).is_err());
        assert!(
            !checker.store.valid_occurrence(&retargeted_type),
            "a fresh target cannot bless the aborted canonical/source registration"
        );
        let retargeted_kind = retarget
            .transport_kind_checked(&old_kind)
            .expect("kind algebra");
        let Kind::Meta(retargeted_kind) = retargeted_kind else {
            panic!("kind view")
        };
        let thinned = retargeted_kind.with_renaming(
            retargeted_kind
                .contextual()
                .expect("contextual test occurrence")
                .renaming()
                .restrict(&BTreeSet::default()),
        );
        assert!(
            checker
                .validate_stamps(vec![Stamp::KindView(thinned)])
                .is_err(),
            "thinning cannot bless the aborted canonical kind context"
        );
        assert!(
            checker
                .validate_stamps(vec![Stamp::KindView(retargeted_kind)])
                .is_err()
        );
        let Kind::Meta(meta) = old_kind else {
            panic!("scoped kind root")
        };
        assert!(meta.contextual().is_some(), "scoped kind proof carrier");
        assert!(
            checker
                .validate_stamps(vec![Stamp::KindView(meta)])
                .is_err(),
            "kind proof use must check the current registration, not owner/ScopeId alone"
        );
    });
}

#[test]
fn paired_late_kind_evidence_retries_without_default_or_stale_dependency_cache() {
    use crate::types::module_contract::context::Context;
    test_checker::with_checker(|checker| {
        define(checker, vec![member("E", None)]);
        let a = checker
            .materialize_existential_witness(&Type::con("Probe", "S"), "A")
            .expect("A");
        let outer = checker
            .register_existential_scope(Scope::default(), &a)
            .expect("A scope");
        checker.store.enter_context(outer.clone());
        let role = checker.kinds.fresh();
        checker.env.define_type(
            "Wrapper",
            TypeDef {
                kind: Kind::arrow(role.clone(), Kind::Type),
                form: Form::NOMINAL,
                variance: Vec::new(),
            },
        );
        checker.env.define_type(
            "Dependent",
            TypeDef {
                kind: Kind::Module,
                form: Form::ModuleContract {
                    requirements: vec![R::Value {
                        name: "value".into(),
                        ty: Type::app(
                            Type::con("Probe", "Wrapper"),
                            Type::Row(Row::closed(vec![(
                                Elem::named("A.E"),
                                Type::Tuple(Vec::new()),
                            )])),
                        ),
                    }],
                },
                variance: Vec::new(),
            },
        );
        let pair = checker
            .materialize_existential_witness(&Type::con("Probe", "Dependent"), "B")
            .expect("B");
        let scope = Scope::default();
        let pending = checker
            .register_existential_scope(scope, &pair)
            .expect_err("row role unresolved");
        assert!(pending.is_pending());
        assert_eq!(
            pending.to_string(),
            "witness view needs more inference evidence"
        );
        assert_eq!(
            checker.kinds.resolve_deep(&role),
            role,
            "read must not default kind"
        );
        let types = checker.store.checkpoint();
        let kinds = checker.kinds.checkpoint();
        checker
            .kinds
            .unify(&role, &Kind::ErrorRow)
            .expect("actual semantic role");
        let errors = checker
            .register_existential_scope(scope, &pair)
            .expect("ready after kind solving");
        assert_eq!(
            errors.restrict(&errors.ids().skip(1).collect()),
            Context::default()
        );
        checker.store.rollback(types);
        checker.kinds.rollback(kinds);
        assert!(checker.current_witness_context(&errors).is_err());
        assert!(
            checker.current_witness_context(&outer).is_ok(),
            "outer lease survives rollback"
        );
        checker
            .kinds
            .unify(&role, &Kind::Row(Box::new(Kind::Type)))
            .expect("actual literal role");
        let fields = checker
            .register_existential_scope(scope, &pair)
            .expect("ready retry after rollback");
        assert_eq!(fields.restrict(&fields.ids().skip(1).collect()).len(), 1);
        assert!(Arc::ptr_eq(
            &pair.0.revision,
            &checker
                .validate_existential_witness(&pair)
                .expect("live pair")
                .0
                .revision
        ));
        assert!(
            checker.current_witness_context(&errors).is_err(),
            "fresh retry cannot resurrect old lease"
        );
    });
}
