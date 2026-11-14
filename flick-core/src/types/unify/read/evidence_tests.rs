#[cfg(test)]
mod read_tests {
    use super::*;
    use crate::types::TypeDef;

    #[test]
    fn an_independent_layout_is_not_a_named_existential_coordinate_proof() {
        use crate::types::ModuleRequirement;
        test_checker::with_checker(|checker| {
            let requirements = vec![ModuleRequirement::Type {
                name: "E".into(),
                params: Vec::new(),
                manifest: None,
            }];
            checker.env.define_type(
                "S",
                TypeDef {
                    kind: Kind::Module,
                    form: Form::ModuleContract {
                        requirements: requirements.clone(),
                    },
                },
            );
            let issued = checker
                .materialize_existential_witness(&Type::con("Probe", "S"), "W")
                .expect("actual named view");
            let mut forged = issued.view().clone();
            forged.exports[0]
                .definition
                .as_mut()
                .expect("type header")
                .kind = Kind::Symbol;
            let forged = issued.replace_actual_for_test(forged);
            assert!(
                checker.validate_existential_witness(&forged).is_err(),
                "an independent Interface cannot certify the actual named contract view"
            );
        });
    }

    #[test]
    fn semantic_projection_sorts_labels_without_detaching_their_values() {
        use crate::types::module_contract::context::{Context, Embedding};
        use crate::types::ty::Scope;
        use crate::types::{ModuleRequirement, ModuleSignature};
        test_checker::with_checker(|checker| {
            let requirements = vec![ModuleRequirement::Type {
                name: "E".into(),
                params: Vec::new(),
                manifest: None,
            }];
            checker.env.define_type(
                "S",
                TypeDef {
                    kind: Kind::Module,
                    form: Form::ModuleContract {
                        requirements: requirements.clone(),
                    },
                },
            );
            checker.env.define_type(
                "GlobalE",
                TypeDef {
                    kind: Kind::Type,
                    form: Form::NOMINAL,
                },
            );
            let signature = ModuleSignature::Structure { requirements };
            let source_view = checker
                .materialize_witness_view(&signature, "W")
                .expect("checked type-only view");
            let source = checker
                .store
                .register_scope(Scope::default(), Type::con("Probe", "S"), &source_view)
                .expect("owned scope");
            checker.store.enter_context(source.clone());
            let root = checker.store.fresh_of(&Kind::ErrorRow);
            checker.store.set(
                root.meta_root().expect("root"),
                Type::Row(Row::closed(vec![
                    (Elem::named("W.E"), Type::con("W", "E")),
                    (Elem::named("Probe.GlobalE"), Type::con("Probe", "GlobalE")),
                ])),
            );
            checker.store.enter_context(Context::default());
            let target_view = checker
                .materialize_witness_view(&signature, "A")
                .expect("checked type-only view");
            let target = checker
                .store
                .register_scope(Scope::default(), Type::con("Probe", "S"), &target_view)
                .expect("owned scope");
            let occurrence = Embedding::align(&source, &target)
                .expect("alpha view")
                .transport(&root);
            let expected = Type::Row(Row::closed(vec![
                (Elem::named("A.E"), Type::con("A", "E")),
                (Elem::named("Probe.GlobalE"), Type::con("Probe", "GlobalE")),
            ]));
            assert_eq!(
                checker
                    .read_types()
                    .head(&occurrence, &Kind::ErrorRow)
                    .expect("projected head")
                    .as_ref(),
                &expected
            );
        });
    }

    #[test]
    fn solved_contextual_roots_keep_the_enclosing_forall_kind_environment() {
        use crate::iface::Interface;
        use crate::types::module_contract::context::{Context, Embedding};
        use crate::types::ty::Scope;
        test_checker::with_checker(|checker| {
            let register = |store: &mut Store, module: &str| {
                store
                    .register_scope(
                        Scope::default(),
                        Type::con("Provider", "S"),
                        &Interface {
                            module: module.into(),
                            ..Interface::default()
                        },
                    )
                    .expect("owned scope")
            };
            let source = register(&mut checker.store, "W");
            checker.store.enter_context(source.clone());
            let kind = Kind::arrow(Kind::Row(Box::new(Kind::Type)), Kind::Type);
            checker.cx.tyvars.push(TyVar {
                name: "f".into(),
                ty: Type::Var("f".into()),
                kind: kind.clone(),
            });
            let root = checker.store.fresh_of(&Kind::Type);
            checker.store.set(
                root.meta_root().expect("root"),
                Type::app(
                    Type::Var("f".into()),
                    Type::Row(Row::closed(vec![(Elem::named("W.E"), Type::con("W", "E"))])),
                ),
            );
            checker.cx.tyvars.pop();
            checker.store.enter_context(Context::default());
            let target = register(&mut checker.store, "X");
            let view = Embedding::align(&source, &target).expect("alpha view");
            let ty = Type::Forall {
                vars: vec![Binder {
                    name: "f".into(),
                    kind,
                }],
                constraints: Vec::new(),
                body: Box::new(view.transport(&root)),
            };
            let Type::Forall { vars, .. } = &ty else {
                panic!("forall")
            };
            let expected = Type::Forall {
                vars: vars.clone(),
                constraints: Vec::new(),
                body: Box::new(Type::app(
                    Type::Var("f".into()),
                    Type::Row(Row::closed(vec![(Elem::named("W.E"), Type::con("X", "E"))])),
                )),
            };
            assert_eq!(checker.read_types().zonk(&ty, &Kind::Type), Ok(expected));
        });
    }

    #[test]
    fn frontend_allocations_retain_expected_kinds_without_defaulting() {
        use crate::syntax::cst::Node;
        use crate::syntax::kind::SyntaxKind as K;
        use crate::types::convert::Free;
        fn name_type(node: &Node) -> Option<&Node> {
            if node.kind == K::NameType {
                Some(node)
            } else {
                node.nodes().find_map(name_type)
            }
        }
        test_checker::with_source(
            "module Probe exposing ()\ntype alias T = f\n",
            |checker, tree| {
                checker.hole_ok = true;
                let span = checker.span(tree);
                let (hole, _) = checker.hole(span, &Kind::ErrorRow);
                let (todo, _) = checker.todo_type(span, &Kind::ServiceRow);
                let expected = Kind::arrow(Kind::Row(Box::new(Kind::Type)), Kind::Type);
                let free = checker.ty(
                    name_type(tree).expect("parsed name type"),
                    &expected,
                    Free::Fresh,
                );
                let stage = checker.open_stage();
                let read = checker.read_types();
                assert_eq!(
                    vec![
                        read.head_kind(&hole, &[]),
                        read.head_kind(&todo, &[]),
                        read.head_kind(&free, &[]),
                        read.head_kind(&stage.services, &[])
                    ],
                    vec![
                        Ok(Kind::ErrorRow),
                        Ok(Kind::ServiceRow),
                        Ok(expected),
                        Ok(Kind::ServiceRow)
                    ]
                );
            },
        );
    }

    #[test]
    fn closed_existential_application_reads_the_actual_contract_parameter_kind() {
        use crate::types::ModuleRequirement;
        use crate::types::ty::Scope;
        test_checker::with_checker(|checker| {
            checker.env.define_type(
                "S",
                TypeDef {
                    kind: Kind::Module,
                    form: Form::ModuleContract {
                        requirements: vec![ModuleRequirement::Type {
                            name: "F".into(),
                            params: vec![Binder {
                                name: "r".into(),
                                kind: Kind::Row(Box::new(Kind::Type)),
                            }],
                            manifest: None,
                        }],
                    },
                },
            );
            let ty = Type::Exists {
                scope: Scope::default(),
                binder: "K".into(),
                contract: Box::new(Type::con("Probe", "S")),
                body: Box::new(Type::app(
                    Type::con("$exists:K", "F"),
                    Type::Row(Row::closed(vec![(
                        Elem::named("W.E"),
                        Type::con("Probe", "Value"),
                    )])),
                )),
            };
            let before = checker.env.own_types().count();
            assert_eq!(checker.read_types().zonk(&ty, &Kind::Type), Ok(ty));
            assert_eq!(
                checker.env.own_types().count(),
                before,
                "readonly kind evidence installs nothing"
            );
        });
    }

    #[test]
    fn semantic_omission_atoms_follow_solutions_but_literal_ones_do_not() {
        use crate::types::Atom;
        test_checker::with_checker(|checker| {
            let root = checker.store.fresh_of(&Kind::Type);
            let id = root.meta_root().expect("root");
            checker.store.set(id, Type::con("Probe", "E"));
            let ty = Type::Without {
                row: Box::new(Type::Var("r".into())),
                gone: vec![Elem::Provided(Box::new(Elem::Error(Atom::Meta(id))))],
            };
            let expected = Type::Without {
                row: Box::new(Type::Var("r".into())),
                gone: vec![Elem::Provided(Box::new(Elem::named("Probe.E")))],
            };
            assert_eq!(
                checker.store.zonk(&ty),
                expected,
                "existing semantic baseline"
            );
            assert_eq!(
                checker.read_types().zonk(&ty, &Kind::ErrorRow),
                Ok(expected)
            );
            assert_eq!(
                checker
                    .read_types()
                    .zonk(&ty, &Kind::Row(Box::new(Kind::Type))),
                Ok(ty),
                "literal position is not an atom occurrence"
            );
        });
    }

    #[test]
    fn specialized_root_allocators_retain_their_actual_kinds() {
        test_checker::with_checker(|checker| {
            let capture = checker.store.fresh_capture_row();
            let errors = checker.store.fresh_error_row();
            let number = checker.store.fresh_numeric(false);
            let read = checker.read_types();
            assert_eq!(read.head_kind(&capture, &[]), Ok(Kind::CaptureRow));
            assert_eq!(read.head_kind(&errors, &[]), Ok(Kind::ErrorRow));
            assert_eq!(read.head_kind(&number, &[]), Ok(Kind::Type));
        });
    }

    #[test]
    fn zonk_keeps_the_existing_row_normal_forms() {
        test_checker::with_checker(|checker| {
            let entry = |name: &str| (Elem::named(name), Type::con("Global", "Value"));
            let ty = Type::Record(Row {
                entries: vec![entry("a")],
                tail: Some(Box::new(Type::Row(Row::closed(vec![
                    entry("b"),
                    entry("a"),
                ])))),
                ..Row::default()
            });
            let expected = Type::Record(Row::closed(vec![entry("a"), entry("b")]));
            assert_eq!(
                checker.store.zonk(&ty),
                expected,
                "existing closed baseline"
            );
            assert_eq!(checker.read_types().zonk(&ty, &Kind::Type), Ok(expected));
            let omitted = Type::Without {
                row: Box::new(Type::Union(
                    Box::new(Type::Row(Row::closed(vec![entry("a")]))),
                    Box::new(Type::Row(Row::closed(vec![entry("b")]))),
                )),
                gone: vec![Elem::named("a")],
            };
            let expected = Type::Row(Row::closed(vec![entry("b")]));
            assert_eq!(checker.store.zonk(&omitted), expected);
            assert_eq!(
                checker
                    .read_types()
                    .zonk(&omitted, &Kind::Row(Box::new(Kind::Type))),
                Ok(expected)
            );
        });
    }

    #[test]
    fn contextual_record_tail_projects_types_but_not_field_keys() {
        use crate::iface::Interface;
        use crate::types::module_contract::context::{Context, Embedding};
        use crate::types::ty::Scope;

        test_checker::with_checker(|checker| {
            let register = |store: &mut Store, module: &str| {
                store
                    .register_scope(
                        Scope::default(),
                        Type::con("Provider", "S"),
                        &Interface {
                            module: module.into(),
                            ..Interface::default()
                        },
                    )
                    .expect("owned witness")
            };
            let source = register(&mut checker.store, "W");
            checker.store.enter_context(source.clone());
            let root = checker.store.fresh_of(&Kind::Type);
            let tail = checker.store.fresh_of(&Kind::Row(Box::new(Kind::Type)));
            checker.store.enter_context(Context::default());
            let target = register(&mut checker.store, "X");
            let view = Embedding::align(&source, &target).expect("alpha view");
            let occurrence = view.transport(&root);
            checker.store.set(
                root.meta_root().expect("root"),
                Type::Record(Row {
                    tail: Some(Box::new(tail.clone())),
                    ..Row::default()
                }),
            );
            checker.store.set(
                tail.meta_root().expect("tail"),
                Type::Row(Row::closed(vec![(Elem::named("W.E"), Type::con("W", "E"))])),
            );
            let expected = Type::Record(Row {
                tail: Some(Box::new(Type::Row(Row::closed(vec![(
                    Elem::named("W.E"),
                    Type::con("X", "E"),
                )])))),
                ..Row::default()
            });
            let before = format!("{:?}", checker.store.metas);
            let read = checker.read_types();
            let head = read.head(&occurrence, &Kind::Type).expect("head");
            let Type::Record(record) = head.as_ref() else {
                panic!("record head")
            };
            assert!(
                matches!(record.tail.as_deref(), Some(Type::MetaView(_))),
                "head stays lazy below its root"
            );
            let Type::Record(expected_row) = &expected else {
                panic!("record")
            };
            let expected_zonk = Type::Record(flatten_row(expected_row.clone(), false));
            assert_eq!(read.zonk(&occurrence, &Kind::Type), Ok(expected_zonk));
            assert!(matches!(
                read.head(&expected, &Kind::Type),
                Ok(Cow::Borrowed(_))
            ));
            assert_eq!(format!("{:?}", checker.store.metas), before, "read purity");
        });
    }

    #[test]
    fn declared_kinds_and_real_dictionary_authority_supply_read_evidence() {
        test_checker::with_checker(|checker| {
            checker.env.define_type(
                "Ordinary$Dict",
                TypeDef {
                    kind: Kind::Symbol,
                    form: Form::Nominal {
                        literals: Vec::new(),
                        hidden: false,
                    },
                },
            );
            checker.env.define_type(
                "C",
                TypeDef {
                    kind: Kind::arrow(Kind::Type, Kind::Constraint),
                    form: Form::Class {
                        params: vec!["a".into()],
                        supers: Vec::new(),
                        fundeps: Vec::new(),
                        methods: Vec::new(),
                        defaults: Vec::new(),
                        default_sigs: Vec::new(),
                        derive: false,
                    },
                },
            );
            let read = checker.read_types();
            assert_eq!(
                read.head_kind(&Type::con("Probe", "C$Dict"), &[]),
                Ok(Kind::arrow(Kind::Type, Kind::Type)),
                "real Class authority supplies its nominal dictionary kind"
            );
            assert_eq!(
                read.head_kind(&Type::con("Probe", "Ordinary$Dict"), &[]),
                Ok(Kind::Symbol)
            );
            assert!(
                read.head_kind(&Type::con("Probe", "Missing$Dict"), &[])
                    .is_err()
            );
            let lexical = vec![Binder {
                name: "f".into(),
                kind: Kind::arrow(Kind::ErrorRow, Kind::Type),
            }];
            assert_eq!(
                read.head_kind(&Type::Var("f".into()), &lexical),
                Ok(lexical[0].kind.clone())
            );
        });
    }

    #[test]
    fn raw_row_shape_does_not_guess_a_kind_and_later_kind_solution_is_observed() {
        test_checker::with_checker(|checker| {
            let kind = checker.kinds.fresh();
            let root = checker.store.fresh_of(&kind);
            assert_eq!(checker.read_types().head_kind(&root, &[]), Ok(kind.clone()));
            assert!(
                checker
                    .read_types()
                    .head_kind(
                        &Type::Row(Row::closed(vec![(Elem::named("W.E"), Type::con("W", "E"))])),
                        &[]
                    )
                    .is_err()
            );
            checker
                .kinds
                .unify(&kind, &Kind::Row(Box::new(Kind::Type)))
                .expect("later kind evidence");
            assert_eq!(
                checker.read_types().head_kind(&root, &[]),
                Ok(Kind::Row(Box::new(Kind::Type)))
            );
        });
    }
}
