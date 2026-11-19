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
#[cfg(feature = "wasm-backends")]
#[cfg(feature = "wasm-backends")]
pub(crate) use execute::Reads;
pub(crate) use tools::Tool;

use crate::artifact::Artifact;
use crate::toolchain;
use std::path::Path;
use std::process::Command;
#[cfg(feature = "wasm-backends")]
use std::sync::Arc;
use std::sync::OnceLock;

#[cfg(feature = "wasm-backends")]
use crate::commands::contributed::Contributor;
use crate::sources::FsHost;
#[cfg(feature = "wasm-backends")]
use flick_backend::host::runtime::Cancellation;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
#[cfg(feature = "wasm-backends")]
use flick_core::project;
#[cfg(feature = "wasm-backends")]
use flick_core::project::config;
use flick_core::project::{Compilation, Project};
use flick_core::source::SourceMap;
#[cfg(feature = "wasm-backends")]
use flick_ir::FileId;
use flick_ir::Program;
#[cfg(feature = "wasm-backends")]
use std::collections::BTreeMap;
#[cfg(feature = "wasm-backends")]
use std::fmt::Write as _;

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

/// The id and digest of the backend `project` selects, read from its pin
/// or the distributed one, without loading it.
#[cfg(feature = "wasm-backends")]
pub(crate) fn selected_pin(project: &Project) -> Option<(String, String)> {
    if let Some(package) = &project.config.build.backend {
        return Some((package.id.clone(), package.sha256.clone()));
    }
    let pin = if project.config.build.emit == config::Emit::Go {
        official::go_pin()
    } else {
        official::pin()
    };
    pin.ok().map(|pin| (pin.identity.id, pin.sha256))
}

/// The backend `project` selects, as a contributor of commands.
#[cfg(feature = "wasm-backends")]
pub(crate) fn contributor(project: &Project) -> Result<Arc<dyn Contributor>, String> {
    let go = project.config.build.emit == config::Emit::Go;
    let rt = project.config.build.effect == config::Backend::Rt;
    match Selected::load(&FsHost::new(), Some(project), go, rt) {
        Ok(Selected::Component(package)) => Ok(package),
        Ok(Selected::Native) => Err("the native development backend adds no commands".into()),
        Err(diagnostic) => Err(diagnostic.message),
    }
}

/// The official backends, each by its id and the digest of its component.
#[cfg(feature = "wasm-backends")]
pub(crate) fn officials() -> Vec<(String, String)> {
    ([official::pin(), official::go_pin()].into_iter().flatten())
        .map(|pin| (pin.identity.id, pin.sha256))
        .collect()
}

/// The official backend `id`, loaded for `project` whatever it selects.
#[cfg(feature = "wasm-backends")]
pub(crate) fn official_contributor(
    project: &Project,
    id: &str,
) -> Result<Arc<dyn Contributor>, String> {
    let tools = Some(&project.config.tools);
    let package = if id == "flick/go" {
        official::go(&project.root, tools)
    } else {
        official::javascript(&project.root, false, tools)
    };
    match package {
        Ok(package) => Ok(Arc::new(package)),
        Err(diagnostic) => Err(diagnostic.message),
    }
}

/// The host tools `[tools]` grants `id` in `project`, found on `PATH`.
#[cfg(feature = "wasm-backends")]
pub(crate) fn granted_tools(project: &Project, id: &str) -> Vec<Tool> {
    (project.config.tools.get(id).into_iter().flatten())
        .filter_map(|tool| Tool::find(tool, None).ok())
        .collect()
}

/// Whether a step sets only the variables of its tool's adapter.
#[cfg(feature = "wasm-backends")]
pub(crate) fn authorize(step: &flick_backend::ToolchainStep) -> Result<(), String> {
    tools::authorize(step)
}

/// Runs the steps of `result` in `order`, each in a directory beside `out`.
#[cfg(feature = "wasm-backends")]
pub(crate) fn run_steps(
    result: &flick_backend::BuildResult,
    order: &[usize],
    sources: &BTreeMap<String, Vec<u8>>,
    reads: &execute::Reads,
    tools: &[Tool],
    out: &Path,
    cancel: &Cancellation,
) -> Result<Vec<Artifact>, String> {
    execute::run(result, order, sources, reads, tools, out, cancel).map_err(|failure| match failure
    {
        execute::Failure::Cancelled => "cancelled".to_owned(),
        execute::Failure::Step(why) => why,
    })
}

static ALLOWED: OnceLock<Vec<String>> = OnceLock::new();

/// The tools `--allow-tool` grants the selected backend outside a project.
pub(crate) fn allow_tools(tools: Vec<String>) {
    let _ = ALLOWED.set(tools);
}

/// The project of a command run outside any: the working directory, its
/// backend the default one, granting the official backends only what
/// `--allow-tool` allows.
#[cfg(feature = "wasm-backends")]
pub(crate) fn outside_project(dir: &Path) -> Result<Project, Diagnostic> {
    let allowed: Vec<String> = allowed_tools().iter().map(|t| format!("{t:?}")).collect();
    let mut text = String::from("schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[tools]\n");
    for (id, _) in officials() {
        let _ = writeln!(text, "{id:?} = [{}]", allowed.join(", "));
    }
    let file = FileId::new(0);
    Ok(Project {
        root: dir.to_path_buf(),
        config: config::parse(&text, file)?,
        config_file: file,
        workspace: None,
    })
}

fn allowed_tools() -> &'static [String] {
    ALLOWED.get().map_or(&[], Vec::as_slice)
}

pub(crate) fn failure(why: impl Into<String>) -> Diagnostic {
    Diagnostic::new(codes::P013, Severity::Error, why)
}
