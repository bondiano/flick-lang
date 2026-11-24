//! The options the fixture declares: its case, a banner, and the tags of
//! its axes.

use flick_backend::{OptionDescriptor, Parameter, ValueKind, Verb};

pub(super) fn literal_options() -> Vec<OptionDescriptor> {
    vec![
        OptionDescriptor {
            parameter: Parameter {
                name: "case".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "Select a test fixture behavior.".into(),
            },
            default: None,
            implicit_value: None,
            choices: vec![],
            verbs: vec![Verb::Build, Verb::Run, Verb::Test],
        },
        OptionDescriptor {
            parameter: Parameter {
                name: "banner".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "Prepend a comment banner to generated JavaScript output.".into(),
            },
            default: None,
            implicit_value: None,
            choices: vec![],
            verbs: vec![Verb::Build, Verb::Inspect],
        },
        OptionDescriptor {
            parameter: Parameter {
                name: "dialect".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "Select literal fixture dialect tag.".into(),
            },
            default: Some("alpha".into()),
            implicit_value: None,
            choices: vec!["alpha".into(), "beta".into()],
            verbs: vec![Verb::Build, Verb::Run, Verb::Test],
        },
        OptionDescriptor {
            parameter: Parameter {
                name: "jit".into(),
                kind: ValueKind::Flag,
                multiple: false,
                required: false,
                help: "Enable fixture JIT capability tag.".into(),
            },
            default: Some("false".into()),
            implicit_value: None,
            choices: vec![],
            verbs: vec![Verb::Build, Verb::Run, Verb::Test],
        },
        OptionDescriptor {
            parameter: Parameter {
                name: "ffi".into(),
                kind: ValueKind::Flag,
                multiple: false,
                required: false,
                help: "Enable fixture FFI capability tag.".into(),
            },
            default: Some("false".into()),
            implicit_value: None,
            choices: vec![],
            verbs: vec![Verb::Build, Verb::Run, Verb::Test],
        },
    ]
}
