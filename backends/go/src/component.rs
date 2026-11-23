//! The official Go backend over the versioned component records.

mod data;
mod doctor;
mod emit;
#[cfg(feature = "guest")]
mod guest;
mod optimize;
mod templates;

use std::collections::BTreeSet;

use crate::adapter;

use flick_backend::{
    Abi, AdapterArg, AdapterArgKind, AdapterWord, Backend, BackendId, BuildRequest, BuildResult,
    Descriptor, Diagnostic, Environment, EnvironmentServices, Inputs, Mode, ModuleRole, NamedRange,
    OptionDescriptor, Parameter, Profile, Severity, TagAxis, TargetSupport, ValueKind, Verb,
    VersionRange, command,
};

/// The same implementation serves the component and development parity tests.
#[derive(Default)]
pub struct Go;

pub(super) fn failure(code: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic {
        code: code.into(),
        severity: Severity::Error,
        message: message.into(),
        span: None,
        notes: vec![],
    }]
}

impl Backend for Go {
    fn commands(&self) -> Vec<command::Descriptor> {
        vec![doctor::descriptor()]
    }

    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        if invocation.name == "doctor" {
            doctor::run(invocation)
        } else {
            Err(command::unknown(&invocation.name))
        }
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
                id: "flick/go".into(),
                version: "0.1.0".into(),
            },
            backend_api: range("0.1.0", "0.2.0"),
            ir: range("0.15.0", "0.16.0"),
            language: range("0.1.0", "0.2.0"),
            runtime_abis: vec![NamedRange {
                name: "flickrt".into(),
                versions: range("0.1.0", "0.2.0"),
            }],
            environments: vec![Environment {
                backend: "go".into(),
                platform: "native".into(),
            }],
            tags: vec!["go".into()],
            axes: vec![TagAxis {
                name: "cgo".into(),
                tags: vec!["cgo".into()],
                default: None,
                exclusive: false,
            }],
            profiles: vec![Profile::Debug, Profile::Release],
            modes: vec![Mode::WholeProgram],
            capabilities: vec![],
            options: vec![OptionDescriptor {
                parameter: Parameter {
                    name: "cgo".into(),
                    kind: ValueKind::Flag,
                    multiple: false,
                    required: false,
                    help: "Enable cgo (sets CGO_ENABLED=1 instead of CGO_ENABLED=0).".into(),
                },
                default: Some("false".into()),
                implicit_value: None,
                choices: vec![],
                verbs: vec![Verb::Build, Verb::Run, Verb::Dev, Verb::Test],
            }],
            launch_runtimes: vec![],
            host_tools: vec!["go".into()],
            projections: vec![Abi {
                name: "flick.go".into(),
                version: "0.1.0".into(),
            }],
            stages_by_class_option: None,
            foreign_extension: Some("go".into()),
            foreign_suffixes: vec![".go".into()],
            handled_services: vec![EnvironmentServices {
                environment: Environment {
                    backend: "go".into(),
                    platform: "native".into(),
                },
                services: vec!["Database".into(), "Db".into(), "Server".into()],
            }],
            adapter_words: vec![
                AdapterWord {
                    name: "this".into(),
                    args: vec![],
                    repeatable: false,
                    help: "Call the exported Go method on the first argument via `flickrt.CallMethod`.".into(),
                },
                AdapterWord {
                    name: "cancel".into(),
                    args: vec![AdapterArg {
                        name: "mode".into(),
                        kind: AdapterArgKind::Ident,
                        choices: vec!["signal".into()],
                    }],
                    repeatable: false,
                    help: "Deliver interruption on an `Async` call through its `flickrt.Context`.".into(),
                },
                AdapterWord {
                    name: "variadic".into(),
                    args: vec![],
                    repeatable: false,
                    help: "Spread the last `List a` argument into Go variadic parameters (`flickrt.List(xs)...`).".into(),
                },
            ],
            type_form_attributes: vec![],
        }
    }

    fn check_target(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        let data = data::Data::read(request, inputs)?;
        let own_modules: BTreeSet<&str> = request
            .modules
            .iter()
            .filter(|m| m.role == ModuleRole::Own)
            .map(|m| m.name.as_str())
            .collect();
        let adapter_diagnostics = adapter::validate_program(&data.program, &own_modules);
        if !adapter_diagnostics.is_empty() {
            return Err(adapter_diagnostics);
        }
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
        let own_modules: BTreeSet<&str> = request
            .modules
            .iter()
            .filter(|m| m.role == ModuleRole::Own)
            .map(|m| m.name.as_str())
            .collect();
        let adapter_diagnostics = adapter::validate_program(&data.program, &own_modules);
        if !adapter_diagnostics.is_empty() {
            return Err(adapter_diagnostics);
        }
        emit::generate(request, &data)
    }
}
