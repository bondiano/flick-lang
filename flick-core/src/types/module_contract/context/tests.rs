use super::*;

fn append(scopes: &mut Scopes, base: &Context, name: &str) -> Context {
    scopes.append(base, Type::con("Provider", "S"), name.into())
}

#[test]
fn identity_weakening_and_composition_preserve_order() {
    let mut scopes = Scopes::default();
    let a = append(&mut scopes, &Context::default(), "A");
    let ab = append(&mut scopes, &a, "B");
    let abc = append(&mut scopes, &ab, "C");
    let first = Embedding::weaken(&a, &ab).expect("valid ordered embedding");
    let second = Embedding::weaken(&ab, &abc).expect("valid ordered embedding");
    assert_eq!(
        first.compose(&second).expect("valid ordered embedding"),
        Embedding::weaken(&a, &abc).expect("valid ordered embedding")
    );
    assert_eq!(
        first
            .compose(&Embedding::identity(&ab))
            .expect("valid ordered embedding"),
        first
    );
    assert_eq!(
        Embedding::checked(ab.clone(), ab, vec![1, 0]),
        Err(Invalid::Order)
    );
}

#[test]
fn thinning_removes_dependents_and_never_reintroduces_excluded_coordinates() {
    let mut scopes = Scopes::default();
    let a = append(&mut scopes, &Context::default(), "A");
    let ab = scopes.append(&a, Type::con("A", "Contract"), "B".into());
    let abc = scopes.append(&ab, Type::con("B", "Contract"), "C".into());
    let keep = abc.ids().skip(1).collect();
    assert_eq!(abc.restrict(&keep), Context::default());
    let keep = abc.ids().take(2).collect();
    assert_eq!(abc.restrict(&keep), ab);
}

#[test]
fn alignment_checks_contracts_and_transports_ancestor_dependencies() {
    let mut scopes = Scopes::default();
    let a = append(&mut scopes, &Context::default(), "A");
    let ab = scopes.append(&a, Type::con("A", "Contract"), "B".into());
    let x = append(&mut scopes, &Context::default(), "X");
    let xy = scopes.append(&x, Type::con("X", "Contract"), "Y".into());
    let view = Embedding::align(&ab, &xy).expect("valid ordered embedding");
    assert_eq!(
        view.transport(&Type::con("A.Child", "Token")),
        Type::con("X.Child", "Token")
    );
    let wrong = scopes.append(&x, Type::con("Provider", "Different"), "Y2".into());
    assert_eq!(Embedding::align(&ab, &wrong), Err(Invalid::Contract));
}

#[test]
fn equalizer_thins_same_root_different_images() {
    let mut scopes = Scopes::default();
    let a = append(&mut scopes, &Context::default(), "A");
    let ab = append(&mut scopes, &a, "B");
    let left = Embedding::checked(a.clone(), ab.clone(), vec![0]).expect("valid ordered embedding");
    let right = Embedding::checked(a, ab.clone(), vec![1]).expect("valid ordered embedding");
    let equal = left.equalizer(&right).expect("valid ordered embedding");
    assert_eq!(equal.source, Context::default());
    assert_eq!(equal.target, ab);
    assert_eq!(
        left.equalizer(&left).expect("valid ordered embedding"),
        left
    );
}

#[test]
fn meet_excludes_ambient_coordinates_missing_from_either_operand() {
    let mut scopes = Scopes::default();
    let a = append(&mut scopes, &Context::default(), "A");
    let ab = append(&mut scopes, &a, "B");
    let abc = append(&mut scopes, &ab, "C");
    let left = Embedding::weaken(&a, &abc).expect("valid ordered embedding");
    let right = Embedding::weaken(&ab, &abc).expect("valid ordered embedding");
    let (to_left, to_right) = left.meet(&right).expect("valid ordered embedding");
    assert_eq!(to_left.source, a);
    assert_eq!(to_right.source, a);
    assert_eq!(
        to_left.compose(&left).expect("valid ordered embedding"),
        to_right.compose(&right).expect("valid ordered embedding")
    );
}

#[test]
fn transport_is_simultaneous_and_preserves_literal_record_keys() {
    use crate::types::ty::Binder;
    use crate::types::{Elem, Kind, Row};

    let mut scopes = Scopes::default();
    let a = append(&mut scopes, &Context::default(), "A");
    let ab = append(&mut scopes, &a, "B");
    let b = append(&mut scopes, &Context::default(), "B");
    let bc = append(&mut scopes, &b, "C");
    let view = Embedding::align(&ab, &bc).expect("valid ordered embedding");
    let ty = Type::Forall {
        vars: vec![Binder {
            name: "a".into(),
            kind: Kind::Named {
                module: "A".into(),
                name: "Mode".into(),
            },
        }],
        constraints: vec![Type::con("B", "Class")],
        body: Box::new(Type::Tuple(vec![
            Type::con("A.Child", "Token"),
            Type::Row(Row::closed(vec![(
                Elem::named("A.Service"),
                Type::con("A", "Service"),
            )])),
            Type::Record(Row::closed(vec![(
                Elem::named("A.field"),
                Type::con("B", "Token"),
            )])),
        ])),
    };
    let Type::Forall {
        vars,
        constraints,
        body,
    } = view.transport(&ty)
    else {
        panic!("forall")
    };
    assert_eq!(
        vars[0].kind,
        Kind::Named {
            module: "B".into(),
            name: "Mode".into()
        }
    );
    assert_eq!(constraints, vec![Type::con("C", "Class")]);
    let Type::Tuple(parts) = *body else {
        panic!("tuple")
    };
    assert_eq!(parts[0], Type::con("B.Child", "Token"));
    assert_eq!(
        parts[1],
        Type::Row(Row::closed(vec![(
            Elem::named("B.Service"),
            Type::con("B", "Service")
        )]))
    );
    assert_eq!(
        parts[2],
        Type::Record(Row::closed(vec![(
            Elem::named("A.field"),
            Type::con("C", "Token")
        )]))
    );
}

#[test]
fn kind_and_semantic_label_dependencies_are_not_dropped_as_literals() {
    use crate::types::ty::Binder;
    use crate::types::{Elem, Kind, Row};

    let mut scopes = Scopes::default();
    let a = append(&mut scopes, &Context::default(), "A");
    for contract in [
        Type::Forall {
            vars: vec![Binder {
                name: "a".into(),
                kind: Kind::Named {
                    module: "A".into(),
                    name: "Mode".into(),
                },
            }],
            constraints: Vec::new(),
            body: Box::new(Type::Var("a".into())),
        },
        Type::Without {
            row: Box::new(Type::empty_row()),
            gone: vec![Elem::Provided(Box::new(Elem::named("A.Service")))],
        },
        Type::Row(Row::closed(vec![(
            Elem::named("A.Service"),
            Type::empty_row(),
        )])),
    ] {
        let dependent = scopes.append(&a, contract, "Dependent".into());
        let keep = dependent.ids().skip(1).collect();
        assert_eq!(dependent.restrict(&keep), Context::default());
    }
    let literal = scopes.append(
        &a,
        Type::Record(Row::closed(vec![(
            Elem::named("A.field"),
            Type::empty_row(),
        )])),
        "Literal".into(),
    );
    let keep = literal.ids().skip(1).collect();
    assert_eq!(literal.restrict(&keep).len(), 1);
}

#[test]
fn matching_contracts_and_stamp_spelling_do_not_grant_cross_checker_authority() {
    let mut first = Scopes::default();
    let mut second = Scopes::default();
    let left = append(&mut first, &Context::default(), "Same.Spelling");
    let right = append(&mut second, &Context::default(), "Same.Spelling");
    assert_ne!(
        left.ids().collect::<Vec<_>>(),
        right.ids().collect::<Vec<_>>()
    );
    assert_eq!(Embedding::align(&left, &right), Err(Invalid::Authority));
    assert_eq!(Embedding::weaken(&left, &right), Err(Invalid::Domain));
}

#[test]
fn lexical_order_is_not_the_numeric_nonce_order() {
    let preallocated_inner = Scope::default().identity();
    let preallocated_outer = Scope::default().identity();
    assert!(
        preallocated_inner < preallocated_outer,
        "allocation counter only"
    );
    let mut scopes = Scopes::default();
    let outer = scopes.append_at(
        &Context::default(),
        preallocated_outer,
        Type::con("Provider", "S"),
        "Outer".into(),
    );
    let context = scopes.append_at(
        &outer,
        preallocated_inner,
        Type::con("Outer", "Contract"),
        "Inner".into(),
    );
    let lexical = vec![preallocated_outer, preallocated_inner];
    assert_eq!(context.ids().collect::<Vec<_>>(), lexical);
    assert_eq!(
        context
            .restrict(&context.ids().collect())
            .ids()
            .collect::<Vec<_>>(),
        lexical
    );
    let view = Embedding::identity(&context);
    assert_eq!(view.image, vec![0, 1]);
    assert_eq!(
        view.equalizer(&view)
            .expect("equalizer")
            .source
            .ids()
            .collect::<Vec<_>>(),
        lexical
    );
    let (left, right) = view.meet(&view).expect("common context");
    assert_eq!(left.source.ids().collect::<Vec<_>>(), lexical);
    assert_eq!(right.source.ids().collect::<Vec<_>>(), lexical);
}

#[test]
fn shifted_ordered_view_has_an_inverse_only_on_its_closed_image() {
    let mut scopes = Scopes::default();
    let root = append(&mut scopes, &Context::default(), "Root");
    let ambient = append(&mut scopes, &Context::default(), "Ambient");
    let target = append(&mut scopes, &ambient, "Shifted");
    let view = Embedding::checked(root, target, vec![1]).expect("shifted embedding");
    let inverse = view.inverse_on_image();
    assert_eq!(inverse.source.len(), 1);
    assert_eq!(inverse.source.0[0].witness, "Shifted");
    assert_eq!(
        inverse.transport(&view.transport(&Type::con("Root.Child", "Token"))),
        Type::con("Root.Child", "Token")
    );
    assert_eq!(
        view.compose(&inverse),
        Err(Invalid::Domain),
        "the ambient codomain is not the inverse's image domain"
    );
}

#[test]
fn transport_composes_occurrence_views_and_preserves_nested_bound_coordinates() {
    use crate::types::ty::MetaOccurrence;

    let mut scopes = Scopes::default();
    let source = append(&mut scopes, &Context::default(), "A");
    let target = append(&mut scopes, &Context::default(), "X");
    let bound = Scope::default();
    let nested = scopes.append_at(
        &source,
        bound.identity(),
        Type::con("A", "Contract"),
        "Inner".into(),
    );
    let view = Embedding::align(&source, &target).expect("alpha view");
    let occurrence = MetaOccurrence::new(9, scopes.owner(), Embedding::identity(&nested));
    let ty = Type::Exists {
        scope: bound,
        binder: "K".into(),
        contract: Box::new(Type::con("A", "Contract")),
        body: Box::new(Type::Meta(occurrence)),
    };
    let Type::Exists {
        scope,
        contract,
        body,
        ..
    } = view.transport(&ty)
    else {
        panic!("exists")
    };
    assert_eq!(
        scope.identity(),
        bound.identity(),
        "logical transport is not template insertion"
    );
    assert_eq!(*contract, Type::con("X", "Contract"));
    let Type::Meta(meta) = *body else {
        panic!("contextual occurrence")
    };
    assert_eq!(meta.root(), 9);
    assert_eq!(
        meta.contextual()
            .expect("contextual test occurrence")
            .renaming()
            .mapped(source.ids().next().expect("A")),
        target.ids().next()
    );
    assert_eq!(
        meta.contextual()
            .expect("contextual test occurrence")
            .renaming()
            .mapped(bound.identity()),
        Some(bound.identity())
    );
    assert_eq!(
        meta.contextual()
            .expect("contextual test occurrence")
            .renaming()
            .target
            .0[1]
            .contract,
        Type::con("X", "Contract")
    );
}

#[test]
fn a_record_tail_keeps_field_labels_literal_while_rebasing_their_types() {
    use crate::types::{Elem, Row};

    let mut scopes = Scopes::default();
    let source = append(&mut scopes, &Context::default(), "W");
    let target = append(&mut scopes, &Context::default(), "X");
    let view = Embedding::align(&source, &target).expect("alpha view");
    let ty = Type::Record(Row {
        entries: Vec::new(),
        tail: Some(Box::new(Type::Row(Row::closed(vec![(
            Elem::named("W.E"),
            Type::con("W", "E"),
        )])))),
        ..Row::default()
    });
    let Type::Record(record) = view.transport(&ty) else {
        panic!("record")
    };
    let Type::Row(tail) = *record.tail.expect("field row tail") else {
        panic!("field row")
    };
    assert_eq!(
        tail.entries[0].0,
        Elem::named("W.E"),
        "field name is not a semantic witness identity"
    );
    assert_eq!(tail.entries[0].1, Type::con("X", "E"));
}

#[test]
fn checked_embedding_rejects_foreign_ambient_tail_even_with_empty_source() {
    let mut local = Scopes::default();
    let a = append(&mut local, &Context::default(), "A");
    let mut foreign = Scopes::default();
    let b = append(&mut foreign, &Context::default(), "B");
    let mixed = Context(vec![a.0[0].clone(), b.0[0].clone()]);
    assert_eq!(
        Embedding::checked(a, mixed.clone(), vec![0]),
        Err(Invalid::Authority)
    );
    assert_eq!(
        Embedding::checked(mixed.clone(), mixed.clone(), vec![0, 1]),
        Err(Invalid::Authority)
    );
    assert_eq!(
        Embedding::checked(Context::default(), mixed, Vec::new()),
        Err(Invalid::Authority)
    );
    let uniform = append(&mut local, &Context::default(), "C");
    assert!(Embedding::weaken(&Context::default(), &uniform).is_ok());
    assert!(Embedding::weaken(&Context::default(), &Context::default()).is_ok());
}
