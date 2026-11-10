use super::*;
use crate::iface::{read, write};

fn structure(fields: &[&str]) -> ModuleSignature {
    ModuleSignature::Structure {
        requirements: fields
            .iter()
            .map(|name| R::Value {
                name: (*name).into(),
                ty: Type::con("Prelude", "Int"),
            })
            .collect(),
    }
}

fn factory(input: ModuleSignature, output: ModuleSignature) -> ModuleSignature {
    ModuleSignature::Functor {
        group_size: 1,
        binder: "K".into(),
        parameter: Box::new(input),
        result: Box::new(output),
    }
}

#[test]
fn factory_argument_is_contravariant_and_result_is_covariant() {
    let expected = factory(structure(&["seed"]), structure(&["answer"]));
    let accepts_less_returns_more = factory(structure(&[]), structure(&["answer", "extra"]));
    let requires_more = factory(structure(&["seed", "extra"]), structure(&["answer"]));
    let returns_less = factory(structure(&["seed"]), structure(&[]));
    assert!(accepts_less_returns_more.is_subtype_of(&expected));
    assert!(!requires_more.is_subtype_of(&expected));
    assert!(!returns_less.is_subtype_of(&expected));
}

#[test]
fn result_annotation_hides_representation_with_a_fresh_identity() {
    let implementation = ModuleSignature::Structure {
        requirements: vec![
            R::Type {
                name: "Key".into(),
                params: Vec::new(),
                manifest: Some(Type::con("Prelude", "String")),
            },
            R::Value {
                name: "value".into(),
                ty: Type::Var("Key".into()),
            },
        ],
    }
    .interface("Argument");
    let signature = ModuleSignature::Structure {
        requirements: vec![
            R::Type {
                name: "Key".into(),
                params: Vec::new(),
                manifest: None,
            },
            R::Value {
                name: "value".into(),
                ty: Type::Var("Key".into()),
            },
        ],
    };
    let sealed = signature.seal(&implementation, "Chosen");
    assert_eq!(
        sealed
            .find(Namespace::Type, "Key")
            .expect("type")
            .entity
            .module,
        "Chosen"
    );
    assert_eq!(
        sealed
            .find(Namespace::Value, "value")
            .expect("value")
            .scheme,
        Some(Type::con("Chosen", "Key"))
    );
}

#[test]
fn refinements_cannot_invent_members_or_contradict_equalities() {
    let mut signature = ModuleSignature::Structure {
        requirements: vec![R::Type {
            name: "Key".into(),
            params: Vec::new(),
            manifest: None,
        }],
    };
    assert!(
        signature
            .refine(&["Missing".into()], Type::con("Prelude", "Int"))
            .is_err()
    );
    assert!(
        signature
            .refine(&["Key".into()], Type::con("Prelude", "Int"))
            .is_ok()
    );
    assert!(
        signature
            .refine(&["Key".into()], Type::con("Prelude", "String"))
            .is_err()
    );
}

#[test]
fn parameter_renaming_respects_shadowing_inside_nested_modules() {
    let inner = factory(
        structure(&[]),
        ModuleSignature::Structure {
            requirements: vec![R::Type {
                name: "Key".into(),
                params: Vec::new(),
                manifest: Some(Type::con("$module:K", "Key")),
            }],
        },
    );
    let mut outer = ModuleSignature::Structure {
        requirements: vec![R::Module {
            name: "Inner".into(),
            signature: Box::new(inner.clone()),
        }],
    };
    outer.rename_parameter("K", "Actual");
    assert_eq!(
        outer,
        ModuleSignature::Structure {
            requirements: vec![R::Module {
                name: "Inner".into(),
                signature: Box::new(inner)
            }]
        }
    );
}

#[test]
fn parameter_renaming_preserves_dependent_nominal_identities() {
    let mut signature = ModuleSignature::Structure {
        requirements: vec![R::Nominal {
            name: "Clock".into(),
            category: ModuleNominalKind::Service,
            identity: Some(Entity {
                module: "$module:K.Child".into(),
                namespace: Namespace::Type,
                name: "Clock".into(),
            }),
            definition: None,
            methods: Vec::new(),
        }],
    };
    signature.rename_parameter("K", "Actual");
    let ModuleSignature::Structure { requirements } = signature else {
        panic!("structure")
    };
    let R::Nominal {
        identity: Some(identity),
        ..
    } = &requirements[0]
    else {
        panic!("nominal")
    };
    assert_eq!(identity.module, "Actual.Child");
}

#[test]
fn higher_order_signatures_survive_interface_serialization() {
    let signature = factory(
        factory(structure(&["seed"]), structure(&["answer"])),
        structure(&["answer"]),
    );
    let interface = signature.interface("App.Factory");
    let written = write(&interface);
    let read = read(written.as_bytes(), "App.Factory").expect("interface");
    assert_eq!(read.functor, Some(signature));
}
