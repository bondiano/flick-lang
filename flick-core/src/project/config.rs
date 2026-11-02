//! `flick.toml`: the schema version, `[project]`, `[build]`, `[sql]`, `[lint]`,
//! `[dependencies]`, `[workspace]` and `[scripts]`. Unknown keys are errors so
//! that a typo never silently means a default.

use crate::codegen::Labels;
use crate::diagnostics::Code;
use crate::source::Span;
use crate::types::{self, Dialect, Thresholds};

#[cfg(feature = "native-project")]
mod parse;
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
    /// `[scripts]`, by name.
    pub scripts: Vec<Script>,
}

/// A script of `[scripts]`: shell commands run in order, where `@name` runs
/// another script of the same manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    pub name: String,
    pub steps: Vec<String>,
}

/// A Flick library this project compiles from its sources:
/// `[dependencies] name = { path = "…" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    /// The library's directory, relative to the manifest.
    pub path: String,
    pub span: Span,
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
    pub build: Build,
    /// `[sql] dialect`: what `sql table` and `sql { … }` are checked against.
    pub sql_dialect: &'static Dialect,
    /// `[lint] allow`: warnings that are not reported.
    pub allow: Vec<Code>,
    /// `[lint] no-let-assert`: every `let assert` is an error.
    pub no_let_assert: bool,
    /// `[lint] max-function-lines`, `max-nesting`, `max-complexity`,
    /// `max-params`.
    pub thresholds: Thresholds,
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
    #[cfg(feature = "typescript")]
    pub emit_dts: bool,
    #[cfg(feature = "typescript")]
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

impl Default for Build {
    fn default() -> Self {
        Build {
            out_dir: "dist".into(),
            profile: Profile::Debug,
            emit: Emit::Js,
            #[cfg(feature = "typescript")]
            emit_dts: false,
            #[cfg(feature = "typescript")]
            promise_facade: false,
            schema_aot: false,
            solver_steps: types::Limits::default().steps,
            solver_depth: types::Limits::default().depth,
            labels: Labels::default(),
            effect: Backend::Effect,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "native-project", derive(serde::Deserialize))]
#[cfg_attr(feature = "native-project", serde(rename_all = "lowercase"))]
pub enum Emit {
    Js,
    #[cfg(feature = "typescript")]
    Ts,
    /// A Go module per entry, built into an executable.
    Go,
}

#[cfg(all(test, feature = "native-project"))]
#[path = "config_tests.rs"]
mod tests;
