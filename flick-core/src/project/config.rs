//! `flick.toml`: the schema version, `[project]`, `[package]`, `[build]`,
//! `[target.<name>]`, `[target.<name>.dependencies]`, `[sql]`, `[lint]`,
//! `[dependencies]`, `[workspace]` and `[scripts]`. Unknown keys are errors so
//! that a typo never silently means a default.

#[cfg(feature = "native-project")]
use std::collections::BTreeMap;

use crate::codegen::Labels;
use crate::diagnostics::Code;
pub use crate::manifest::TargetDependency;
pub use crate::plugin::reach::Grants;
use crate::source::Span;
use crate::types::{self, Thresholds};

mod backend;
pub use backend::BackendEnvironment;
#[cfg(feature = "native-project")]
pub use backend::BackendPackage;

#[cfg(feature = "native-project")]
mod deps;
#[cfg(feature = "native-project")]
mod parse;
#[cfg(feature = "native-project")]
mod parse_targets;
#[cfg(feature = "native-project")]
mod workspace;
#[cfg(feature = "native-project")]
pub use parse::{SCHEMA_VERSION, parse, parse_manifest};

/// A whole `flick.toml`: a project, a workspace of member projects, or both,
/// and the scripts it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub project: Option<Config>,
    /// `[workspace] members`: the directories of the member projects,
    /// relative to the manifest.
    pub members: Option<Vec<String>>,
    /// `[[workspace.plugins]]`: what every member gets, in order.
    pub plugins: Vec<WorkspacePlugin>,
    /// `[scripts]`, by name.
    pub scripts: Vec<Script>,
}

/// A plugin of `[[workspace.plugins]]`: each member of the workspace but its
/// package depends on `package`, from `source` relative to the workspace's
/// root, and grants its plugin `grants` on top of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePlugin {
    pub package: String,
    pub source: Source,
    pub grants: Grants,
    pub span: Span,
}

/// A script of `[scripts]`: shell commands run in order, where `@name` runs
/// another script of the same manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub name: String,
    pub steps: Vec<String>,
}

/// A Flick library this project compiles from its sources, under
/// `[dependencies]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub source: Source,
    pub span: Span,
}

impl Dependency {
    /// The library's directory relative to the manifest, for a path
    /// dependency.
    pub fn path(&self) -> Option<&str> {
        match &self.source {
            Source::Path(path) => Some(path),
            _ => None,
        }
    }
}

/// Where the sources of a dependency come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// `{ path = "…" }`: a directory, relative to the manifest.
    Path(String),
    /// `{ git = "…", tag = "…", subdir = "…" }`: a tag of a repository, the
    /// library at `subdir` of it.
    Git {
        url: String,
        tag: String,
        subdir: Option<String>,
    },
    /// `{ url = "…", sha256 = "…" }`: a gzipped tarball of the library.
    Url { url: String, sha256: String },
}

impl Source {
    /// How `flick.lock` names the source, without what pins it.
    pub fn key(&self) -> String {
        match self {
            Source::Path(path) => format!("path+{path}"),
            Source::Git { url, tag, subdir } => {
                let subdir = subdir
                    .as_ref()
                    .map_or(String::new(), |s| format!("&subdir={s}"));
                format!("git+{url}?tag={tag}{subdir}")
            }
            Source::Url { url, .. } => format!("url+{url}"),
        }
    }

    /// The directory of the library inside its fetched sources.
    pub fn subdir(&self) -> Option<&str> {
        match self {
            Source::Git { subdir, .. } => subdir.as_deref(),
            _ => None,
        }
    }
}

/// `[package]`: the name and version a library is published under, and the
/// versions of Flick that compile it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
    /// `flick = "…"`: a semver requirement on the compiler.
    pub flick: Option<String>,
    /// `plugin = "…"`: the WebAssembly component of the plugin the library
    /// provides, relative to its directory.
    pub plugin: Option<String>,
    /// `load = ["proto"]`: the extensions of the files its plugin turns into
    /// modules, without the dot.
    pub load: Vec<String>,
    /// `hooks = ["emit", "ir"]`: the stages of a build its plugin takes part
    /// in besides blocks.
    pub hooks: Vec<String>,
    /// `watch = ["Ui.Event.queue", "*.parse"]`: the names whose places of
    /// call its plugin checks, each `Module.name` or `*.name` for the name
    /// of any module; a service, `Module.Service`, whose signatures it is
    /// told of; or a tag: `@maker`, for the values it reaches, or `@cell`.
    pub watch: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub kind: Kind,
    pub source_dir: String,
    pub test_dir: String,
    /// The directory of proof modules, which only `flick prove` reads.
    pub proof_dir: String,
    /// The entry points of an app, by name: `entries`, or `entry` as the
    /// one named `main`. Each produces a target launcher.
    pub entries: Vec<Entry>,
    /// `[dependencies]`, by name.
    pub dependencies: Vec<Dependency>,
    /// `[target.<name>.dependencies]`, by target and name.
    pub target_dependencies: Vec<TargetDependency>,
    /// `[package]`, which a library others fetch has.
    pub package: Option<Package>,
    pub build: Build,
    /// `[target.<name>]`: backend-declared options by target short name.
    #[cfg(feature = "native-project")]
    pub targets: BTreeMap<String, BTreeMap<String, TargetOption>>,
    /// `[sql] dialect`: the dialect of the database `sql` blocks are for.
    pub sql_dialect: &'static str,
    /// `[lint] allow`: warnings that are not reported.
    pub allow: Vec<Code>,
    /// `[lint] no-let-assert`: every `let assert` is an error.
    pub no_let_assert: bool,
    /// `[lint] max-function-lines`, `max-nesting`, `max-complexity`,
    /// `max-params`.
    pub thresholds: Thresholds,
    /// `[plugins.<package>]`: what the plugin of each package may reach.
    #[cfg(feature = "native-project")]
    pub plugins: BTreeMap<String, Grants>,
    /// `[tools]`: the host tools that the steps of a backend or a plugin may
    /// run, by its id or package.
    #[cfg(feature = "native-project")]
    pub tools: BTreeMap<String, Vec<String>>,
}

/// An entry point of an app: its name and the module whose `main` it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub module: String,
    /// The span of the module's name in `flick.toml`.
    pub span: Span,
    /// The module whose `run` starts the entry and whose `Main` is the type
    /// of its `main`, with the span of its name: the entry's `runner`, else
    /// the project's; without one, a pure `main` is called and an effect is
    /// run by the target's default runtime.
    pub runner: Option<(String, Span)>,
    /// `target = "client"`: the entry runs on a client target, so nothing it
    /// reaches needs `Server`.
    pub client: bool,
}

/// The name of the entry `entry = "…"` defines.
pub const MAIN_ENTRY: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(serde::Deserialize))]
#[cfg_attr(feature = "native-project", serde(rename_all = "lowercase"))]
pub enum Kind {
    App,
    Lib,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    pub out_dir: String,
    pub profile: Profile,
    /// Default target backend name.
    #[cfg(feature = "native-project")]
    pub target: Option<String>,
    /// An exact backend package pin.
    #[cfg(feature = "native-project")]
    pub backend: Option<BackendPackage>,
    /// Backend plus platform; absent means the selected backend's default.
    pub environment: Option<BackendEnvironment>,
    /// Instance resolution budget per definition.
    pub solver_steps: u32,
    pub solver_depth: u32,
    /// What a release build labels `Module.name` for debugging tools.
    pub labels: Labels,
}

/// A backend option value read from `[target.<name>]` in `flick.toml`.
#[cfg(feature = "native-project")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetValue {
    Bool(bool),
    String(String),
    List(Vec<(String, Span)>),
    /// A TOML value whose type cannot represent a backend option yet, kept
    /// with its type name so the selected backend's validator reports `P058`.
    Unsupported(String),
}

/// One key/value entry inside `[target.<name>]`, with source spans for both
/// the key (unknown option) and the value (invalid type or choice).
#[cfg(feature = "native-project")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetOption {
    pub value: TargetValue,
    pub key_span: Span,
    pub value_span: Span,
}

/// What checking a module reads of a project's configuration: the key of
/// modules kept between runs holds this of it and nothing else, so no other
/// setting, and no place of one in `flick.toml`, checks them again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checked {
    pub limits: types::Limits,
    pub no_let_assert: bool,
    pub thresholds: types::Thresholds,
}

impl Default for Checked {
    /// What a file outside every project is checked with.
    fn default() -> Self {
        Checked {
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
        }
    }
}

impl Config {
    pub fn checked(&self) -> Checked {
        Checked {
            limits: types::Limits {
                steps: self.build.solver_steps,
                depth: self.build.solver_depth,
            },
            no_let_assert: self.no_let_assert,
            thresholds: self.thresholds,
        }
    }

    /// The project's default target short name: `[build.backend]` short name,
    /// else `[build] target`, else `[build] environment` backend prefix, else
    /// `"js"`.
    #[cfg(feature = "native-project")]
    pub fn default_target(&self) -> &str {
        if let Some(backend) = &self.build.backend {
            return backend.id.rsplit('/').next().unwrap_or(&backend.id);
        }
        if let Some(target) = &self.build.target {
            return target.as_str();
        }
        if let Some(environment) = &self.build.environment {
            return environment.backend.as_str();
        }
        "js"
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            kind: Kind::Lib,
            source_dir: "src".into(),
            test_dir: "test".into(),
            proof_dir: "proofs".into(),
            entries: Vec::new(),
            dependencies: Vec::new(),
            target_dependencies: Vec::new(),
            package: None,
            build: Build::default(),
            #[cfg(feature = "native-project")]
            targets: BTreeMap::new(),
            sql_dialect: "",
            allow: Vec::new(),
            no_let_assert: false,
            thresholds: Thresholds::default(),
            #[cfg(feature = "native-project")]
            plugins: BTreeMap::new(),
            #[cfg(feature = "native-project")]
            tools: BTreeMap::new(),
        }
    }
}

impl Default for Build {
    fn default() -> Self {
        Build {
            out_dir: "dist".into(),
            profile: Profile::Debug,
            #[cfg(feature = "native-project")]
            target: None,
            #[cfg(feature = "native-project")]
            backend: None,
            environment: None,
            solver_steps: types::Limits::default().steps,
            solver_depth: types::Limits::default().depth,
            labels: Labels::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(serde::Deserialize))]
#[cfg_attr(feature = "native-project", serde(rename_all = "lowercase"))]
pub enum Profile {
    Debug,
    Release,
}

#[cfg(all(test, feature = "native-project"))]
#[path = "config_tests.rs"]
mod tests;
