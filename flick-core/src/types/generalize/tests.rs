use super::*;
use crate::types::ty::{Effects, Row};

fn row(tail: Type) -> Row {
    Row {
        tail: Some(Box::new(tail)),
        ..Row::default()
    }
}

fn kinds(ty: &Type, params: &dyn Fn(&Type) -> Vec<Kind>) -> Vec<Kind> {
    let evidence = kinds_of(ty, params);
    metas_of(ty)
        .into_iter()
        .map(|root| evidence.get(&root).cloned().unwrap_or(Kind::Type))
        .collect()
}

#[test]
fn evidence_keeps_first_root_order_and_last_kind_in_preorder() {
    let [first, second, third, fourth] = [9, 5, 2, 8].map(Type::meta);
    let ty = Type::Tuple(vec![
        first.clone(),
        Type::Fun {
            param: Box::new(second.clone()),
            effects: Some(Box::new(Effects::new(
                Type::Union(Box::new(first.clone()), Box::new(second.clone())),
                Type::Union(Box::new(second.clone()), Box::new(third.clone())),
            ))),
            result: Box::new(third),
        },
        Type::Bounded {
            ty: Box::new(fourth),
            captures: Box::new(Type::Row(row(first))),
        },
        Type::Record(row(second)),
    ]);
    assert_eq!(metas_of(&ty), [9, 5, 2, 8]);
    assert_eq!(
        kinds(&ty, &|_| vec![]),
        [
            Kind::CaptureRow,
            Kind::Row(Box::new(Kind::Type)),
            Kind::ErrorRow,
            Kind::Type,
        ]
    );
}

#[test]
fn evidence_from_application_prefixes_and_shared_effect_rows_is_stable() {
    let [shared, nested, unresolved, plain] = [5, 9, 2, 8].map(Type::meta);
    let row = Type::Without {
        row: Box::new(Type::Row(row(nested.clone()))),
        gone: Vec::new(),
    };
    let head = Type::con("Demo", "Constructor");
    let partial = Type::app(head.clone(), row);
    let app = Type::apps(head.clone(), [shared.clone(), nested.clone(), unresolved]);
    let ty = Type::Tuple(vec![
        Type::Fun {
            param: Box::new(shared.clone()),
            effects: Some(Box::new(Effects::new(shared.clone(), shared))),
            result: Box::new(plain),
        },
        app,
        partial,
    ]);
    let params = |t: &Type| {
        if t == &head {
            vec![Kind::ServiceRow, Kind::ErrorRow, Kind::meta(0)]
        } else {
            Vec::new()
        }
    };
    assert_eq!(
        kinds(&ty, &params),
        [Kind::ServiceRow, Kind::Type, Kind::ServiceRow, Kind::Type]
    );
}

#[test]
fn generalized_binders_and_written_signature_follow_appearance_order() {
    use crate::types::unify::test_checker::with_checker;

    with_checker(|checker| {
        checker.store.level += 1;
        let a = checker.store.fresh();
        let row_tail = checker.store.fresh();
        let b = checker.store.fresh();
        checker.store.level -= 1;
        let ty = Type::Tuple(vec![b.clone(), Type::Record(row(row_tail)), a, b]);
        let expected = Type::Tuple(vec![
            Type::Var("a".into()),
            Type::Record(row(Type::Var("b".into()))),
            Type::Var("c".into()),
            Type::Var("a".into()),
        ]);
        assert_eq!(checker.signature_of(&ty), expected);
        assert_eq!(
            checker.generalize(&ty, (0, 0)),
            Type::Forall {
                vars: vec![
                    Binder {
                        name: "a".into(),
                        kind: Kind::Type,
                    },
                    Binder {
                        name: "b".into(),
                        kind: Kind::Row(Box::new(Kind::Type)),
                    },
                    Binder {
                        name: "c".into(),
                        kind: Kind::Type,
                    },
                ],
                constraints: vec![],
                body: Box::new(expected),
            }
        );
    });
}

#[test]
fn type_evidence_resets_a_previous_row_kind() {
    let root = Type::meta(7);
    let constructor = Type::con("Demo", "Plain");
    let ty = Type::Tuple(vec![
        Type::Record(row(root.clone())),
        Type::app(constructor.clone(), root),
    ]);
    let parameter = |head: &Type| {
        if head == &constructor {
            vec![Kind::Type]
        } else {
            Vec::new()
        }
    };
    assert_eq!(kinds(&ty, &parameter), [Kind::Type]);
}
