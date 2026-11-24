mod common;

use flick_backend::*;

use common::{descriptor, request};

#[test]
fn option_descriptors_and_option_values_are_validated_in_handshake() {
    let valid_flag = OptionDescriptor {
        parameter: Parameter {
            name: "typescript".into(),
            kind: ValueKind::Flag,
            multiple: false,
            required: false,
            help: "Emit TypeScript".into(),
        },
        default: Some("false".into()),
        implicit_value: None,
        choices: vec![],
        verbs: vec![Verb::Build, Verb::Inspect],
    };
    let valid_choice = OptionDescriptor {
        parameter: Parameter {
            name: "runtime".into(),
            kind: ValueKind::Text,
            multiple: false,
            required: false,
            help: "JS runtime".into(),
        },
        default: Some("rt".into()),
        implicit_value: None,
        choices: vec!["rt".into(), "effect".into()],
        verbs: vec![Verb::Build, Verb::Run, Verb::Dev, Verb::Test, Verb::Inspect],
    };
    let mut desc = descriptor();
    desc.options = vec![valid_flag.clone(), valid_choice.clone()];
    assert_eq!(validate_options(&desc), Ok(()));

    let req = request();
    let mut target = req.target.clone();
    target.options = vec![
        OptionValue {
            key: "command".into(),
            value: "build".into(),
        },
        OptionValue {
            key: "option.typescript".into(),
            value: "true".into(),
        },
        OptionValue {
            key: "option.runtime".into(),
            value: "effect".into(),
        },
    ];
    assert_eq!(handshake(&req.backend, &desc, &target), Ok(()));

    // Unknown option, bad flag value, bad choice, duplicate non-multiple
    for (label, extra) in [
        (
            "unknown",
            OptionValue {
                key: "option.bogus".into(),
                value: "1".into(),
            },
        ),
        (
            "bad-flag",
            OptionValue {
                key: "option.typescript".into(),
                value: "yes".into(),
            },
        ),
        (
            "bad-choice",
            OptionValue {
                key: "option.runtime".into(),
                value: "deno".into(),
            },
        ),
    ] {
        let mut bad = req.target.clone();
        bad.options = vec![extra];
        assert!(handshake(&req.backend, &desc, &bad).is_err(), "{label}");
    }
    let mut dup = req.target.clone();
    dup.options = vec![
        OptionValue {
            key: "option.runtime".into(),
            value: "rt".into(),
        },
        OptionValue {
            key: "option.runtime".into(),
            value: "effect".into(),
        },
    ];
    assert!(handshake(&req.backend, &desc, &dup).is_err());
}

#[test]
fn malformed_option_descriptors_are_rejected() {
    let valid_flag = OptionDescriptor {
        parameter: Parameter {
            name: "typescript".into(),
            kind: ValueKind::Flag,
            multiple: false,
            required: false,
            help: "Emit TypeScript".into(),
        },
        default: Some("false".into()),
        implicit_value: None,
        choices: vec![],
        verbs: vec![Verb::Build, Verb::Inspect],
    };
    let valid_choice = OptionDescriptor {
        parameter: Parameter {
            name: "runtime".into(),
            kind: ValueKind::Text,
            multiple: false,
            required: false,
            help: "JS runtime".into(),
        },
        default: Some("rt".into()),
        implicit_value: None,
        choices: vec!["rt".into(), "effect".into()],
        verbs: vec![Verb::Build, Verb::Run, Verb::Dev, Verb::Test, Verb::Inspect],
    };
    for case in [
        "bad-name",
        "dup-name",
        "flag-multiple",
        "flag-default",
        "bad-default-choice",
    ] {
        let mut bad_desc = descriptor();
        match case {
            "bad-name" => {
                let mut o = valid_flag.clone();
                o.parameter.name = "BadName".into();
                bad_desc.options = vec![o];
            }
            "dup-name" => {
                bad_desc.options = vec![valid_flag.clone(), valid_flag.clone()];
            }
            "flag-multiple" => {
                let mut o = valid_flag.clone();
                o.parameter.multiple = true;
                bad_desc.options = vec![o];
            }
            "flag-default" => {
                let mut o = valid_flag.clone();
                o.default = Some("maybe".into());
                bad_desc.options = vec![o];
            }
            "bad-default-choice" => {
                let mut o = valid_choice.clone();
                o.default = Some("deno".into());
                bad_desc.options = vec![o];
            }
            _ => unreachable!(),
        }
        assert!(validate_options(&bad_desc).is_err(), "{case}");
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn foreign_metadata_descriptors_are_validated_in_handshake() {
    let mut desc = descriptor();
    desc.foreign_adapters = vec![
        AdapterWord {
            name: "method".into(),
            args: vec![],
            repeatable: false,
            doc: "Call with receiver".into(),
        },
        AdapterWord {
            name: "callback".into(),
            args: vec![
                AdapterArg {
                    name: "fn".into(),
                    kind: AdapterArgKind::ParamIndex,
                },
                AdapterArg {
                    name: "data".into(),
                    kind: AdapterArgKind::Keyword(vec!["data".into()]),
                },
                AdapterArg {
                    name: "user_data".into(),
                    kind: AdapterArgKind::ParamIndex,
                },
            ],
            repeatable: true,
            doc: "Pair callback with userdata parameter".into(),
        },
    ];
    desc.type_form_attributes = vec![TypeFormAttribute {
        name: "c".into(),
        doc: "C foreign type form".into(),
    }];
    assert_eq!(validate_foreign_metadata(&desc), Ok(()));
    let req = request();
    assert_eq!(handshake(&req.backend, &desc, &req.target), Ok(()));

    for case in [
        "bad-word-name",
        "hyphen-word-name",
        "core-word-collision",
        "dup-word",
        "empty-arg-name",
        "empty-keyword-choices",
        "dup-keyword-choice",
        "bad-attr-name",
        "core-attr-collision",
        "dup-attr",
    ] {
        let mut bad = descriptor();
        match case {
            "bad-word-name" => {
                bad.foreign_adapters.push(AdapterWord {
                    name: "Method".into(),
                    args: vec![],
                    repeatable: false,
                    doc: String::new(),
                });
            }
            "hyphen-word-name" => {
                bad.foreign_adapters.push(AdapterWord {
                    name: "cancel-signal".into(),
                    args: vec![],
                    repeatable: false,
                    doc: String::new(),
                });
            }
            "core-word-collision" => {
                bad.foreign_adapters.push(AdapterWord {
                    name: "arity".into(),
                    args: vec![],
                    repeatable: false,
                    doc: String::new(),
                });
            }
            "dup-word" => {
                let w = AdapterWord {
                    name: "this".into(),
                    args: vec![],
                    repeatable: false,
                    doc: String::new(),
                };
                bad.foreign_adapters = vec![w.clone(), w];
            }
            "empty-arg-name" => {
                bad.foreign_adapters.push(AdapterWord {
                    name: "results".into(),
                    args: vec![AdapterArg {
                        name: String::new(),
                        kind: AdapterArgKind::Uint,
                    }],
                    repeatable: false,
                    doc: String::new(),
                });
            }
            "empty-keyword-choices" => {
                bad.foreign_adapters.push(AdapterWord {
                    name: "cancel".into(),
                    args: vec![AdapterArg {
                        name: "kind".into(),
                        kind: AdapterArgKind::Keyword(vec![]),
                    }],
                    repeatable: false,
                    doc: String::new(),
                });
            }
            "dup-keyword-choice" => {
                bad.foreign_adapters.push(AdapterWord {
                    name: "cancel".into(),
                    args: vec![AdapterArg {
                        name: "kind".into(),
                        kind: AdapterArgKind::Keyword(vec!["signal".into(), "signal".into()]),
                    }],
                    repeatable: false,
                    doc: String::new(),
                });
            }
            "bad-attr-name" => {
                bad.type_form_attributes.push(TypeFormAttribute {
                    name: "TS".into(),
                    doc: String::new(),
                });
            }
            "core-attr-collision" => {
                bad.type_form_attributes.push(TypeFormAttribute {
                    name: "when".into(),
                    doc: String::new(),
                });
            }
            "dup-attr" => {
                let a = TypeFormAttribute {
                    name: "ts".into(),
                    doc: String::new(),
                };
                bad.type_form_attributes = vec![a.clone(), a];
            }
            _ => unreachable!(),
        }
        assert!(validate_foreign_metadata(&bad).is_err(), "{case}");
        assert!(
            handshake(&req.backend, &bad, &req.target).is_err(),
            "{case}"
        );
    }
}
