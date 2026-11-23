//! Backend descriptor and CLI/manifest options for the literal test backend.

use flick_backend::{
    AdapterArg, AdapterArgKind, AdapterWord, BackendId, Descriptor, Environment,
    EnvironmentServices, Mode, NamedRange, OptionDescriptor, Parameter, Profile, TagAxis,
    TypeFormAttribute, ValueKind, Verb, VersionRange,
};

pub(crate) fn descriptor() -> Descriptor {
    let range = |a: &str, b: &str| VersionRange {
        minimum: a.into(),
        maximum_exclusive: b.into(),
    };
    Descriptor {
        identity: BackendId {
            id: "test/literal".into(),
            version: "1.0.0".into(),
        },
        backend_api: range("0.1.0", "0.2.0"),
        ir: range("0.15.0", "0.16.0"),
        language: range("0.1.0", "0.2.0"),
        runtime_abis: vec![NamedRange {
            name: "flickrt".into(),
            versions: range("0.1.0", "0.2.0"),
        }],
        environments: vec![Environment {
            backend: "js".into(),
            platform: "node".into(),
        }],
        tags: vec!["literal".into()],
        axes: vec![
            TagAxis {
                name: "dialect".into(),
                tags: vec!["alpha".into(), "beta".into()],
                default: Some("alpha".into()),
                exclusive: true,
            },
            TagAxis {
                name: "capabilities".into(),
                tags: vec!["jit".into(), "ffi".into()],
                default: None,
                exclusive: false,
            },
        ],
        profiles: vec![Profile::Debug, Profile::Release],
        modes: vec![Mode::PerModule, Mode::WholeProgram],
        capabilities: vec![],
        options: literal_options(),
        launch_runtimes: vec!["node".into()],
        host_tools: vec!["node".into()],
        projections: vec![],
        stages_by_class_option: None,
        foreign_extension: None,
        foreign_suffixes: vec![],
        handled_services: vec![EnvironmentServices {
            environment: Environment {
                backend: "js".into(),
                platform: "node".into(),
            },
            services: vec!["Database".into(), "Db".into(), "Server".into()],
        }],
        adapter_words: vec![AdapterWord {
            name: "slot".into(),
            args: vec![AdapterArg {
                name: "index".into(),
                kind: AdapterArgKind::Index,
                choices: vec![],
            }],
            repeatable: true,
            help: "Bind parameter `index` to a literal target calling slot.".into(),
        }],
        type_form_attributes: vec![TypeFormAttribute {
            name: "lit".into(),
            help: "Literal target type form annotation.".into(),
        }],
    }
}

fn literal_options() -> Vec<OptionDescriptor> {
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
