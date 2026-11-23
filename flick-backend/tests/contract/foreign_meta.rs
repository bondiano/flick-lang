use flick_backend::*;

use crate::common::{TEXT, descriptor, request};

#[test]
#[allow(clippy::too_many_lines)]
fn foreign_metadata_validation_rejects_collisions_and_bad_shapes() {
    type Mutation = fn(&mut Descriptor);
    let valid = descriptor();
    assert_eq!(validate_foreign_metadata(&valid), Ok(()));

    let mut with_custom = descriptor();
    with_custom.adapter_words.push(AdapterWord {
        name: "slot".into(),
        args: vec![AdapterArg {
            name: "index".into(),
            kind: AdapterArgKind::Index,
            choices: vec![],
        }],
        repeatable: true,
        help: "Bind slot.".into(),
    });
    with_custom.adapter_words.push(AdapterWord {
        name: "abi".into(),
        args: vec![AdapterArg {
            name: "kind".into(),
            kind: AdapterArgKind::Ident,
            choices: vec!["c".into(), "fast".into()],
        }],
        repeatable: false,
        help: "Select calling convention.".into(),
    });
    with_custom.type_form_attributes.push(TypeFormAttribute {
        name: "c_type".into(),
        help: "C type form.".into(),
    });
    assert_eq!(validate_foreign_metadata(&with_custom), Ok(()));

    let bad_cases: [(&str, Mutation); 10] = [
        ("bad_word_name", |d| {
            d.adapter_words.push(AdapterWord {
                name: "BadWord".into(),
                args: vec![],
                repeatable: false,
                help: String::new(),
            });
        }),
        ("core_word_collision_arity", |d| {
            d.adapter_words.push(AdapterWord {
                name: "arity".into(),
                args: vec![],
                repeatable: false,
                help: String::new(),
            });
        }),
        ("core_word_collision_owned", |d| {
            d.adapter_words.push(AdapterWord {
                name: "owned".into(),
                args: vec![],
                repeatable: false,
                help: String::new(),
            });
        }),
        ("duplicate_word", |d| {
            for _ in 0..2 {
                d.adapter_words.push(AdapterWord {
                    name: "slot".into(),
                    args: vec![],
                    repeatable: false,
                    help: String::new(),
                });
            }
        }),
        ("bad_arg_name", |d| {
            d.adapter_words.push(AdapterWord {
                name: "slot".into(),
                args: vec![AdapterArg {
                    name: "bad-arg".into(),
                    kind: AdapterArgKind::Index,
                    choices: vec![],
                }],
                repeatable: false,
                help: String::new(),
            });
        }),
        ("index_with_choices", |d| {
            d.adapter_words.push(AdapterWord {
                name: "slot".into(),
                args: vec![AdapterArg {
                    name: "idx".into(),
                    kind: AdapterArgKind::Index,
                    choices: vec!["0".into()],
                }],
                repeatable: false,
                help: String::new(),
            });
        }),
        ("duplicate_choice", |d| {
            d.adapter_words.push(AdapterWord {
                name: "abi".into(),
                args: vec![AdapterArg {
                    name: "kind".into(),
                    kind: AdapterArgKind::Ident,
                    choices: vec!["c".into(), "c".into()],
                }],
                repeatable: false,
                help: String::new(),
            });
        }),
        ("bad_ident_choice", |d| {
            d.adapter_words.push(AdapterWord {
                name: "abi".into(),
                args: vec![AdapterArg {
                    name: "kind".into(),
                    kind: AdapterArgKind::Ident,
                    choices: vec!["NotIdent".into()],
                }],
                repeatable: false,
                help: String::new(),
            });
        }),
        ("core_attr_collision", |d| {
            d.type_form_attributes.push(TypeFormAttribute {
                name: "when".into(),
                help: String::new(),
            });
        }),
        ("duplicate_attr", |d| {
            for _ in 0..2 {
                d.type_form_attributes.push(TypeFormAttribute {
                    name: "ts".into(),
                    help: String::new(),
                });
            }
        }),
    ];
    let req = request();
    for (label, mutate) in bad_cases {
        let mut bad = descriptor();
        mutate(&mut bad);
        assert!(validate_foreign_metadata(&bad).is_err(), "{label}");
        assert!(
            handshake(&req.backend, &bad, &req.target).is_err(),
            "{label}: handshake must reject invalid foreign metadata"
        );
    }
}

#[test]
fn check_target_driver_validates_foreign_adapter_word_spans() {
    struct Accepting(Descriptor);
    impl Backend for Accepting {
        fn describe(&self) -> Descriptor {
            self.0.clone()
        }
        fn check_target(
            &self,
            _: &BuildRequest,
            _: &mut dyn Inputs,
        ) -> Result<TargetSupport, Vec<Diagnostic>> {
            Ok(TargetSupport {
                capabilities: vec![],
            })
        }
        fn generate(
            &self,
            _: &BuildRequest,
            _: &mut dyn Inputs,
        ) -> Result<BuildResult, Vec<Diagnostic>> {
            unreachable!("check_target does not invoke generate")
        }
    }

    let mut req = request();
    let mut program = flick_ir::Program::from_cbor(&req.program).expect("program");
    let valid_span = flick_ir::Span::new(flick_ir::FileId::new(0), 0, 6);
    program.modules[0].items.push(flick_ir::Item {
        span: valid_span,
        kind: flick_ir::ItemKind::Foreign(flick_ir::Foreign {
            name: "ext".into(),
            source_name: None,
            mode: flick_ir::ForeignMode::Pure,
            binding: flick_ir::Binding {
                from: Some("./ext.js".into()),
                name: "ext".into(),
            },
            arity: 1,
            dicts: 0,
            stages: vec![false],
            adapter: flick_ir::Adapter {
                owned: vec![0],
                words: vec![flick_ir::AdapterWord {
                    name: "slot".into(),
                    args: vec![flick_ir::AdapterArg::Index(0)],
                    span: valid_span,
                }],
                ..flick_ir::Adapter::default()
            },
            catch: None,
            type_params: vec![],
            ty: None,
        }),
    });
    req.program = program.to_cbor();
    let backend = Accepting(descriptor());
    let ok = check_target(&backend, &req, Limits::default(), |id| {
        (id == "source/App").then(|| TEXT.as_bytes().to_vec())
    });
    assert!(ok.is_ok(), "{ok:?}");

    // An adapter word span extending past EOF must fail request validation before check_target.
    if let flick_ir::ItemKind::Foreign(f) = &mut program.modules[0].items[1].kind {
        f.adapter.words[0].span = flick_ir::Span::new(flick_ir::FileId::new(0), 0, 9999);
    }
    req.program = program.to_cbor();
    let err = check_target(&backend, &req, Limits::default(), |id| {
        (id == "source/App").then(|| TEXT.as_bytes().to_vec())
    })
    .expect_err("out-of-bounds adapter word span");
    assert!(matches!(err, Failure::Protocol(_)), "{err:?}");
}
