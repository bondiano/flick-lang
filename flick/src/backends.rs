//! Backend package selection at the common build boundary.

#[cfg(feature = "wasm-backends")]
mod component;
#[cfg(feature = "wasm-backends")]
mod execute;
#[cfg(feature = "wasm-backends")]
mod foreign;
#[cfg(feature = "wasm-backends")]
pub(crate) mod launch;
#[cfg(feature = "wasm-backends")]
mod official;
pub(crate) mod options;
#[cfg(feature = "wasm-backends")]
pub(crate) mod packages;
#[cfg(feature = "wasm-backends")]
mod request;
#[cfg(feature = "wasm-backends")]
mod runtimes;
#[cfg(feature = "wasm-backends")]
mod scaffold;
#[cfg(feature = "wasm-backends")]
mod tools;
#[cfg(feature = "wasm-backends")]
pub(crate) use component::Package;
#[cfg(feature = "wasm-backends")]
pub(crate) use execute::Reads;
#[cfg(feature = "wasm-backends")]
pub(crate) use foreign::{
    asked, check_runtime_project, foreign, foreign_extension, foreign_suffixes_for,
    handles_service, package_dirs, package_inputs, stages_by_class, stages_by_class_cached, verify,
    verify_for_target,
};
#[cfg(feature = "wasm-backends")]
pub(crate) use tools::Tool;

use crate::artifact::Artifact;
#[cfg(feature = "wasm-backends")]
use std::path::Path;
#[cfg(feature = "wasm-backends")]
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "wasm-backends")]
use crate::commands::contributed::Contributor;
use crate::sources::FsHost;
#[cfg(feature = "wasm-backends")]
use flick_backend::BackendPin;
#[cfg(feature = "wasm-backends")]
use flick_backend::host::runtime::Cancellation;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
#[cfg(feature = "wasm-backends")]
use flick_core::project::config;
#[cfg(feature = "wasm-backends")]
use flick_core::project::lock::{Lock, LockedBackend};
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

/// Verify the runtime contract reported by the selected backend component.
pub(crate) fn check_runtime(
    project: &Project,
    backend: &Selected,
    resolved: &options::ResolvedOptions,
) -> Option<Diagnostic> {
    let _ = project;
    match backend {
        Selected::Native => {
            let _ = resolved;
            None
        }
        #[cfg(feature = "wasm-backends")]
        Selected::Component(package) => package.check_runtime(&resolved.to_wire()),
    }
}

impl Selected {
    pub(crate) fn descriptor(&self) -> Option<flick_backend::Descriptor> {
        match self {
            Self::Native => None,
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => Some(package.descriptor()),
        }
    }

    pub(crate) fn reference_ir(
        &self,
        sources: &mut SourceMap,
        compilation: &Compilation,
        stages_by_class: bool,
    ) -> Option<Program> {
        match self {
            Self::Native => {
                let _ = (sources, compilation, stages_by_class);
                None
            }
            #[cfg(feature = "wasm-backends")]
            Self::Component(_) => Some(request::program(sources, compilation, stages_by_class).0),
        }
    }

    /// Resolve and verify a target backend pin before consulting cached output.
    /// Selecting an unavailable or incompatible component never selects native codegen.
    pub(crate) fn load_target(
        host: &FsHost,
        project: Option<&Project>,
        cli_target: Option<&str>,
    ) -> Result<(Self, options::TargetChoice, bool), Diagnostic> {
        if project.is_some() && !allowed_tools().is_empty() {
            return Err(Diagnostic::new(
                codes::P051,
                Severity::Error,
                "`--allow-tool` grants a tool outside a project; a project allows its tools in `[tools]`",
            ));
        }
        let (choice, overridden) = options::resolve_target(cli_target, project)?;
        match &choice {
            options::TargetChoice::Pinned { package, .. } => {
                let project = project.expect("pinned target comes from a project");
                #[cfg(feature = "wasm-backends")]
                {
                    let loaded = component::Package::load(project, package)
                        .map(|p| Self::Component(Arc::new(p)))?;
                    Ok((loaded, choice, overridden))
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
            options::TargetChoice::Official { id, .. } => {
                #[cfg(feature = "wasm-backends")]
                {
                    let root = project.map_or(host.cwd.as_path(), |p| p.root.as_path());
                    let tools = project.map(|p| &p.config.tools);
                    official::by_id(id, root, project, tools)
                        .map(|p| (Self::Component(Arc::new(p)), choice, overridden))
                }
                #[cfg(not(feature = "wasm-backends"))]
                {
                    let _ = (host, id, overridden);
                    Err(failure("builds require WebAssembly backend support"))
                }
            }
        }
    }

    /// Loads the selected backend locally without reaching the network (for `flick doctor`).
    #[cfg(feature = "wasm-backends")]
    pub(crate) fn load_target_local(
        host: &FsHost,
        project: Option<&Project>,
        cli_target: Option<&str>,
    ) -> Result<(Arc<component::Package>, options::TargetChoice), Diagnostic> {
        let (choice, _) = options::resolve_target(cli_target, project)?;
        let package = match &choice {
            options::TargetChoice::Pinned { package, .. } => {
                let project = project.expect("pinned target comes from a project");
                component::Package::load(project, package)?
            }
            options::TargetChoice::Official { id, .. } => {
                let root = project.map_or(host.cwd.as_path(), |p| p.root.as_path());
                let tools = project.map(|p| &p.config.tools);
                official::load_local(id, root, project, tools)?
            }
        };
        Ok((Arc::new(package), choice))
    }

    #[cfg(feature = "wasm-backends")]
    pub(crate) fn pin(&self) -> Option<flick_backend::BackendPin> {
        match self {
            Self::Native => None,
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => Some(package.pin()),
        }
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

    pub(crate) fn cache_identity(&self, resolved: &options::ResolvedOptions) -> String {
        match self {
            Self::Native => {
                let _ = resolved;
                "native-baseline".into()
            }
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => package.cache_identity(&resolved.to_wire()),
        }
    }

    pub(crate) fn accepts_cached(&self, files: &[Artifact]) -> bool {
        match self {
            Self::Native => !files.iter().any(|a| a.path == ".flick-backend.cbor"),
            #[cfg(feature = "wasm-backends")]
            Self::Component(package) => package.accepts_cached(files),
        }
    }
}

/// Pins the official backend `project` builds with, unless it pins its own:
/// for `flick install` (`locked`, `offline`) and `flick update` (`update`).
#[cfg(feature = "wasm-backends")]
pub(crate) fn pin_official(
    project: &Project,
    update: bool,
    locked: bool,
    offline: bool,
) -> Result<(), Diagnostic> {
    let (choice, _) = options::resolve_target(None, Some(project))?;
    match choice {
        options::TargetChoice::Pinned { .. } => Ok(()),
        options::TargetChoice::Official { id, .. } => {
            official::pin(project, id, update, locked, offline)
        }
    }
}

/// The id and digest of the backend `project` selects, read from its pin
/// or the distributed one, without loading it.
#[cfg(feature = "wasm-backends")]
pub(crate) fn selected_pin(project: &Project) -> Option<(String, String)> {
    let (choice, _) = options::resolve_target(None, Some(project)).ok()?;
    match choice {
        options::TargetChoice::Pinned { package, .. } => Some((package.id, package.sha256)),
        options::TargetChoice::Official { id, .. } => {
            official::known(id, Some(project)).map(|pin| (pin.identity.id, pin.sha256))
        }
    }
}

/// The backend `project` selects, as a contributor of commands.
#[cfg(feature = "wasm-backends")]
pub(crate) fn contributor(project: &Project) -> Result<Arc<dyn Contributor>, String> {
    match Selected::load_target(&FsHost::new(), Some(project), None) {
        Ok((Selected::Component(package), _, _)) => Ok(package),
        Ok((Selected::Native, _, _)) => {
            Err("the native development backend adds no commands".into())
        }
        Err(diagnostic) => Err(diagnostic.message),
    }
}

/// The official backends, each by its id and the digest of its component.
#[cfg(feature = "wasm-backends")]
pub(crate) fn officials() -> Vec<(String, String)> {
    (official::IDS.into_iter())
        .filter_map(|id| official::known(id, None))
        .map(|pin| (pin.identity.id, pin.sha256))
        .collect()
}

/// The official backend `id`, loaded lightweight for scaffolding (`flick new` / `flick init`).
#[cfg(feature = "wasm-backends")]
pub(crate) fn scaffold_target(
    id: &str,
    existing_lock: Option<&Lock>,
    offline: bool,
) -> Result<(Arc<dyn Contributor>, Option<LockedBackend>), Diagnostic> {
    let (pin, bytes, entry) = official::resolve_target(id, existing_lock, offline)?;
    let contributor = scaffold::ScaffoldBackend::from_bytes(&pin, &bytes, true)?;
    Ok((Arc::new(contributor), entry))
}

/// A verified backend component package loaded lightweight for scaffolding (`--from`).
#[cfg(feature = "wasm-backends")]
pub(crate) fn scaffold_package(
    pin: &BackendPin,
    bytes: &[u8],
) -> Result<Arc<dyn Contributor>, Diagnostic> {
    let contributor = scaffold::ScaffoldBackend::from_bytes(pin, bytes, false)?;
    Ok(Arc::new(contributor))
}

/// The official backend `id`, loaded for `project` whatever it selects.
#[cfg(feature = "wasm-backends")]
pub(crate) fn official_contributor(
    project: &Project,
    id: &str,
) -> Result<Arc<dyn Contributor>, String> {
    let tools = Some(&project.config.tools);
    match official::by_id(id, &project.root, Some(project), tools) {
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
static OFFLINE: AtomicBool = AtomicBool::new(false);

/// Whether this run fetches no official backend.
pub(crate) fn set_offline(offline: bool) {
    OFFLINE.store(offline, Ordering::Relaxed);
}

#[cfg(feature = "wasm-backends")]
fn offline() -> bool {
    OFFLINE.load(Ordering::Relaxed)
}

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
