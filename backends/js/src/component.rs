//! Official backend implementation over the versioned component records.

mod data;
mod emit;
#[cfg(feature = "guest")]
mod guest;
mod launch;
mod optimize;

use flick_backend::{
    Backend, BackendId, BuildRequest, BuildResult, Capability, Descriptor, Diagnostic, Environment,
    Inputs, Mode, NamedRange, Profile, Severity, TargetSupport, VersionRange,
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
    ["typescript", "declarations", "promise-facade", "schema-aot"]
        .into_iter()
        .map(|name| Capability {
            name: name.into(),
            version: "1.0.0".into(),
        })
        .collect()
}

impl Backend for Javascript {
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
            ir: range("0.11.0", "0.12.0"),
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
