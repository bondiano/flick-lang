use super::*;
use crate::iface::{Decl, Entity, Export};
use crate::types::Kind;

fn string() -> Type {
    Type::con("Prelude", "String")
}

fn key() -> Type {
    Type::Var("Key".into())
}

fn contract(manifest: Option<Type>) -> TypeDef {
    TypeDef {
        kind: Kind::Module,
        form: Form::ModuleContract {
            requirements: vec![
                ModuleRequirement::Type {
                    name: "Key".into(),
                    params: Vec::new(),
                    manifest,
                },
                ModuleRequirement::Value {
                    name: "equal".into(),
                    ty: Type::pure_fun(key(), Type::pure_fun(key(), Type::con("Prelude", "Bool"))),
                },
            ],
        },
        variance: Vec::new(),
        native: None,
    }
}

fn export(
    namespace: Namespace,
    name: &str,
    definition: Option<TypeDef>,
    scheme: Option<Type>,
) -> Export {
    Export {
        entity: Entity {
            module: "Keys".into(),
            namespace,
            name: name.into(),
        },
        decl: if namespace == Namespace::Type {
            Decl::Alias
        } else {
            Decl::Let
        },
        parent: None,
        scheme,
        definition,
        blocked: None,
        at: None,
        via: Vec::new(),
    }
}

fn actual(key_type: Type) -> Interface {
    Interface {
        module: "Keys".into(),
        exports: vec![
            export(
                Namespace::Type,
                "Key",
                Some(TypeDef {
                    kind: Kind::Type,
                    form: Form::Alias {
                        params: Vec::new(),
                        body: key_type.clone(),
                        opaque: false,
                    },
                    variance: Vec::new(),
                    native: None,
                }),
                None,
            ),
            export(
                Namespace::Value,
                "equal",
                None,
                Some(Type::pure_fun(
                    key_type.clone(),
                    Type::pure_fun(key_type, Type::con("Prelude", "Bool")),
                )),
            ),
            export(Namespace::Value, "extra", None, Some(string())),
        ],
        ..Interface::default()
    }
}

#[test]
fn abstract_type_matches_a_manifest_implementation_and_ignores_extra_exports() {
    assert_eq!(check(&contract(None), &actual(string())), []);
}

#[test]
fn manifest_type_requires_the_same_equality() {
    let mismatches = check(
        &contract(Some(string())),
        &actual(Type::con("Prelude", "Int")),
    );
    assert!(
        mismatches.contains(&Mismatch::TypeEquality("Key".into())),
        "{mismatches:?}"
    );
}

#[test]
fn a_missing_value_and_a_wrong_value_type_are_distinct() {
    let mut actual = actual(string());
    actual.exports.retain(|e| e.entity.name != "equal");
    assert!(check(&contract(None), &actual).contains(&Mismatch::Missing("equal".into())));
    actual.exports.push(export(
        Namespace::Value,
        "equal",
        None,
        Some(Type::pure_fun(string(), string())),
    ));
    assert!(check(&contract(None), &actual).contains(&Mismatch::ValueType("equal".into())));
}

#[test]
fn manifest_class_requires_nominal_identity() {
    let contract = TypeDef {
        kind: Kind::Module,
        form: Form::ModuleContract {
            requirements: vec![ModuleRequirement::Nominal {
                name: "Eq".into(),
                category: super::super::env::ModuleNominalKind::Class,
                identity: Some(Entity {
                    module: "Prelude".into(),
                    namespace: Namespace::Type,
                    name: "Eq".into(),
                }),
                definition: None,
                methods: Vec::new(),
            }],
        },
        variance: Vec::new(),
        native: None,
    };
    let actual = Interface {
        module: "Keys".into(),
        exports: vec![Export {
            entity: Entity {
                module: "Other".into(),
                namespace: Namespace::Type,
                name: "Eq".into(),
            },
            decl: Decl::Class,
            parent: None,
            scheme: None,
            definition: None,
            blocked: None,
            at: None,
            via: Vec::new(),
        }],
        ..Interface::default()
    };
    assert!(check(&contract, &actual).contains(&Mismatch::NominalIdentity("Eq".into())));
}

#[test]
fn existential_opening_maps_outer_contract_but_respects_a_shadowing_body() {
    let original = Type::Exists {
        scope: Scope::default(),
        binder: "K".into(),
        contract: Box::new(Type::con("$exists:K", "OuterContract")),
        body: Box::new(Type::con("$exists:K", "InnerType")),
    };
    let opened = open_existential_type(&original, "K", "Fresh");
    let Type::Exists { contract, body, .. } = opened else {
        panic!("nested existential");
    };
    assert_eq!(*contract, Type::con("Fresh", "OuterContract"));
    assert_eq!(*body, Type::con("$exists:K", "InnerType"));
}

#[test]
fn existential_identity_mapping_covers_kinds_and_qualified_row_omissions_not_literal_keys() {
    use crate::types::{Atom, Binder, Elem, Row};
    let original = Type::Forall {
        vars: vec![Binder {
            name: "choice".into(),
            kind: Kind::Named {
                module: "$exists:K".into(),
                name: "Colour".into(),
            },
        }],
        constraints: vec![],
        body: Box::new(Type::Without {
            row: Box::new(Type::Row(Row::closed(vec![
                (
                    Elem::named("$exists:K.Tick"),
                    Type::con("$exists:K.Nested", "Value"),
                ),
                (Elem::Capture(Atom::Var("K".into())), Type::Var("K".into())),
            ]))),
            gone: vec![Elem::Provided(Box::new(Elem::named("$exists:K.Tick")))],
        }),
    };
    let opened = open_existential_type(&original, "K", "Fresh");
    let Type::Forall { vars, body, .. } = &opened else {
        panic!("forall");
    };
    assert_eq!(
        vars[0].kind,
        Kind::Named {
            module: "Fresh".into(),
            name: "Colour".into()
        }
    );
    let Type::Without { row, gone } = body.as_ref() else {
        panic!("omission");
    };
    let Type::Row(row) = row.as_ref() else {
        panic!("row");
    };
    assert!(row.entries.contains(&(
        Elem::named("Fresh.Tick"),
        Type::con("Fresh.Nested", "Value")
    )));
    assert!(
        row.entries
            .contains(&(Elem::Capture(Atom::Var("K".into())), Type::Var("K".into())))
    );
    assert_eq!(
        gone,
        &vec![Elem::Provided(Box::new(Elem::named("Fresh.Tick")))]
    );
    assert_eq!(close_existential_type(&opened, "Fresh", "K"), original);
    let literal_record = Type::Record(Row::closed(vec![(
        Elem::named("$exists:K.Field"),
        string(),
    )]));
    assert_eq!(
        open_existential_type(&literal_record, "K", "Fresh"),
        literal_record
    );
}

#[test]
fn closing_a_witness_avoids_capture_by_an_inner_same_named_existential() {
    let original = Type::Exists {
        scope: Scope::default(),
        binder: "K".into(),
        contract: Box::new(string()),
        body: Box::new(Type::Tuple(vec![
            Type::con("Fresh", "Outer"),
            Type::con("$exists:K", "Inner"),
        ])),
    };
    let closed = close_existential_type(&original, "Fresh", "K");
    let Type::Exists { binder, body, .. } = &closed else {
        panic!("nested existential");
    };
    assert_ne!(binder, "K", "alpha-rename only the capturing inner binder");
    assert_eq!(
        **body,
        Type::Tuple(vec![
            Type::con("$exists:K", "Outer"),
            Type::con(&format!("$exists:{binder}"), "Inner")
        ])
    );
    let reopened = open_existential_type(&closed, "K", "Fresh");
    let Type::Exists { body, .. } = reopened else {
        panic!("nested existential");
    };
    assert_eq!(
        *body,
        Type::Tuple(vec![
            Type::con("Fresh", "Outer"),
            Type::con(&format!("$exists:{binder}"), "Inner")
        ])
    );
}

fn witness_row_operations(labels: &str, values: &str) -> Type {
    use crate::types::{Elem, Row};

    let mut left = Row::closed(vec![(
        Elem::named(format!("{labels}.Field")),
        Type::con(values, "Value"),
    )]);
    left.tail = Some(Box::new(Type::Row(Row::closed(vec![(
        Elem::named(format!("{labels}.Tail")),
        Type::con(values, "TailValue"),
    )]))));
    let right = Row::closed(vec![(
        Elem::named(format!("{labels}.Other")),
        Type::Record(Row::closed(vec![(
            Elem::named("$exists:K.Nested"),
            Type::con(values, "NestedValue"),
        )])),
    )]);
    Type::Without {
        row: Box::new(Type::Union(
            Box::new(Type::Row(left)),
            Box::new(Type::Row(right)),
        )),
        gone: vec![Elem::named(format!("{labels}.Field"))],
    }
}

#[test]
fn witness_transport_distinguishes_field_tail_operations_from_semantic_rows() {
    use crate::types::Row;

    for fields in [false, true] {
        let wrap = |tail| {
            if fields {
                Type::Record(Row {
                    tail: Some(Box::new(tail)),
                    ..Row::default()
                })
            } else {
                tail
            }
        };
        let original = wrap(witness_row_operations("$exists:K", "$exists:K"));
        let labels = if fields { "$exists:K" } else { "Fresh" };
        let expected = wrap(witness_row_operations(labels, "Fresh"));
        let opened = open_existential_type(&original, "K", "Fresh");
        assert_eq!(opened, expected, "field-row position={fields}");
        assert_eq!(close_existential_type(&opened, "Fresh", "K"), original);
    }
}

#[test]
fn literal_field_tail_labels_do_not_count_as_witness_dependencies() {
    use crate::types::Row;

    let operations = witness_row_operations("$exists:K", "Global");
    assert!(witness_scope_occurs(&operations, "$exists:K"));
    let literal = Type::Record(Row {
        tail: Some(Box::new(operations)),
        ..Row::default()
    });
    assert!(!witness_scope_occurs(&literal, "$exists:K"));
    assert_eq!(open_existential_type(&literal, "K", "Fresh"), literal);
}
