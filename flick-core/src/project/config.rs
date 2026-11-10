//! `flick.toml`: the schema version, `[project]`, `[package]`, `[build]`,
//! `[sql]`, `[lint]`, `[dependencies]`, `[js.dependencies]`, `[workspace]` and `[scripts]`. Unknown
//! keys are errors so that a typo never silently means a default.

use std::collections::BTreeMap;

use crate::codegen::Labels;
use crate::diagnostics::Code;
use crate::source::Span;
use crate::types::{self, Thresholds};

#[cfg(feature = "native-project")]
mod deps;
#[cfg(feature = "native-project")]
mod parse;
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

/// An npm package a manifest needs at run time on the js target, under
/// `[js.dependencies]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsDependency {
    pub name: String,
    /// A version or range of the registry.
    pub version: String,
    pub span: Span,
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
    /// of any module.
    pub watch: Vec<String>,
    pub span: Span,
}

/// What `[plugins.<package>]` lets the plugin of a package reach while it
/// expands the project's blocks; nothing without it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    /// Paths from the project's root: a file, or a directory and what it
    /// holds.
    pub fs_read: Vec<String>,
    /// Hosts it fetches from over HTTP: `db.example.com` at any port, or
    /// `localhost:8080` at that one.
    pub net: Vec<String>,
    /// Variables of the environment it reads.
    pub env: Vec<String>,
    /// The heads of blocks and the classes, as `Cli.Value`, the project does
    /// not let the plugin handle.
    pub disable: Vec<String>,
    /// Where `[plugins.<package>]` is.
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub kind: Kind,
    pub source_dir: String,
    pub test_dir: String,
    /// The entry points of an app, by name: `entries`, or `entry` as the
    /// one named `main`. Each builds a runner `flick-<name>.js`.
    pub entries: Vec<Entry>,
    /// `[dependencies]`, by name.
    pub dependencies: Vec<Dependency>,
    /// `[js.dependencies]`, by name.
    pub js_dependencies: Vec<JsDependency>,
    /// `[package]`, which a library others fetch has.
    pub package: Option<Package>,
    pub build: Build,
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
    pub plugins: BTreeMap<String, Grants>,
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
    /// run as on Node.
    pub runner: Option<(String, Span)>,
    /// `target = "client"`: the entry runs in a browser, so nothing it
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
#[cfg_attr(feature = "native-project", derive(serde::Deserialize))]
#[cfg_attr(
    feature = "native-project",
    serde(deny_unknown_fields, rename_all = "kebab-case", default)
)]
pub struct Build {
    pub out_dir: String,
    pub profile: Profile,
    pub emit: Emit,
    pub emit_dts: bool,
    /// Write Promise façades of runtime functions beside the modules.
    pub promise_facade: bool,
    pub schema_aot: bool,
    /// Instance resolution budget per definition.
    pub solver_steps: u32,
    pub solver_depth: u32,
    /// What a release build labels `Module.name` for debugging tools.
    pub labels: Labels,
    /// What runs effects: Effect, or nothing, so that only pure code builds.
    pub effect: Backend,
    /// The module that renders and mounts views, instead of React.
    pub ui: Option<String>,
}

/// What checking a module reads of a project's configuration: the key of
/// modules kept between runs holds this of it and nothing else, so no other
/// setting, and no place of one in `flick.toml`, checks them again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Checked {
    pub limits: types::Limits,
    pub no_let_assert: bool,
    pub thresholds: types::Thresholds,
    /// Whether effects are checked: not in a build without a runtime.
    pub effects: bool,
}

impl Default for Checked {
    /// What a file outside every project is checked with.
    fn default() -> Self {
        Checked {
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            effects: true,
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
            effects: self.build.effect != Backend::None,
        }
    }
}

impl Default for Build {
    fn default() -> Self {
        Build {
            out_dir: "dist".into(),
            profile: Profile::Debug,
            emit: Emit::Js,
            emit_dts: false,
            promise_facade: false,
            schema_aot: false,
            solver_steps: types::Limits::default().steps,
            solver_depth: types::Limits::default().depth,
            labels: Labels::default(),
            effect: Backend::Rt,
            ui: None,
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

/// `[build] effect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(serde::Deserialize))]
#[cfg_attr(feature = "native-project", serde(rename_all = "lowercase"))]
pub enum Backend {
    Effect,
    /// No runtime calls; the output imports no Effect.
    None,
    /// Flick's own runtime, without Effect.
    Rt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(serde::Deserialize))]
#[cfg_attr(feature = "native-project", serde(rename_all = "lowercase"))]
pub enum Emit {
    Js,
    Ts,
    /// A Go module per entry, built into an executable.
    Go,
}

#[cfg(all(test, feature = "native-project"))]
#[path = "config_tests.rs"]
mod tests;
