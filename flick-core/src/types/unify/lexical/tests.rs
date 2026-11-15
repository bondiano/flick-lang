use super::*;
use crate::types::ty::Scope;
use crate::types::{Atom, ModuleRequirement, TypeDef};

fn contract(checker: &mut Checker<'_>) {
    checker.env.define_type(
        "S",
        TypeDef {
            kind: Kind::Module,
            form: Form::ModuleContract {
                requirements: vec![ModuleRequirement::Type {
                    name: "Token".into(),
                    params: Vec::new(),
                    manifest: None,
                }],
            },
        },
    );
}

fn package(checker: &mut Checker<'_>, binder: &str, witness: &str) -> (Type, Type, LexicalScope) {
    let contract = Type::con("Probe", "S");
    checker
        .materialize_existential_witness(&contract, witness)
        .expect("issued witness");
    let closed = Scope::default();
    checker.with_lexical_witness(witness, |checker, opened| {
        let hole = checker.fresh();
        let body = lexical::close_type(&hole, opened, binder, closed);
        (
            Type::Exists {
                scope: closed,
                binder: binder.into(),
                contract: Box::new(contract.clone()),
                body: Box::new(body),
            },
            hole,
            opened.clone(),
        )
    })
}

fn token_package(binder: &str) -> Type {
    Type::Exists {
        scope: Scope::default(),
        binder: binder.into(),
        contract: Box::new(Type::con("Probe", "S")),
        body: Box::new(Type::con(&format!("$exists:{binder}"), "Token")),
    }
}

#[test]
fn unsolved_aliases_transport_future_solutions_between_independent_binders() {
    test_checker::with_checker(|checker| {
        contract(checker);
        let (a, _, _) = package(checker, "A", "First");
        let (b, _, _) = package(checker, "B", "Second");
        checker
            .unify(&a, &b)
            .expect("alias unsolved existential holes");
        checker
            .unify(&b, &token_package("C"))
            .expect("solve through second binder");
        for (package, binder) in [(&a, "A"), (&b, "B")] {
            let Type::Exists { body, .. } = checker.zonk(package) else {
                panic!("package")
            };
            assert_eq!(*body, Type::con(&format!("$exists:{binder}"), "Token"));
        }
        for (package, namespace) in [(&a, "OpenedA"), (&b, "OpenedB")] {
            let Type::Exists {
                scope,
                binder,
                body,
                ..
            } = package
            else {
                panic!("package")
            };
            checker.with_lexical_witness(namespace, |checker, opened| {
                assert_eq!(
                    checker.zonk(&lexical::open_type(body, binder, *scope, opened)),
                    Type::con(namespace, "Token")
                );
            });
        }
    });
}

#[test]
fn successful_aliases_intersect_permissions_in_both_directions() {
    for reverse in [false, true] {
        test_checker::with_checker(|checker| {
            contract(checker);
            let outside = checker.fresh();
            let (_, inside, scope) = package(checker, "K", "Aliased");
            let (left, right) = if reverse {
                (&inside, &outside)
            } else {
                (&outside, &inside)
            };
            checker
                .unify(left, right)
                .expect("alias shared root permissions");
            for handle in [&outside, &inside] {
                assert!(
                    checker
                        .unify(handle, &Type::con(&scope.namespace, "Token"))
                        .is_err()
                );
            }
            checker
                .unify(&inside, &Type::con("Prelude", "Int"))
                .expect("closed alias solution");
            assert_eq!(checker.zonk(&outside), Type::con("Prelude", "Int"));
            assert_eq!(checker.zonk(&inside), Type::con("Prelude", "Int"));
        });
    }
}

#[test]
fn failed_alias_trials_restore_permissions_in_both_directions() {
    for reverse in [false, true] {
        test_checker::with_checker(|checker| {
            contract(checker);
            let outside = checker.fresh();
            let (_, inside, scope) = package(checker, "K", "Inside");
            let (left, right) = if reverse {
                (inside.clone(), outside.clone())
            } else {
                (outside.clone(), inside.clone())
            };
            assert!(
                checker
                    .unify(
                        &Type::Tuple(vec![left, Type::con("Prelude", "Int")]),
                        &Type::Tuple(vec![right, Type::con("Prelude", "Bool")])
                    )
                    .is_err()
            );
            assert_eq!(checker.zonk(&outside), outside);
            checker
                .unify(&inside, &Type::con(&scope.namespace, "Token"))
                .expect("aborted pruning restored");
            assert!(
                checker
                    .unify(&outside, &Type::con(&scope.namespace, "Token"))
                    .is_err()
            );
        });
    }
}

#[test]
fn a_whole_closed_package_keeps_its_locally_bound_hole_permission() {
    test_checker::with_checker(|checker| {
        contract(checker);
        let outside = checker.fresh();
        let (package, _, _) = package(checker, "K", "Closed");
        checker
            .unify(&outside, &package)
            .expect("whole closed package");
        checker
            .unify(&package, &token_package("Other"))
            .expect("local hidden solution");
        let Type::Exists { body, .. } = checker.zonk(&outside) else {
            panic!("closed package")
        };
        assert_eq!(*body, Type::con("$exists:K", "Token"));
    });
}

#[test]
fn copied_shadowed_binders_keep_unsolved_occurrence_targets_distinct() {
    test_checker::with_checker(|checker| {
        contract(checker);
        let (_, hole, opened) = package(checker, "Unused", "Outer");
        let inner = Type::Exists {
            scope: Scope::default(),
            binder: "K".into(),
            contract: Box::new(Type::con("Probe", "S")),
            body: Box::new(hole.clone()),
        };
        let closed = Scope::default();
        let body = lexical::close_type(&inner, &opened, "K", closed);
        let Type::Exists { binder, .. } = &body else {
            panic!("inner package")
        };
        assert_ne!(
            binder, "K",
            "an unresolved view can later refer to the outer binder"
        );
        let package = Type::Exists {
            scope: closed,
            binder: "K".into(),
            contract: Box::new(Type::con("Probe", "S")),
            body: Box::new(body),
        };
        let copied = package.substitute(&[]);
        let Type::Exists { scope, body, .. } = &copied else {
            panic!("outer package")
        };
        let mut target = None;
        lexical::visit_type_metas(body, &mut |meta, kind| {
            if !kind {
                target = Some(meta.lexical().expect("lexical view").view[0].target.id);
            }
        });
        assert_eq!(target, Some(scope.identity()));
        assert_ne!(scope.identity(), closed.identity());
        checker
            .unify(&hole, &Type::con("Outer", "Token"))
            .expect("later hidden solution");
        let Type::Exists { body, .. } = checker.zonk(&copied) else {
            panic!("package")
        };
        let Type::Exists { binder, body, .. } = *body else {
            panic!("nested package")
        };
        assert_ne!(binder, "K");
        assert_eq!(*body, Type::con("$exists:K", "Token"));
    });
}

#[test]
fn capture_avoidance_keeps_a_free_unsolved_view_out_of_a_new_binder_name() {
    test_checker::with_checker(|checker| {
        contract(checker);
        let (_, outer, opened) = package(checker, "Unused", "OuterFresh");
        let (beta, beta_hole, beta_opened) = package(checker, "K$Closed0", "BetaFresh");
        let Type::Exists {
            scope: beta_scope,
            body: beta_view,
            ..
        } = beta
        else {
            panic!("beta package")
        };
        let inner = Type::Exists {
            scope: Scope::default(),
            binder: "K".into(),
            contract: Box::new(Type::con("Probe", "S")),
            body: Box::new(Type::Tuple(vec![outer.clone(), *beta_view])),
        };
        let outer_scope = Scope::default();
        let body = lexical::close_type(&inner, &opened, "K", outer_scope);
        let Type::Exists { binder, .. } = &body else {
            panic!("inner")
        };
        assert_ne!(
            binder, "K$Closed0",
            "the candidate is a free unresolved occurrence namespace"
        );
        let closed = Type::Exists {
            scope: beta_scope,
            binder: "K$Closed0".into(),
            contract: Box::new(Type::con("Probe", "S")),
            body: Box::new(Type::Exists {
                scope: outer_scope,
                binder: "K".into(),
                contract: Box::new(Type::con("Probe", "S")),
                body: Box::new(body),
            }),
        };
        checker
            .unify(&outer, &Type::con(&opened.namespace, "Token"))
            .expect("outer solution");
        checker
            .unify(&beta_hole, &Type::con(&beta_opened.namespace, "Token"))
            .expect("later free solution");
        let Type::Exists { body, .. } = checker.zonk(&closed) else {
            panic!("beta")
        };
        let Type::Exists { body, .. } = *body else {
            panic!("outer")
        };
        let Type::Exists { body, .. } = *body else {
            panic!("inner")
        };
        assert_eq!(
            *body,
            Type::Tuple(vec![
                Type::con("$exists:K", "Token"),
                Type::con("$exists:K$Closed0", "Token")
            ])
        );
    });
}

#[test]
fn semantic_omission_atoms_cannot_hide_a_witness_but_field_strings_stay_literal() {
    test_checker::with_checker(|checker| {
        contract(checker);
        let (_, inside, scope) = package(checker, "K", "Hidden");
        checker
            .unify(&inside, &Type::con(&scope.namespace, "Token"))
            .expect("inside solution");
        let Type::Meta(meta) = inside else {
            panic!("hole")
        };
        let outside = checker.fresh();
        let omitted = Type::Without {
            row: Box::new(Type::Var("r".into())),
            gone: vec![Elem::Capture(Atom::Meta(meta))],
        };
        assert!(checker.unify(&outside, &omitted).is_err());
        let outside = checker.fresh();
        checker
            .unify(
                &outside,
                &Type::Record(Row::closed(vec![(
                    Elem::named("Hidden.Token"),
                    Type::con("Prelude", "Int"),
                )])),
            )
            .expect("literal record field");
    });
}

#[test]
fn lexical_kinds_close_reopen_and_refuse_outer_capture() {
    test_checker::with_checker(|checker| {
        let outside = checker.kinds.fresh();
        checker.with_lexical_witness("KindWitness", |checker, opened| {
            let named = Kind::Named {
                module: opened.namespace.clone(),
                name: "Mode".into(),
            };
            assert!(checker.kinds.unify(&outside, &named).is_err());
            let local = checker.kinds.fresh();
            assert!(checker.kinds.valid_occurrence(&local));
            assert!(checker.kinds.valid_occurrence_in(&local, |context| {
                checker.store.valid_witness_context(context)
            }));
            checker.kinds.unify(&local, &named).expect("local kind");
            let closed = LexicalScope {
                id: Scope::default().identity(),
                namespace: "$exists:K".into(),
            };
            let view = lexical::transport_kind(&local, &[(opened.clone(), closed.clone())]);
            assert_eq!(
                checker.kinds.resolve_deep(&view),
                Kind::Named {
                    module: "$exists:K".into(),
                    name: "Mode".into()
                }
            );
            assert_eq!(
                checker
                    .kinds
                    .resolve_deep_in(&view, &|context| checker
                        .store
                        .valid_witness_context(context))
                    .expect("checked lexical kind"),
                Kind::Named {
                    module: "$exists:K".into(),
                    name: "Mode".into()
                }
            );
            let next = LexicalScope {
                id: Scope::default().identity(),
                namespace: "Next".into(),
            };
            let reopened = lexical::transport_kind(&view, &[(closed, next)]);
            assert_eq!(
                checker.kinds.resolve_deep(&reopened),
                Kind::Named {
                    module: "Next".into(),
                    name: "Mode".into()
                }
            );
        });
    });
}

#[test]
fn dropping_a_lexical_view_does_not_grant_canonical_read_authority() {
    test_checker::with_checker(|checker| {
        contract(checker);
        let (_, hole, scope) = package(checker, "K", "Secret");
        checker
            .unify(&hole, &Type::con(&scope.namespace, "Token"))
            .expect("local binding");
        let raw = Type::meta(hole.meta_root().expect("root"));
        assert_eq!(checker.store.resolve(&raw), raw);
        assert!(!checker.store.valid_occurrence(&raw));
        checker.with_lexical_witness("KindSecret", |checker, opened| {
            let kind = checker.kinds.fresh();
            checker
                .kinds
                .unify(
                    &kind,
                    &Kind::Named {
                        module: opened.namespace.clone(),
                        name: "Mode".into(),
                    },
                )
                .expect("kind binding");
            let Kind::Meta(meta) = kind else {
                panic!("kind meta")
            };
            let raw = Kind::meta(meta.root());
            assert_eq!(checker.kinds.resolve(&raw), raw);
            assert!(!checker.kinds.valid_occurrence(&raw));
        });
    });
}

#[test]
fn same_root_view_pruning_checks_its_declared_kind_and_narrows_kind_metas() {
    test_checker::with_checker(|checker| {
        checker.with_lexical_witness("Kinds", |checker, opened| {
            let named = Kind::Named {
                module: opened.namespace.clone(),
                name: "Mode".into(),
            };
            let variable = checker.store.fresh_of(&named);
            let other = LexicalScope {
                id: Scope::default().identity(),
                namespace: "OtherView".into(),
            };
            checker.store.lexical_scopes.push(other.clone());
            checker.kinds.register_lexical(other.clone());
            let shifted = lexical::transport_type(&variable, &[(opened.clone(), other.clone())]);
            assert!(checker.unify(&variable, &shifted).is_err());
            let inferred = checker.kinds.fresh();
            let variable = checker.store.fresh_of(&inferred);
            let shifted = lexical::transport_type(&variable, &[(opened.clone(), other)]);
            checker
                .unify(&variable, &shifted)
                .expect("permission intersection for unresolved kind");
            assert!(checker.kinds.unify(&inferred, &named).is_err());
            checker
                .kinds
                .unify(&inferred, &Kind::Type)
                .expect("closed kind remains inferable");
        });
    });
}
