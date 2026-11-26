//! Option descriptors for the `test/literal` fixture backend.

use flick_backend::{OptionDescriptor, Parameter, ValueKind, Verb};

fn text_option(name: &str, help: &str) -> OptionDescriptor {
    OptionDescriptor {
        parameter: Parameter {
            name: name.into(),
            kind: ValueKind::Text,
            multiple: false,
            required: false,
            help: help.into(),
        },
        default: None,
        implicit_value: None,
        choices: vec![],
        verbs: vec![Verb::Build, Verb::Run, Verb::Test],
    }
}

pub(crate) fn literal_options() -> Vec<OptionDescriptor> {
    vec![
        text_option("case", "Select a test fixture behavior."),
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
                name: "kind".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "Artifact kind: application (app) or host library (library).".into(),
            },
            default: Some("app".into()),
            implicit_value: None,
            choices: vec!["app".into(), "library".into()],
            verbs: vec![Verb::Build],
        },
        OptionDescriptor {
            parameter: Parameter {
                name: "headers".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "Request projected host header declarations (boolean or format name).".into(),
            },
            default: None,
            implicit_value: Some("c".into()),
            choices: vec![],
            verbs: vec![Verb::Build],
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
        text_option(
            "max-instances",
            "Maximum specialization instances allowed in layout-specialize mode.",
        ),
        text_option(
            "max-output-bytes",
            "Maximum estimated specialized output bytes allowed in layout-specialize mode.",
        ),
    ]
}
