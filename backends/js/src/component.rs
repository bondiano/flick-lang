//! Official backend implementation over the versioned component records.

mod data;
mod emit;
mod ffi;
#[cfg(feature = "guest")]
mod guest;
mod launch;
mod mapper;
mod optimize;

use flick_backend::{
    Backend, BackendId, BuildRequest, BuildResult, Capability, Descriptor, Diagnostic, Environment,
    Inputs, Mode, NamedRange, OptionDescriptor, Parameter, Profile, Severity, TargetSupport,
    ValueKind, Verb, VersionRange, command,
};

/// The same implementation serves the component and development parity tests.
#[derive(Default)]
pub struct Javascript;

pub(super) fn failure(message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic {
        code: "P012".into(),
        severity: Severity::Error,
        message: message.into(),
        span: None,
        notes: vec![],
    }]
}

fn selected(request: &BuildRequest, module: &str) -> bool {
    request.target.mode != Mode::PerModule || request.module_root.as_deref() == Some(module)
}

fn capabilities() -> Vec<Capability> {
    ["host-node-devtools", "host-schema-aot", "host-source-build"]
        .into_iter()
        .map(|name| Capability {
            name: name.into(),
            version: "1.0.0".into(),
        })
        .collect()
}

fn options() -> Vec<OptionDescriptor> {
    let flag = |name: &str, help: &str, verbs: Vec<Verb>| OptionDescriptor {
        parameter: Parameter {
            name: name.into(),
            kind: ValueKind::Flag,
            multiple: false,
            required: false,
            help: help.into(),
        },
        default: Some("false".into()),
        implicit_value: None,
        choices: vec![],
        verbs,
    };
    vec![
        flag(
            "typescript",
            "Emit TypeScript (.ts) instead of JavaScript (.js).",
            vec![Verb::Build, Verb::Inspect],
        ),
        flag(
            "declarations",
            "Also write .d.ts files, the public TypeScript interface; an export without a TypeScript form is an error.",
            vec![Verb::Build],
        ),
        flag(
            "promise-facade",
            "Also write A/B.promise.js: Promise façades of runtime functions.",
            vec![Verb::Build],
        ),
        flag(
            "schema-aot",
            "Compile pure codecs ahead of use in the app's runner.",
            vec![Verb::Build],
        ),
        OptionDescriptor {
            parameter: Parameter {
                name: "runtime".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "JavaScript effect runtime: Flick's own runtime (rt) or the Effect adapter (effect).".into(),
            },
            default: Some("rt".into()),
            implicit_value: None,
            choices: vec!["rt".into(), "effect".into()],
            verbs: vec![Verb::Build, Verb::Run, Verb::Dev, Verb::Test, Verb::Inspect],
        },
        OptionDescriptor {
            parameter: Parameter {
                name: "devtools".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "Send spans and atom logs to DevTools listening at this URL.".into(),
            },
            default: None,
            implicit_value: Some("ws://localhost:34437".into()),
            choices: vec![],
            verbs: vec![Verb::Run, Verb::Dev],
        },
        OptionDescriptor {
            parameter: Parameter {
                name: "ui".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "Module specifier that renders and mounts views instead of React.".into(),
            },
            default: None,
            implicit_value: None,
            choices: vec![],
            verbs: vec![Verb::Build, Verb::Run, Verb::Dev, Verb::Test],
        },
    ]
}

impl Backend for Javascript {
    fn commands(&self) -> Vec<command::Descriptor> {
        let mut commands = ffi::descriptors();
        commands.push(mapper::descriptor());
        commands
    }

    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        if ffi::descriptors().iter().any(|d| d.name == invocation.name) {
            ffi::run(invocation)
        } else {
            Err(command::unknown(&invocation.name))
        }
    }

    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        match serde_json::from_slice::<mapper::Declarations>(&state) {
            Ok(declarations) => Ok(mapper::declare(&declarations, &answers)),
            Err(_) => ffi::resume(&state, &outputs),
        }
    }

    fn open(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Opened, Vec<command::Diagnostic>> {
        if invocation.name == mapper::NAME {
            Ok(mapper::open(invocation))
        } else {
            Err(command::unknown(&invocation.name))
        }
    }

    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Handled, String> {
        let _ = state;
        mapper::handle(&request, &answers)
    }

    fn describe(&self) -> Descriptor {
        let range = |minimum: &str, maximum: &str| VersionRange {
            minimum: minimum.into(),
            maximum_exclusive: maximum.into(),
        };
        Descriptor {
            identity: BackendId {
                id: "flick/js".into(),
                version: "0.1.0".into(),
            },
            backend_api: range("0.1.0", "0.2.0"),
            ir: range("0.13.0", "0.14.0"),
            language: range("0.1.0", "0.2.0"),
            runtime_abis: vec![NamedRange {
                name: "flickrt".into(),
                versions: range("0.1.0", "0.2.0"),
            }],
            environments: ["node", "browser"]
                .into_iter()
                .map(|platform| Environment {
                    backend: "js".into(),
                    platform: platform.into(),
                })
                .collect(),
            profiles: vec![Profile::Debug, Profile::Release],
            modes: vec![Mode::PerModule, Mode::WholeProgram],
            capabilities: capabilities(),
            options: options(),
        }
    }

    fn check_target(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        let data = data::Data::read(request, inputs)?;
        data.check(request)?;
        Ok(TargetSupport {
            capabilities: capabilities(),
        })
    }

    fn generate(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        let data = data::Data::read(request, inputs)?;
        data.check(request)?;
        emit::generate(request, &data)
    }
}
