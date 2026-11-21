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
