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
        }],
        ..Interface::default()
    };
    assert!(check(&contract, &actual).contains(&Mismatch::NominalIdentity("Eq".into())));
}
