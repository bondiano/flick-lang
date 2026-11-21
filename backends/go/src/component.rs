//! The official Go backend over the versioned component records.

mod data;
mod emit;
#[cfg(feature = "guest")]
mod guest;
mod optimize;

use flick_backend::{
    Backend, BackendId, BuildRequest, BuildResult, Descriptor, Diagnostic, Environment, Inputs,
    Mode, NamedRange, OptionDescriptor, Parameter, Profile, Severity, TargetSupport, ValueKind,
    Verb, VersionRange,
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
            ir: range("0.13.0", "0.14.0"),
            language: range("0.1.0", "0.2.0"),
            runtime_abis: vec![NamedRange {
                name: "flickrt".into(),
                versions: range("0.1.0", "0.2.0"),
            }],
            environments: vec![Environment {
                backend: "go".into(),
                platform: "native".into(),
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
        }
    }

    fn check_target(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        data::Data::read(request, inputs)?;
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
        emit::generate(request, &data)
    }
}
