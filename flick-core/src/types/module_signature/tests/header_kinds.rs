#[test]
fn header_kinds_use_the_canonical_real_service_implementation_entity() {
    let signature = ModuleSignature::Structure {
        requirements: vec![R::Nominal {
            name: "Clock".into(),
            category: ModuleNominalKind::Service,
            identity: None,
            definition: Some(Box::new(TypeDef {
                kind: Kind::arrow(Kind::Type, Kind::Service),
                form: Form::Service {
                    params: vec!["a".into()],
                    operations: Vec::new(),
                    effect: false,
                },
                variance: Vec::new(),
            })),
            methods: Vec::new(),
        }],
    };
    assert_eq!(
        signature.header_kind("W", "W", "Clock.Impl"),
        Some(Kind::arrow(Kind::Type, Kind::Type))
    );
    assert_eq!(
        signature.header_kind("W", "W.Clock", "Impl"),
        None,
        "a service does not authorize a nested module"
    );
}

#[test]
fn an_ordinary_nested_impl_member_uses_its_own_declared_kind() {
    use crate::types::ty::Binder;
    let signature = ModuleSignature::Structure {
        requirements: vec![R::Module {
            name: "Clock".into(),
            binder: "clock".into(),
            signature: Box::new(ModuleSignature::Structure {
                requirements: vec![R::Type {
                    name: "Impl".into(),
                    params: vec![Binder {
                        name: "s".into(),
                        kind: Kind::Symbol,
                    }],
                    manifest: None,
                }],
            }),
        }],
    };
    assert_eq!(
        signature.header_kind("W", "W.Clock", "Impl"),
        Some(Kind::arrow(Kind::Symbol, Kind::Type))
    );
    assert_eq!(
        signature.header_kind("W", "W", "Clock.Impl"),
        None,
        "a module does not authorize a generated companion"
    );
}
