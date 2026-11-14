//! Package selection at the common build boundary. Native code generation is
//! the temporary baseline when no component package has been selected.

#[cfg(feature = "wasm-backends")]
mod component;
#[cfg(feature = "wasm-backends")]
mod dependencies;
#[cfg(feature = "wasm-backends")]
pub(crate) mod launch;
#[cfg(feature = "wasm-backends")]
mod request;

use crate::artifact::Artifact;
#[cfg(feature = "wasm-backends")]
use flick_backend::{BackendId, BackendPin};
#[cfg(feature = "wasm-backends")]
use flick_core::project::config::BackendPackage;
#[cfg(feature = "wasm-backends")]
use std::sync::Arc;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;

#[derive(Clone, Default)]
pub(crate) enum Selected {
    #[default]
    Native,
    #[cfg(feature = "wasm-backends")]
    Component(Arc<component::Package>),
}

impl Selected {
    #[cfg(feature = "wasm-backends")]
    pub(crate) fn pin(&self) -> Option<BackendPin> {
        match self {
            Self::Native => None,
            Self::Component(package) => Some(package.pin()),
        }
    }
    /// Resolve and verify a pin before consulting cached output. Selecting an
    /// unavailable or incompatible component never selects native codegen.
    pub(crate) fn load(project: Option<&Project>) -> Result<Self, Diagnostic> {
        let Some((project, package)) =
            project.and_then(|p| Some((p, p.config.build.backend.as_ref()?)))
        else {
            return Ok(Self::Native);
        };
        #[cfg(feature = "wasm-backends")]
        {
            component::Package::load(project, package).map(|p| Self::Component(Arc::new(p)))
        }
        #[cfg(not(feature = "wasm-backends"))]
        {
            let _ = project;
            Err(failure(format!(
                "backend {}@{}#{}: this compiler was built without WebAssembly backends",
                package.id, package.version, package.sha256
            )))
        }
    }

    pub(crate) fn validate_environment(
        &self,
        project: Option<&Project>,
        go: bool,
    ) -> Result<(), Diagnostic> {
        let Some(environment) = project.and_then(|p| p.config.build.environment.as_ref()) else {
            return Ok(());
        };
        if self.is_component() {
            return Ok(());
        }
        let expected = if go { ("go", "native") } else { ("js", "node") };
        if (environment.backend.as_str(), environment.platform.as_str()) == expected {
            return Ok(());
        }
        Err(failure(format!(
            "the temporary native baseline does not support environment `{}/{}` for this output; select a backend component",
            environment.backend, environment.platform
        )))
    }

    pub(crate) fn run_paths(&self, files: &[Artifact]) -> Vec<(String, String)> {
        match self {
            Self::Native => {
                let _ = files;
                Vec::new()
            }
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => package.run_paths(files),
        }
    }

    pub(crate) fn cache_identity(&self) -> String {
        match self {
            Self::Native => "native-baseline".into(),
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => package.cache_identity(),
        }
    }

    pub(crate) fn accepts_cached(&self, files: &[Artifact]) -> bool {
        match self {
            Self::Native => !files.iter().any(|a| a.path == ".flick-backend.cbor"),
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => package.accepts_cached(files),
        }
    }

    pub(crate) fn is_component(&self) -> bool {
        !matches!(self, Self::Native)
    }
}

pub(crate) fn failure(why: impl Into<String>) -> Diagnostic {
    Diagnostic::new(codes::P013, Severity::Error, why)
}

#[cfg(feature = "wasm-backends")]
pub(crate) fn pin(package: &BackendPackage) -> BackendPin {
    BackendPin {
        identity: BackendId {
            id: package.id.clone(),
            version: package.version.clone(),
        },
        sha256: package.sha256.clone(),
    }
}
