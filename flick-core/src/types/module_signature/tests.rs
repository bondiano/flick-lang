use super::*;
use crate::iface::{read, write};
use crate::types::{Atom, Elem};

#[test]
fn symbolic_interfaces_keep_manifest_aliases_as_bounded_witness_references() {
    let signature = ModuleSignature::Structure {
        requirements: vec![
            R::Type {
                name: "T".into(),
                params: Vec::new(),
                manifest: Some(Type::Tuple(vec![
                    Type::Var("T".into()),
                    Type::Var("T".into()),
                ])),
            },
            R::Value {
                name: "value".into(),
                ty: Type::Var("T".into()),
            },
        ],
    };
    let interface = signature.interface("Witness");
    assert_eq!(
        interface
            .find(Namespace::Value, "value")
            .and_then(|e| e.scheme.as_ref()),
        Some(&Type::con("Witness", "T"))
    );
    let alias = interface
        .find(Namespace::Type, "T")
        .and_then(|e| e.definition.as_ref())
        .expect("alias definition");
    assert!(
        matches!(&alias.form, Form::Alias { body: Type::Tuple(items), .. } if items == &vec![Type::con("Witness", "T"), Type::con("Witness", "T")])
    );
}

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
    ModuleSignature::Parameterized {
        group_size: 1,
        binder: "K".into(),
        parameter: Box::new(input),
        result: Box::new(output),
    }
}

#[test]
fn dependent_covariance_uses_the_expected_parameters_manifest_equalities() {
    let input = |manifest| ModuleSignature::Structure {
        requirements: vec![
            R::Type {
                name: "Item".into(),
                params: Vec::new(),
                manifest,
            },
            R::Value {
                name: "value".into(),
                ty: Type::Var("Item".into()),
            },
        ],
    };
    let output = |ty| ModuleSignature::Structure {
        requirements: vec![R::Value {
            name: "value".into(),
            ty,
        }],
    };
    let provider = factory(input(None), output(Type::con("$module:K", "Item")));
    let expected = factory(
        input(Some(Type::con("Prelude", "String"))),
        output(Type::con("Prelude", "String")),
    );
    assert!(provider.is_subtype_of(&expected));
    assert!(!expected.is_subtype_of(&provider));
}

fn dependent_structure(binder: &str) -> ModuleSignature {
    let child = ModuleSignature::Structure {
        requirements: vec![R::Type {
            name: "Item".into(),
            params: Vec::new(),
            manifest: None,
        }],
    };
    let item = Type::con(&format!("$module:{binder}"), "Item");
    ModuleSignature::Structure {
        requirements: vec![
            R::Module {
                name: "M".into(),
                binder: binder.into(),
                signature: Box::new(child),
            },
            R::Value {
                name: "value".into(),
                ty: item.clone(),
            },
            R::Type {
                name: "Alias".into(),
                params: Vec::new(),
                manifest: Some(item),
            },
        ],
    }
}

#[test]
fn inferred_rows_distinguish_local_errors_services_and_reexported_identities() {
    for imported in [false, true] {
        let actual = nominal_row_source(if imported { "External" } else { "Factory" });
        let ModuleSignature::Structure { requirements } = ModuleSignature::from_interface(&actual)
        else {
            panic!("structure");
        };
        let R::Nominal { identity, .. } = &requirements[1] else {
            panic!("service");
        };
        assert_eq!(
            identity.as_ref().map(|e| e.module.as_str()),
            imported.then_some("External")
        );
        let R::Value {
            ty: Type::Fun {
                effects: Some(effects),
                ..
            },
            ..
        } = &requirements[2]
        else {
            panic!("operation");
        };
        let Type::Row(errors) = &effects.errors else {
            panic!("errors");
        };
        let Type::Row(services) = &effects.services else {
            panic!("services");
        };
        assert_eq!(
            errors.entries,
            vec![(
                Elem::Error(Atom::Var("Failure".into())),
                Type::Var("Failure".into())
            )]
        );
        assert_eq!(
            services.entries,
            if imported {
                vec![("External.Clock".into(), Type::con("External", "Clock"))]
            } else {
                vec![(
                    Elem::Service(Atom::Var("Clock".into())),
                    Type::Var("Clock".into()),
                )]
            }
        );
    }
}

fn nominal_row_source(owner: &str) -> Interface {
    use super::super::ty::{Effects, Row};
    let mut actual = ModuleSignature::Structure {
        requirements: vec![
            R::Type {
                name: "Failure".into(),
                params: Vec::new(),
                manifest: None,
            },
            R::Nominal {
                name: "Clock".into(),
                category: ModuleNominalKind::Service,
                identity: None,
                methods: Vec::new(),
                definition: Some(Box::new(TypeDef {
                    kind: Kind::Service,
                    form: Form::Service {
                        params: Vec::new(),
                        operations: Vec::new(),
                        effect: false,
                    },
                })),
            },
        ],
    }
    .interface("Factory");
    actual
        .exports
        .iter_mut()
        .find(|e| e.entity.name == "Clock")
        .expect("service")
        .entity
        .module = owner.into();
    actual.exports.push(Export {
        entity: Entity {
            module: "Factory".into(),
            namespace: Namespace::Value,
            name: "run".into(),
        },
        decl: Decl::Let,
        parent: None,
        definition: None,
        blocked: None,
        at: None,
        scheme: Some(Type::Fun {
            param: Box::new(Type::con("Prelude", "Unit")),
            effects: Some(Box::new(Effects::new(
                Type::Row(Row::closed(vec![(
                    Elem::Named(format!("{owner}.Clock")),
                    Type::con(owner, "Clock"),
                )])),
                Type::Row(Row::closed(vec![(
                    "Factory.Failure".into(),
                    Type::con("Factory", "Failure"),
                )])),
            ))),
            result: Box::new(Type::con("Prelude", "Int")),
        }),
    });
    actual
}

#[test]
fn structure_member_witnesses_survive_projection_and_nested_sealing() {
    let signature = dependent_structure("Contract@1#M");
    let actual = signature.interface("Source");
    let source_item = Type::con("Source.M", "Item");
    assert_eq!(
        actual
            .find(Namespace::Value, "value")
            .expect("witness value")
            .scheme,
        Some(source_item.clone())
    );
    assert!(signature.accepts(&actual));
    assert_eq!(
        signature
            .project(&actual)
            .find(Namespace::Value, "value")
            .expect("projected value")
            .scheme,
        Some(source_item)
    );
    for name in ["First", "Second"] {
        let sealed = signature.seal(&actual, name);
        let expected = Type::con(&format!("{name}.M"), "Item");
        assert_eq!(
            sealed
                .find(Namespace::Value, "value")
                .expect("sealed value")
                .scheme,
            Some(expected.clone())
        );
        assert!(
            matches!(&sealed.find(Namespace::Type, "Alias").expect("sealed alias").definition.as_ref().expect("alias definition").form,
            Form::Alias { body, .. } if body == &expected)
        );
        assert!(signature.accepts(&sealed));
    }
}

#[test]
fn structure_member_binders_are_lexical_and_canonical() {
    let mut first = dependent_structure("First@12#M");
    let mut second = dependent_structure("Second@300#M");
    assert!(first.is_subtype_of(&second));
    assert!(second.is_subtype_of(&first));
    assert!(first.display().contains("value : M.Item"));
    assert!(!first.display().contains("$module:"));
    first.canonicalize(&mut 0);
    second.canonicalize(&mut 0);
    assert_eq!(first, second);
}

#[test]
fn nested_signatures_capture_outer_types_and_respect_inner_type_binders() {
    let item = ModuleSignature::Structure {
        requirements: vec![
            R::Type {
                name: "Item".into(),
                params: Vec::new(),
                manifest: None,
            },
            R::Value {
                name: "value".into(),
                ty: Type::Var("Item".into()),
            },
        ],
    };
    let output = ModuleSignature::Structure {
        requirements: vec![R::Type {
            name: "Value".into(),
            params: Vec::new(),
            manifest: Some(Type::Var("Item".into())),
        }],
    };
    let signature = ModuleSignature::Structure {
        requirements: vec![
            R::Type {
                name: "Item".into(),
                params: Vec::new(),
                manifest: None,
            },
            R::Module {
                name: "Inner".into(),
                binder: String::new(),
                signature: Box::new(item.clone()),
            },
            R::Module {
                name: "Build".into(),
                binder: String::new(),
                signature: Box::new(factory(item, output)),
            },
        ],
    };
    let actual = signature.interface("Source");
    assert!(signature.accepts(&actual));
    let projected = signature.project(&actual);
    let sealed = signature.seal(&actual, "Result");
    for (interface, root) in [(&projected, "Source"), (&sealed, "Result")] {
        assert_eq!(
            interface.modules["Inner"]
                .find(Namespace::Value, "value")
                .expect("inner value")
                .scheme,
            Some(Type::con(&format!("{root}.Inner"), "Item"))
        );
        let ModuleSignature::Parameterized {
            parameter, result, ..
        } = interface.modules["Build"]
            .parameterized
            .as_ref()
            .expect("factory")
        else {
            panic!("factory arrow");
        };
        assert_eq!(
            parameter
                .interface("Input")
                .find(Namespace::Value, "value")
                .expect("input value")
                .scheme,
            Some(Type::con("Input", "Item"))
        );
        assert!(
            matches!(&result.interface("Output").find(Namespace::Type, "Value").expect("output type").definition.as_ref().expect("manifest definition").form,
            Form::Alias { body, .. } if body == &Type::con(root, "Item"))
        );
    }
}

#[test]
fn signature_display_distinguishes_atomic_and_curried_groups() {
    let curried = factory(
        structure(&["seed"]),
        factory(structure(&["seed"]), structure(&["answer"])),
    );
    let mut grouped = curried.clone();
    let ModuleSignature::Parameterized { group_size, .. } = &mut grouped else {
        panic!("parameterized")
    };
    *group_size = 2;
    assert_ne!(curried.display(), grouped.display());
    assert!(grouped.display().contains(", "));
}

#[test]
fn signature_display_uses_scoped_names_for_dependent_types() {
    let signature = ModuleSignature::Parameterized {
        group_size: 1,
        binder: "Lib@12#K".into(),
        parameter: Box::new(structure(&[])),
        result: Box::new(ModuleSignature::Structure {
            requirements: vec![R::Type {
                name: "Value".into(),
                params: Vec::new(),
                manifest: Some(Type::con("$module:Lib@12#K", "Key")),
            }],
        }),
    };
    assert_eq!(signature.display(), "(K : {}) -> type Value = K.Key");
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
            binder: String::new(),
            signature: Box::new(inner.clone()),
        }],
    };
    outer.rename_parameter("K", "Actual");
    assert_eq!(
        outer,
        ModuleSignature::Structure {
            requirements: vec![R::Module {
                name: "Inner".into(),
                binder: String::new(),
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
fn parameter_renaming_updates_set_keys_and_preserves_record_fields() {
    use super::super::ty::Row;
    let service = Type::con("$module:K", "Clock");
    let mut signature = ModuleSignature::Structure {
        requirements: vec![R::Value {
            name: "value".into(),
            ty: Type::Tuple(vec![
                Type::Row(Row::closed(vec![(
                    "$module:K.Clock".into(),
                    service.clone(),
                )])),
                Type::Record(Row::closed(vec![("clock".into(), service)])),
            ]),
        }],
    };
    signature.rename_parameter("K", "Actual");
    let ModuleSignature::Structure { requirements } = signature else {
        panic!("structure")
    };
    let R::Value {
        ty: Type::Tuple(parts),
        ..
    } = &requirements[0]
    else {
        panic!("value")
    };
    assert_eq!(
        parts[0],
        Type::Row(Row::closed(vec![(
            "Actual.Clock".into(),
            Type::con("Actual", "Clock")
        )]))
    );
    assert_eq!(
        parts[1],
        Type::Record(Row::closed(vec![(
            "clock".into(),
            Type::con("Actual", "Clock")
        )]))
    );
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
    assert_eq!(read.parameterized, Some(signature));
}

include!("ported_tests.rs");
