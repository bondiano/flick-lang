#[test]
fn support_learning_is_types_only_idempotent_and_conflict_atomic() {
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
    let definition = TypeDef {
        kind: Kind::Type,
        form: Form::NOMINAL,
        variance: Vec::new(),
    };
    let entity = Entity {
        module: "Provider".into(),
        namespace: Namespace::Type,
        name: "Secret".into(),
    };
    let interface = Interface {
        module: "Provider".into(),
        support_types: vec![SupportType {
            entity: entity.clone(),
            definition: definition.clone(),
            owner_path: vec![],
        }],
        ..Interface::default()
    };
    env.learn_type_support(&interface).expect("first learn");
    env.learn_type_support(&interface).expect("equal repeat");
    assert_eq!(env.type_def("Provider", "Secret"), Some(&definition));
    assert!(env.values.is_empty());
    assert!(env.qualifiers.is_empty());
    assert!(env.scope.is_empty());
    assert!(env.instances.is_empty());
    assert!(env.parents.is_empty());
    let mut conflicting = interface.clone();
    conflicting.support_types[0].definition.kind = Kind::Module;
    let mut earlier = conflicting.support_types[0].clone();
    earlier.entity.name = "Earlier".into();
    conflicting.support_types.insert(0, earlier);
    assert!(env.learn_type_support(&conflicting).is_err());
    assert!(env.type_def("Provider", "Earlier").is_none());
    assert_eq!(env.type_def("Provider", "Secret"), Some(&definition));
}
#[test]
fn alias_only_discovery_learns_external_types_and_real_service_companions_only() {
    use crate::iface::{Decl, Export};
    use crate::types::Type;
    let export = |module: &str, name: &str, definition: Option<TypeDef>| Export {
        entity: Entity {
            module: module.into(),
            namespace: if definition.is_some() {
                Namespace::Type
            } else {
                Namespace::Value
            },
            name: name.into(),
        },
        decl: if definition.is_some() {
            Decl::Alias
        } else {
            Decl::Let
        },
        parent: None,
        scheme: definition.is_none().then(|| Type::con("Prelude", "Int")),
        definition,
        blocked: None,
        at: None,
        via: Vec::new(),
    };
    let published = Interface {
        module: "Published".into(),
        exports: vec![export(
            "Published",
            "Alias",
            Some(TypeDef {
                kind: Kind::Service,
                form: Form::Alias {
                    params: vec![],
                    body: Type::con("External", "Clock.Impl"),
                    opaque: false,
                },
                variance: Vec::new(),
            }),
        )],
        ..Interface::default()
    };
    let external = Interface {
        module: "External".into(),
        exports: vec![
            export(
                "External",
                "Clock",
                Some(TypeDef {
                    kind: Kind::Service,
                    form: Form::Service {
                        params: vec![],
                        operations: vec![],
                        effect: true,
                    },
                    variance: Vec::new(),
                }),
            ),
            export("External", "notImported", None),
        ],
        ..Interface::default()
    };
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\nimport Published\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let imported = |name: &str| match name {
        "Published" => Some(published.clone()),
        "External" => Some(external.clone()),
        _ => None,
    };
    let env = Env::new("Consumer", &syntax, &imported, &builtins);
    assert!(env.support_errors.is_empty(), "{:?}", env.support_errors);
    assert!(env.type_def("External", "Clock").is_some());
    assert!(env.type_def("External", "Clock.Impl").is_some());
    assert!(!env.qualifiers.contains_key("External"));
    assert!(env.values.keys().all(|entity| entity.module != "External"));
    assert!(env.instances.is_empty());
}
#[test]
fn same_tree_support_cannot_override_a_child_public_definition() {
    use crate::iface::{Decl, Export, SupportType, read, write};
    use crate::types::Type;
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
    let entity = Entity {
        module: "Provider.Child".into(),
        namespace: Namespace::Type,
        name: "X".into(),
    };
    let alias = |name| TypeDef {
        kind: Kind::Type,
        form: Form::Alias {
            params: vec![],
            body: Type::con("Prelude", name),
            opaque: false,
        },
        variance: Vec::new(),
    };
    let mut parent = Interface {
        module: "Provider".into(),
        support_types: vec![SupportType {
            entity: entity.clone(),
            definition: alias("String"),
            owner_path: vec!["Child".into()],
        }],
        ..Interface::default()
    };
    parent.modules.insert(
        "Child".into(),
        Interface {
            module: "Provider.Child".into(),
            exports: vec![Export {
                entity,
                decl: Decl::Alias,
                parent: None,
                scheme: None,
                definition: Some(alias("Int")),
                blocked: None,
                at: None,
                via: Vec::new(),
            }],
            ..Interface::default()
        },
    );
    assert!(read(write(&parent).expect("publishable interface").as_bytes(), "Provider").is_err());
    assert!(
        env.learn_type_support(&parent).is_err(),
        "support cannot win over a conflicting child public definition"
    );
    assert!(
        env.type_def("Provider.Child", "X").is_none(),
        "failed learning must be atomic"
    );
    parent.support_types[0].definition = alias("Int");
    assert!(read(write(&parent).expect("publishable interface").as_bytes(), "Provider").is_ok());
    env.learn_type_support(&parent)
        .expect("equal support and child public authority");
    env.learn_type_support(&parent)
        .expect("idempotent equal tree");
    let child = parent.modules.get_mut("Child").expect("child view");
    child.exports.clear();
    child.support_types.push(SupportType {
        entity: parent.support_types[0].entity.clone(),
        definition: alias("String"),
        owner_path: vec![],
    });
    assert!(read(write(&parent).expect("publishable interface").as_bytes(), "Provider").is_err());
}
#[test]
fn support_cannot_override_a_child_services_generated_implementation() {
    use crate::iface::{Decl, Export, SupportType, read, write};
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
    let mut entity = Entity {
        module: "Provider.Child".into(),
        namespace: Namespace::Type,
        name: "Clock.Impl".into(),
    };
    let mut parent = Interface {
        module: "Provider".into(),
        support_types: vec![SupportType {
            entity: entity.clone(),
            definition: TypeDef {
                kind: Kind::Type,
                form: Form::NOMINAL,
                variance: Vec::new(),
            },
            owner_path: vec!["Child".into()],
        }],
        ..Interface::default()
    };
    entity.name = "Clock".into();
    parent.modules.insert(
        "Child".into(),
        Interface {
            module: "Provider.Child".into(),
            exports: vec![Export {
                entity,
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
                    variance: Vec::new(),
                }),
                blocked: None,
                at: None,
                via: Vec::new(),
            }],
            ..Interface::default()
        },
    );
    let reader_rejects = read(write(&parent).expect("publishable interface").as_bytes(), "Provider").is_err();
    let learning_rejects = env.learn_type_support(&parent).is_err();
    assert!(
        reader_rejects && learning_rejects,
        "reader rejection: {reader_rejects}, learning rejection: {learning_rejects}"
    );
    assert!(env.type_def("Provider.Child", "Clock").is_none());
    assert!(env.type_def("Provider.Child", "Clock.Impl").is_none());
    assert!(env.static_parameter("Rejected", parent.clone()).is_err());
    assert!(
        !env.qualifiers.contains_key("Rejected"),
        "a failed static support learn must not install a namespace view"
    );
    let service = parent.modules["Child"].exports[0]
        .definition
        .as_ref()
        .expect("real service");
    parent.support_types[0].definition = impl_def(service).expect("real generated authority");
    assert!(read(write(&parent).expect("publishable interface").as_bytes(), "Provider").is_ok());
    env.learn_type_support(&parent)
        .expect("equal generated support");
    env.learn_type_support(&parent)
        .expect("idempotent generated support");
}
#[test]
fn support_service_cannot_override_a_child_public_implementation() {
    use crate::iface::{Decl, Export, SupportType, read, write};
    use crate::types::Type;
    let mut sources = SourceMap::default();
    let text = "module Consumer exposing (..)\n";
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let builtins = Builtins::default();
    let mut env = Env::new("Consumer", &syntax, &|_| None, &builtins);
    let mut entity = Entity {
        module: "Provider.Child".into(),
        namespace: Namespace::Type,
        name: "Clock".into(),
    };
    let mut parent = Interface {
        module: "Provider".into(),
        support_types: vec![SupportType {
            entity: entity.clone(),
            definition: TypeDef {
                kind: Kind::Service,
                form: Form::Service {
                    params: vec![],
                    operations: vec![],
                    effect: true,
                },
                variance: Vec::new(),
            },
            owner_path: vec!["Child".into()],
        }],
        ..Interface::default()
    };
    entity.name = "Clock.Impl".into();
    parent.modules.insert(
        "Child".into(),
        Interface {
            module: "Provider.Child".into(),
            exports: vec![Export {
                entity,
                decl: Decl::Alias,
                parent: None,
                scheme: None,
                definition: Some(TypeDef {
                    kind: Kind::Type,
                    form: Form::Alias {
                        params: vec![],
                        body: Type::con("Prelude", "String"),
                        opaque: false,
                    },
                    variance: Vec::new(),
                }),
                blocked: None,
                at: None,
                via: Vec::new(),
            }],
            ..Interface::default()
        },
    );
    let reader_rejects = read(write(&parent).expect("publishable interface").as_bytes(), "Provider").is_err();
    let learning_rejects = env.learn_type_support(&parent).is_err();
    assert!(
        reader_rejects && learning_rejects,
        "reader rejection: {reader_rejects}, learning rejection: {learning_rejects}"
    );
    assert!(env.type_def("Provider.Child", "Clock").is_none());
    assert!(env.type_def("Provider.Child", "Clock.Impl").is_none());
}
