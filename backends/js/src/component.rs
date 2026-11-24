//! Official backend implementation over the versioned component records.

mod client;
mod data;
mod dependencies;
mod emit;
mod ffi;
#[cfg(feature = "guest")]
mod guest;
mod launch;
mod manifest;
mod mapper;
mod optimize;
mod pack;
mod packages;
mod templates;
mod value_abi;

use flick_backend::{
    Abi, AdapterArg, AdapterArgKind, AdapterWord, Backend, BackendId, BuildRequest, BuildResult,
    Descriptor, Diagnostic, Environment, EnvironmentServices, Inputs, Mode, NamedRange,
    OptionDescriptor, OptionValue, Packages, Parameter, Profile, ProjectFile, Severity, TagAxis,
    TargetSupport, TypeFormAttribute, ValueKind, Verb, VersionRange, command,
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
    fn package_files(&self) -> Vec<String> {
        packages::files()
    }

    fn packages(
        &self,
        files: &[ProjectFile],
        dependencies: &[command::Dependency],
        options: &[OptionValue],
    ) -> Packages {
        packages::packages(files, dependencies, options)
    }

    fn commands(&self) -> Vec<command::Descriptor> {
        let mut commands = ffi::descriptors();
        commands.push(mapper::descriptor());
        commands.extend(dependencies::descriptors());
        commands.extend(client::descriptors());
        commands.push(packages::doctor_descriptor());
        commands
    }

    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        if ffi::descriptors().iter().any(|d| d.name == invocation.name) {
            ffi::run(invocation)
        } else if dependencies::NAMES.contains(&invocation.name.as_str()) {
            dependencies::run(invocation)
        } else if client::NAMES.contains(&invocation.name.as_str()) {
            client::run(invocation)
        } else if invocation.name == "doctor" {
            Ok(packages::run_doctor(invocation))
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

    fn templates(&self) -> Vec<command::TemplateDescriptor> {
        templates::descriptors()
    }

    fn scaffold(
        &self,
        request: &command::TemplateRequest,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        templates::scaffold(request)
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
            ir: range("0.17.0", "0.18.0"),
            language: range("0.1.0", "0.2.0"),
            runtime_abis: vec![NamedRange {
                name: "flickrt".into(),
                versions: range("0.1.0", "0.2.0"),
            }],
            environments: ["node", "browser", "bun", "cloudflare"]
                .into_iter()
                .map(|platform| Environment {
                    backend: "js".into(),
                    platform: platform.into(),
                })
                .collect(),
            tags: vec!["js".into()],
            axes: vec![
                TagAxis {
                    name: "runtime".into(),
                    tags: vec!["rt".into(), "effect".into()],
                    default: Some("rt".into()),
                    exclusive: true,
                },
                TagAxis {
                    name: "adapter".into(),
                    tags: vec!["effect-adapter".into()],
                    default: None,
                    exclusive: false,
                },
            ],
            profiles: vec![Profile::Debug, Profile::Release],
            modes: vec![Mode::PerModule, Mode::WholeProgram],
            capabilities: vec![],
            options: options(),
            launch_runtimes: vec!["node".into()],
            host_tools: vec!["node".into(), "pnpm".into()],
            projections: vec![
                Abi {
                    name: "flick.javascript".into(),
                    version: "0.1.0".into(),
                },
                Abi {
                    name: "flick.typescript".into(),
                    version: "0.2.0".into(),
                },
            ],
            stages_by_class_option: Some(OptionValue {
                key: "runtime".into(),
                value: "rt".into(),
            }),
            foreign_extension: None,
            foreign_suffixes: [".mjs", ".cjs", ".d.mts", ".d.cts", ".d.ts"]
                .into_iter()
                .map(String::from)
                .collect(),
            handled_services: vec![EnvironmentServices {
                environment: Environment {
                    backend: "js".into(),
                    platform: "node".into(),
                },
                services: vec!["Database".into(), "Db".into(), "Server".into()],
            }],
            foreign_adapters: vec![
                AdapterWord {
                    name: "this".into(),
                    args: vec![],
                    repeatable: false,
                    doc: "Pass the first argument as the JavaScript `this` receiver (`target.call(a0, ...)`).".into(),
                },
                AdapterWord {
                    name: "cancel".into(),
                    args: vec![AdapterArg {
                        name: "kind".into(),
                        kind: AdapterArgKind::Keyword(vec!["signal".into()]),
                    }],
                    repeatable: false,
                    doc: "Pass an `AbortSignal` as the last argument of an `Async` call, aborted on interruption.".into(),
                },
                AdapterWord {
                    name: "variadic".into(),
                    args: vec![],
                    repeatable: false,
                    doc: "Spread the last argument (a `List a`) into the JavaScript call.".into(),
                },
            ],
            type_form_attributes: vec![TypeFormAttribute {
                name: "ts".into(),
                doc: "Declare the TypeScript form of a foreign `type` or `service` (`@ts \"...\"`, `@ts from \"pkg\" \"Name\"`, or `@ts none`).".into(),
            }],
            value_abi: Some(value_abi::table()),
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
            capabilities: vec![],
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
