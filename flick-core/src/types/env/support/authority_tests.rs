fn class_definition() -> TypeDef {
    TypeDef {
        kind: Kind::Constraint,
        form: Form::Class {
            params: vec![],
            supers: vec![],
            fundeps: vec![],
            methods: vec![],
            defaults: vec![],
            default_sigs: vec![],
            derive: false,
        },
    }
}

fn class_support_interface(class_support: bool, dictionary_support: bool) -> Interface {
    use crate::iface::{Decl, Export, SupportType};
    use crate::types::Type;
    let entity = |name: &str| Entity {
        module: "Provider".into(),
        namespace: Namespace::Type,
        name: name.into(),
    };
    let mut interface = Interface {
        module: "Provider".into(),
        ..Interface::default()
    };
    for (name, definition, private) in [
        ("C", class_definition(), class_support),
        (
            "C$Dict",
            TypeDef {
                kind: Kind::Type,
                form: Form::Alias {
                    params: vec![],
                    body: Type::con("Prelude", "String"),
                    opaque: false,
                },
            },
            dictionary_support,
        ),
    ] {
        if private {
            interface.support_types.push(SupportType {
                entity: entity(name),
                definition,
                owner_path: vec![],
            });
        } else {
            interface.exports.push(Export {
                entity: entity(name),
                decl: Decl::Alias,
                parent: None,
                scheme: None,
                definition: Some(definition),
                blocked: None,
                at: None,
            });
        }
    }
    interface
        .support_types
        .sort_by(|a, b| a.entity.cmp(&b.entity));
    interface
}

#[test]
fn real_class_dictionary_authority_rejects_explicit_support_and_reverse_public_aliases() {
    use crate::iface::{read, write};
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    for (class_support, dictionary_support) in [(false, true), (true, false), (true, true)] {
        let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
        let interface = class_support_interface(class_support, dictionary_support);
        let reader_rejects = read(write(&interface).as_bytes(), "Provider").is_err();
        let learning_rejects = env.learn_type_support(&interface).is_err();
        assert!(
            reader_rejects && learning_rejects,
            "reader: {reader_rejects}, learning: {learning_rejects}"
        );
        assert!(env.type_def("Provider", "C").is_none());
        assert!(env.type_def("Provider", "C$Dict").is_none());
    }
}

#[test]
fn existing_class_authority_is_nominal_but_unknown_suffix_names_remain_ordinary() {
    use crate::iface::{read, write};
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let mut class = class_support_interface(true, false);
    class.exports.clear();
    let mut dictionary = class_support_interface(false, true);
    dictionary.exports.clear();
    assert!(read(write(&class).as_bytes(), "Provider").is_ok());
    assert!(read(write(&dictionary).as_bytes(), "Provider").is_ok());
    let mut ordinary = dictionary.clone();
    ordinary.support_types[0].entity.name = "Unknown.Impl".into();
    assert!(read(write(&ordinary).as_bytes(), "Provider").is_ok());
    let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
    env.learn_type_support(&ordinary)
        .expect("unknown implementation suffix remains ordinary");
    env.learn_type_support(&class)
        .expect("real private class support");
    assert!(
        env.type_def("Provider", "C$Dict").is_none(),
        "no fabricated dictionary TypeDef"
    );
    assert!(env.learn_type_support(&dictionary).is_err());
    let mut reverse = Env::new("Consumer", &syntax, &|_| None, &builtins);
    reverse
        .learn_type_support(&dictionary)
        .expect("unknown suffix is an ordinary synthetic name");
    assert!(reverse.learn_type_support(&class).is_err());
    assert!(reverse.type_def("Provider", "C").is_none());
}
#[test]
fn identified_nested_real_class_retains_nominal_dictionary_authority() {
    use crate::iface::{read, write};
    use crate::types::{ModuleNominalKind, ModuleRequirement};
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let mut interface = class_support_interface(false, true);
    let class = &mut interface.exports[0];
    let identity = class.entity.clone();
    class.entity.name = "Contract".into();
    class.definition = Some(TypeDef {
        kind: Kind::Module,
        form: Form::ModuleContract {
            requirements: vec![ModuleRequirement::Nominal {
                name: "C".into(),
                category: ModuleNominalKind::Class,
                identity: Some(identity),
                definition: Some(Box::new(class_definition())),
                methods: vec![],
            }],
        },
    });
    assert!(read(write(&interface).as_bytes(), "Provider").is_err());
    let mut abstract_view = interface.clone();
    let Form::ModuleContract { requirements } = &mut abstract_view.exports[0]
        .definition
        .as_mut()
        .expect("contract")
        .form
    else {
        panic!("contract");
    };
    let ModuleRequirement::Nominal { identity, .. } = &mut requirements[0] else {
        panic!("nominal");
    };
    *identity = None;
    assert!(
        read(write(&abstract_view).as_bytes(), "Provider").is_ok(),
        "abstract class metadata invents no canonical identity"
    );
    let dictionary = Interface {
        module: interface.module.clone(),
        support_types: interface.support_types.clone(),
        ..Interface::default()
    };
    interface.support_types.clear();
    let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
    env.learn_type_support(&interface)
        .expect("canonical nested class metadata");
    assert!(
        env.type_def("Provider", "C").is_none(),
        "metadata does not install a global class"
    );
    assert!(env.learn_type_support(&dictionary).is_err());
}
#[test]
fn public_service_projections_do_not_acquire_support_authority() {
    use crate::iface::{Decl, Export};
    use crate::types::{OpClass, Operation, Type};
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
    let mut public = Interface {
        module: "Provider".into(),
        exports: vec![Export {
            entity: Entity {
                module: "Provider".into(),
                namespace: Namespace::Type,
                name: "Clock".into(),
            },
            decl: Decl::Service,
            parent: None,
            scheme: None,
            definition: Some(TypeDef {
                kind: Kind::Service,
                form: Form::Service {
                    params: vec![],
                    operations: vec![],
                    effect: true,
                },
            }),
            blocked: None,
            at: None,
        }],
        ..Interface::default()
    };
    env.learn_type_support(&public)
        .expect("first public service view");
    let Form::Service { operations, .. } =
        &mut public.exports[0].definition.as_mut().expect("service").form
    else {
        panic!("service");
    };
    operations.push(Operation {
        name: "tick".into(),
        ty: Type::con("Prelude", "Int"),
        class: OpClass::Tail,
    });
    env.learn_type_support(&public)
        .expect("a public-only projection is not competing support authority");
}
#[test]
fn nested_nominal_views_establish_companions_not_global_definition_equality() {
    use crate::iface::{Decl, Export, SupportType, read, write};
    use crate::types::{ModuleNominalKind, ModuleRequirement};
    let entity = Entity {
        module: "Provider".into(),
        namespace: Namespace::Type,
        name: "C".into(),
    };
    let mut public = class_definition();
    let Form::Class { methods, .. } = &mut public.form else {
        panic!("class");
    };
    methods.push("show".into());
    let interface = Interface {
        module: "Provider".into(),
        exports: vec![Export {
            entity: entity.clone(),
            decl: Decl::Class,
            parent: None,
            scheme: None,
            definition: Some(public),
            blocked: None,
            at: None,
        }],
        support_types: vec![SupportType {
            entity: Entity {
                module: "Provider".into(),
                namespace: Namespace::Type,
                name: "Contract".into(),
            },
            definition: TypeDef {
                kind: Kind::Module,
                form: Form::ModuleContract {
                    requirements: vec![ModuleRequirement::Nominal {
                        name: "C".into(),
                        category: ModuleNominalKind::Class,
                        identity: Some(entity),
                        definition: Some(Box::new(class_definition())),
                        methods: vec![],
                    }],
                },
            },
            owner_path: vec![],
        }],
        ..Interface::default()
    };
    assert!(
        read(write(&interface).as_bytes(), "Provider").is_ok(),
        "a nested projected class view is not a competing global TypeDef"
    );
}
#[test]
fn witness_registration_is_owned_types_only_and_tracks_distinct_manifest_provenance() {
    use crate::types::{Binder, ModuleNominalKind, ModuleRequirement, ModuleSignature, Type};
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
    let canonical_class = class_definition();
    env.types
        .define("Global", "C".into(), canonical_class.clone());
    let mut projected_class = canonical_class.clone();
    let Form::Class { methods, .. } = &mut projected_class.form else {
        panic!("class");
    };
    methods.push("view_method".into());
    let signature = ModuleSignature::Structure {
        requirements: vec![
            ModuleRequirement::Type {
                name: "Box".into(),
                params: vec![Binder {
                    name: "a".into(),
                    kind: Kind::Type,
                }],
                manifest: Some(Type::app(
                    Type::con("Prelude", "List"),
                    Type::Var("a".into()),
                )),
            },
            ModuleRequirement::Value {
                name: "value".into(),
                ty: Type::con("Prelude", "Int"),
            },
            ModuleRequirement::Nominal {
                name: "Class".into(),
                category: ModuleNominalKind::Class,
                identity: Some(Entity {
                    module: "Global".into(),
                    namespace: Namespace::Type,
                    name: "C".into(),
                }),
                definition: Some(Box::new(projected_class)),
                methods: vec![],
            },
        ],
    };
    let view = signature.interface("Witness");
    env.register_witness_types("Witness", &view)
        .expect("validated view");
    env.register_witness_types("Witness", &view)
        .expect("equal repeated view");
    assert!(env.is_witness_manifest_alias("Witness", "Box"));
    assert!(!env.is_witness_manifest_alias("Global", "C"));
    assert_eq!(env.type_def("Global", "C"), Some(&canonical_class));
    assert!(env.values.is_empty());
    assert!(env.qualifiers.is_empty());
    assert!(env.instances.is_empty());
    let before = env.type_def("Witness", "Box").expect("real alias").clone();
    let mut conflicting = view.clone();
    let Form::Alias { body, .. } = &mut conflicting.exports[0]
        .definition
        .as_mut()
        .expect("alias")
        .form
    else {
        panic!("alias");
    };
    *body = Type::con("Prelude", "Int");
    assert!(env.register_witness_types("Witness", &conflicting).is_err());
    assert_eq!(env.type_def("Witness", "Box"), Some(&before));
    assert!(
        env.support_authorities.is_empty(),
        "witness provenance is not semantic support provenance"
    );
}
