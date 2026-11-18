//! Package selection at the common build boundary. JS uses the distributed
//! component by default; Go retains its temporary native baseline.

#[cfg(feature = "wasm-backends")]
mod component;
#[cfg(feature = "wasm-backends")]
mod dependencies;
#[cfg(feature = "wasm-backends")]
mod execute;
#[cfg(feature = "wasm-backends")]
pub(crate) mod launch;
#[cfg(feature = "wasm-backends")]
mod official;
#[cfg(feature = "wasm-backends")]
mod request;
#[cfg(feature = "wasm-backends")]
mod tools;

use crate::artifact::Artifact;
use crate::toolchain;
use std::path::Path;
use std::process::Command;
#[cfg(feature = "wasm-backends")]
use std::sync::Arc;
use std::sync::OnceLock;

use crate::sources::FsHost;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
#[cfg(feature = "wasm-backends")]
use flick_core::project;
use flick_core::project::{Compilation, Project};
use flick_core::source::SourceMap;
use flick_ir::Program;

#[derive(Clone, Default)]
pub(crate) enum Selected {
    #[default]
    Native,
    #[cfg(feature = "wasm-backends")]
    Component(Arc<component::Package>),
}

/// Preserve the compiler-owned JS runtime diagnostic before loading packages.
/// Other components negotiate their runtime requirements through the backend ABI.
pub(crate) fn check_runtime(project: &Project, rt: bool) -> Option<Diagnostic> {
    if let Some(package) = &project.config.build.backend {
        #[cfg(feature = "wasm-backends")]
        if !official::matches(package) {
            return None;
        }
        #[cfg(not(feature = "wasm-backends"))]
        {
            let _ = package;
            return None;
        }
    }
    toolchain::check_runtime(&project.root, rt)
}

impl Selected {
    #[cfg(feature = "wasm-backends")]
    pub(crate) fn supports_host(&self, capability: &str) -> bool {
        match self {
            Self::Native => true,
            Self::Component(package) => package.supports_host(capability),
        }
    }

    #[cfg_attr(
        not(feature = "wasm-backends"),
        expect(
            clippy::unnecessary_wraps,
            reason = "component tool authorization can fail"
        )
    )]
    pub(crate) fn schema_aot_command(&self, root: &Path) -> Result<Command, String> {
        match self {
            Self::Native => {
                let _ = root;
                Ok(Command::new("node"))
            }
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => package.schema_aot_command(root),
        }
    }
    pub(crate) fn reference_ir(
        &self,
        sources: &mut SourceMap,
        project: Option<&Project>,
        compilation: &Compilation,
        rt: bool,
    ) -> Option<Program> {
        match self {
            Self::Native => {
                let _ = (sources, project, compilation, rt);
                None
            }
            #[cfg(feature = "wasm-backends")]
            Self::Component(_) => Some(
                request::program(sources, compilation, project::stages_by_class(project, rt)).0,
            ),
        }
    }

    /// Resolve and verify a pin before consulting cached output. Selecting an
    /// unavailable or incompatible component never selects native codegen.
    pub(crate) fn load(
        host: &FsHost,
        project: Option<&Project>,
        go: bool,
        rt: bool,
    ) -> Result<Self, Diagnostic> {
        if project.is_some() && !allowed_tools().is_empty() {
            return Err(Diagnostic::new(
                codes::P051,
                Severity::Error,
                "`--allow-tool` grants a tool outside a project; a project allows its tools in `[tools]`",
            ));
        }
        let Some((project, package)) =
            project.and_then(|p| Some((p, p.config.build.backend.as_ref()?)))
        else {
            #[cfg(feature = "wasm-backends")]
            {
                let root = project.map_or(host.cwd.as_path(), |p| p.root.as_path());
                let tools = project.map(|p| &p.config.tools);
                let package = if go {
                    official::go(root, tools)
                } else {
                    official::javascript(root, rt, tools)
                };
                return package.map(|p| Self::Component(Arc::new(p)));
            }
            #[cfg(not(feature = "wasm-backends"))]
            {
                let _ = (host, go, rt);
                return Err(failure("builds require WebAssembly backend support"));
            }
        };
        #[cfg(feature = "wasm-backends")]
        {
            component::Package::load(project, package, rt).map(|p| Self::Component(Arc::new(p)))
        }
        #[cfg(not(feature = "wasm-backends"))]
        {
            let _ = (project, rt);
            Err(failure(format!(
                "backend {}@{}#{}: this compiler was built without WebAssembly backends",
                package.id, package.version, package.sha256
            )))
        }
    }

    #[cfg(feature = "wasm-backends")]
    pub(crate) fn pin(&self) -> Option<flick_backend::BackendPin> {
        match self {
            Self::Native => None,
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => Some(package.pin()),
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

static ALLOWED: OnceLock<Vec<String>> = OnceLock::new();

/// The tools `--allow-tool` grants the selected backend outside a project.
pub(crate) fn allow_tools(tools: Vec<String>) {
    let _ = ALLOWED.set(tools);
}

fn allowed_tools() -> &'static [String] {
    ALLOWED.get().map_or(&[], Vec::as_slice)
}

pub(crate) fn failure(why: impl Into<String>) -> Diagnostic {
    Diagnostic::new(codes::P013, Severity::Error, why)
}
