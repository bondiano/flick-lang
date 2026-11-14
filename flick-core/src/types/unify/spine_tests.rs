use super::Store;
use crate::types::Type;

#[test]
fn bound_alias_constructor_heads_remain_visible_in_application_spines() {
    for name in ["Opaque", "Reveal"] {
        let mut store = Store::default();
        let variable = store.fresh();
        let Type::Meta(meta) = variable else {
            unreachable!()
        };
        let constructor = Type::con("Test", name);
        store.set(meta, constructor.clone());
        let argument = Type::Var("a".to_owned());
        let application = Type::app(variable, argument.clone());
        let (head, args) = store.spine(&application);
        assert_eq!(
            head.as_ref(),
            &constructor,
            "top-only resolution hides {name}"
        );
        assert_eq!(
            args.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
            vec![&argument]
        );
    }
}

#[test]
fn bound_partial_constructor_heads_prepend_their_existing_arguments() {
    let mut store = Store::default();
    let variable = store.fresh();
    let Type::Meta(meta) = variable else {
        unreachable!()
    };
    let constructor = Type::con("Test", "Task");
    let first = Type::Var("scope".to_owned());
    let second = Type::Var("result".to_owned());
    store.set(meta, Type::app(constructor.clone(), first.clone()));
    let application = Type::app(variable, second.clone());
    let (head, args) = store.spine(&application);
    assert_eq!(head.as_ref(), &constructor);
    assert_eq!(
        args.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
        vec![&first, &second]
    );
}

mod occurrence_baseline {
    use super::super::{Context, Embedding, Kind, MetaState, Mismatch, Store, test_checker};
    use crate::iface::Interface;
    use crate::types::lang::Lang;
    use crate::types::ty::without;
    use crate::types::{Atom, Elem, MetaOccurrence, Row, Scope, Type};

    fn register(store: &mut Store, module: &str) -> Context {
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
    }

    fn contextual_variable(store: &mut Store, shifted: bool, kind: &Kind) -> Type {
        let source = register(store, "A");
        store.enter_context(source.clone());
        let root = store.fresh_of(kind);
        if !shifted {
            return root;
        }
        store.enter_context(Context::default());
        let target = register(store, "X");
        Embedding::align(&source, &target)
            .expect("alpha view")
            .transport(&root)
    }

    fn contextual_without_refuses(left: bool, shifted: bool) {
        // Every invocation owns a disposable checker: a baseline panic must
        // never leave a checker that another assertion goes on to reuse.
        test_checker::with_checker(|checker| {
            let root = contextual_variable(&mut checker.store, shifted, &Kind::ErrorRow);
            let omitted = without(root, &[Elem::named("gone")]);
            let normalized = checker.store.normalize(&omitted);
            assert!(normalized.entries.is_empty());
            assert_eq!(normalized.tails, vec![omitted.clone()]);
            let before = format!("{:?}", checker.store);
            let empty = Type::empty_row();
            let result = if left {
                checker.unify_row_types(&omitted, &empty)
            } else {
                checker.unify_row_types(&empty, &omitted)
            };
            assert!(matches!(result, Err(Mismatch::Types(..))), "{result:?}");
            assert_eq!(format!("{:?}", checker.store), before);
        });
    }

    #[test]
    fn contextual_without_identity_left() {
        contextual_without_refuses(true, false);
    }

    #[test]
    fn contextual_without_identity_right() {
        contextual_without_refuses(false, false);
    }

    #[test]
    fn contextual_without_shifted_left() {
        contextual_without_refuses(true, true);
    }

    #[test]
    fn contextual_without_shifted_right() {
        contextual_without_refuses(false, true);
    }

    #[test]
    fn legacy_without_empty_row_binds_in_both_directions() {
        for left in [false, true] {
            test_checker::with_checker(|checker| {
                let root = checker.store.fresh_error_row();
                let omitted = without(root.clone(), &[Elem::named("gone")]);
                let empty = Type::empty_row();
                let result = if left {
                    checker.unify_row_types(&omitted, &empty)
                } else {
                    checker.unify_row_types(&empty, &omitted)
                };
                result.expect("legacy omitted tail can be empty");
                assert_eq!(checker.store.resolve(&root), empty);
            });
        }
    }

    #[test]
    fn contextual_without_preserves_earlier_missing_label_refusal() {
        for left in [false, true] {
            test_checker::with_checker(|checker| {
                let root = contextual_variable(&mut checker.store, false, &Kind::ErrorRow);
                let omitted = without(root, &[Elem::named("gone")]);
                let row = Type::Row(Row {
                    entries: vec![(Elem::named("kept"), Type::lang(Lang::Int))],
                    tail: Some(Box::new(omitted)),
                    ..Row::default()
                });
                let before = format!("{:?}", checker.store);
                let empty = Type::empty_row();
                let result = if left {
                    checker.unify_row_types(&row, &empty)
                } else {
                    checker.unify_row_types(&empty, &row)
                };
                assert!(
                    matches!(result, Err(Mismatch::Label { label, left: actual })
                    if label == Elem::named("kept") && actual == left)
                );
                assert_eq!(format!("{:?}", checker.store), before);
            });
        }
    }

    fn assert_marks(store: &Store, root: u32, expected: (bool, bool, bool, bool)) {
        assert_eq!(
            (
                store.is_capture_row(root),
                store.is_error_row(root),
                store.is_numeric(root),
                store.is_fractional(root),
            ),
            expected,
        );
    }

    #[test]
    fn fresh_canonical_initialization_preserves_kinds_marks_and_levels() {
        for contextual in [false, true] {
            let mut store = Store {
                level: 3,
                ..Store::default()
            };
            let context = if contextual {
                register(&mut store, "A")
            } else {
                Context::default()
            };
            store.enter_context(context.clone());
            let allocations = [
                (
                    store.fresh_capture_row(),
                    Kind::CaptureRow,
                    true,
                    false,
                    false,
                    false,
                ),
                (
                    store.fresh_error_row(),
                    Kind::ErrorRow,
                    false,
                    true,
                    false,
                    false,
                ),
                (
                    store.fresh_numeric(false),
                    Kind::Type,
                    false,
                    false,
                    true,
                    false,
                ),
                (
                    store.fresh_numeric(true),
                    Kind::Type,
                    false,
                    false,
                    true,
                    true,
                ),
                (
                    store.fresh_of(&Kind::ServiceRow),
                    Kind::ServiceRow,
                    false,
                    false,
                    false,
                    false,
                ),
                (
                    store.fresh_of(&Kind::CaptureRow),
                    Kind::CaptureRow,
                    true,
                    false,
                    false,
                    false,
                ),
                (
                    store.fresh_of(&Kind::ErrorRow),
                    Kind::ErrorRow,
                    false,
                    true,
                    false,
                    false,
                ),
                (
                    store.fresh_of(&Kind::Row(Box::new(Kind::Type))),
                    Kind::Row(Box::new(Kind::Type)),
                    false,
                    false,
                    false,
                    false,
                ),
            ];
            store.level = 9;
            for (ty, kind, capture, error, numeric, fractional) in allocations {
                let root = ty.meta_root().expect("fresh root");
                assert_eq!(matches!(ty, Type::MetaView(_)), contextual);
                assert!(store.valid_occurrence(&ty));
                assert_eq!(store.resolve(&ty), ty);
                assert_eq!(store.meta_level(root), Some(3));
                assert_marks(&store, root, (capture, error, numeric, fractional));
                let metadata = &store.meta_contexts[root as usize];
                assert_eq!(metadata.canonical, context);
                assert_eq!(metadata.permitted, context);
                assert_eq!(metadata.kind, Some(kind.clone()));

                let beside = store.fresh_beside(root);
                let next = beside.meta_root().expect("fresh beside root");
                assert_ne!(next, root);
                assert_eq!(matches!(beside, Type::MetaView(_)), contextual);
                assert!(store.valid_occurrence(&beside));
                assert_eq!(store.meta_level(next), Some(3));
                assert_eq!(store.meta_contexts[next as usize].kind, Some(kind));
                assert_eq!(store.meta_contexts[next as usize].permitted, context);
                // mark_like copies capture/error marks, never numeric marks.
                assert_marks(&store, next, (capture, error, false, false));
            }
        }
    }

    #[test]
    fn legacy_numeric_restrictions_propagate_to_legacy_target() {
        let mut store = Store::default();
        let numeric = store.fresh_numeric(true).meta_root().expect("numeric root");
        let target = store.fresh();
        let root = target.meta_root().expect("legacy target");
        assert!(store.numeric_fits(numeric, &target));
        assert!(store.is_numeric(root));
        assert!(store.is_fractional(root));
    }

    #[test]
    fn contextual_targets_do_not_gain_numeric_marks_or_level_lowering() {
        for shifted in [false, true] {
            let mut store = Store::default();
            let numeric = store.fresh_numeric(true).meta_root().expect("numeric root");
            store.level = 4;
            let target = contextual_variable(&mut store, shifted, &Kind::Type);
            let root = target.meta_root().expect("contextual target");
            let before = format!("{store:?}");
            assert!(!store.numeric_fits(numeric, &target));
            store.adjust(&target, 0).expect("no skolem");
            assert_eq!(format!("{store:?}"), before);
            assert_eq!(store.meta_level(root), Some(4));
            assert!(!store.is_numeric(root));
            assert!(!store.is_fractional(root));
            assert!(matches!(store.metas[root as usize], MetaState::Open { .. }));
            store.level = 0;
            assert!(store.deep_metas(&target).is_empty());
        }
    }

    #[test]
    fn contextual_types_do_not_gain_binding_in_either_direction() {
        for shifted in [false, true] {
            for left in [false, true] {
                test_checker::with_checker(|checker| {
                    let occurrence = contextual_variable(&mut checker.store, shifted, &Kind::Type);
                    let canonical = |store: &Store| {
                        format!(
                            "{:?}",
                            (
                                &store.metas,
                                &store.meta_contexts,
                                &store.active_context,
                                &store.capture_rows,
                                &store.error_rows,
                                &store.numeric,
                                &store.fractional,
                                &store.scopes,
                            )
                        )
                    };
                    let before = canonical(&checker.store);
                    let concrete = Type::lang(Lang::Int);
                    let result = if left {
                        checker.unify(&occurrence, &concrete)
                    } else {
                        checker.unify(&concrete, &occurrence)
                    };
                    assert!(result.is_err(), "contextual binding remains unsupported");
                    assert_eq!(canonical(&checker.store), before);
                    assert_eq!(checker.store.resolve(&occurrence), occurrence);
                });
            }
        }
    }

    #[test]
    fn contextual_kinds_do_not_gain_binding_or_defaulting() {
        use crate::types::kinds::KindStore;
        use crate::types::module_contract::context::Scopes;
        for shifted in [false, true] {
            let mut scopes = Scopes::default();
            let source = scopes.append(&Context::default(), Type::con("Provider", "S"), "A".into());
            let target = scopes.append(&Context::default(), Type::con("Provider", "S"), "X".into());
            let mut kinds = KindStore::for_witnesses(scopes.owner());
            kinds.enter_context(source.clone());
            let original = kinds.fresh();
            let occurrence = if shifted {
                Embedding::align(&source, &target)
                    .expect("alpha view")
                    .transport_kind_checked(&original)
                    .expect("kind occurrence transport")
            } else {
                original.clone()
            };
            assert!(kinds.unify(&occurrence, &Kind::Type).is_err());
            assert_eq!(kinds.resolve(&original), original);
            assert_eq!(kinds.zonk(&occurrence, true), occurrence);
            assert_eq!(kinds.resolve(&original), original);
            kinds
                .unify(&occurrence, &occurrence)
                .expect("same-value equality is preserved");
            assert_eq!(kinds.resolve(&original), original);
        }
    }

    #[test]
    fn legacy_kinds_still_bind_and_default() {
        use crate::types::kinds::KindStore;
        let mut kinds = KindStore::default();
        let bound = kinds.fresh();
        kinds.unify(&bound, &Kind::Symbol).expect("legacy binding");
        assert_eq!(kinds.resolve(&bound), Kind::Symbol);
        let defaulted = kinds.fresh();
        assert_eq!(kinds.zonk(&defaulted, true), Kind::Type);
        assert_eq!(kinds.resolve(&defaulted), Kind::Type);
    }

    #[test]
    fn inference_meta_layout_measurements() {
        use std::mem::{align_of, size_of};
        eprintln!(
            "inference-meta-layout Type={}/{} Kind={}/{} Atom={}/{} MetaOccurrence={}/{}",
            size_of::<Type>(),
            align_of::<Type>(),
            size_of::<Kind>(),
            align_of::<Kind>(),
            size_of::<Atom>(),
            align_of::<Atom>(),
            size_of::<MetaOccurrence>(),
            align_of::<MetaOccurrence>(),
        );
    }
}
