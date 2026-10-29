//! `flick.toml`: the schema version, `[project]`, `[build]` and `[sql]`. Unknown
//! keys are errors so that a typo never silently means a default.

use std::collections::BTreeMap;
use std::ops;

use serde::Deserialize;
use toml::{Spanned, de};

use crate::codegen::Labels;
use crate::diagnostics::{Code, Diagnostic, Severity, codes};
use crate::source::{FileId, Span};
use crate::types::{self, Dialect, Thresholds, dialect};

pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub kind: Kind,
    pub source_dir: String,
    pub test_dir: String,
    /// The entry points of an app, by name: `entries`, or `entry` as the
    /// one named `main`. Each builds a runner `flick-<name>.js`.
    pub entries: Vec<Entry>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    App,
    Lib,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case", default)]
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
            effect: Backend::Effect,
            ui: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Debug,
    Release,
}

/// `[build] effect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Effect,
    /// No runtime calls; the output imports no Effect.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Emit {
    Js,
    Ts,
    /// A Go module per entry, built into an executable.
    Go,
}

#[derive(Deserialize)]
struct Version {
    #[serde(rename = "schema-version")]
    schema_version: Option<Spanned<toml::Value>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Raw {
    #[serde(rename = "schema-version")]
    _schema_version: i64,
    project: Spanned<RawProject>,
    #[serde(default)]
    build: Build,
    #[serde(default)]
    sql: RawSql,
    #[serde(default)]
    lint: RawLint,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "kebab-case")]
struct RawLint {
    #[serde(default)]
    allow: Vec<Spanned<String>>,
    #[serde(default)]
    no_let_assert: bool,
    max_function_lines: Option<Spanned<i64>>,
    max_nesting: Option<Spanned<i64>>,
    max_complexity: Option<Spanned<i64>>,
    max_params: Option<Spanned<i64>>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSql {
    dialect: Option<Spanned<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawProject {
    kind: Kind,
    #[serde(default = "source_dir")]
    source_dir: String,
    #[serde(default = "test_dir")]
    test_dir: String,
    entry: Option<Spanned<String>>,
    entries: Option<Spanned<BTreeMap<Spanned<String>, Spanned<toml::Value>>>>,
    runner: Option<Spanned<String>>,
}

fn source_dir() -> String {
    "src".into()
}

fn test_dir() -> String {
    "test".into()
}

/// Parses `flick.toml`; reports the first problem. The schema version is
/// checked first: a file of another version is not judged by this schema.
pub fn parse(text: &str, file: FileId) -> Result<Config, Diagnostic> {
    let span = |range: ops::Range<usize>| Span::new(file, offset(range.start), offset(range.end));
    let invalid = |err: de::Error| {
        let message = format!("invalid `flick.toml`: {}", err.message().trim_end());
        let diagnostic = Diagnostic::new(codes::P005, Severity::Error, message);
        match err.span() {
            Some(range) => diagnostic.with_span(span(range)),
            None => diagnostic,
        }
    };
    let version: Version = toml::from_str(text).map_err(invalid)?;
    match version.schema_version {
        Some(v) if v.get_ref().as_integer() == Some(SCHEMA_VERSION) => {}
        Some(v) => {
            return Err(Diagnostic::error(
                codes::P006,
                format!(
                    "unsupported `schema-version = {}`; this compiler supports {SCHEMA_VERSION}",
                    v.get_ref()
                ),
                span(v.span()),
            ));
        }
        None => {
            return Err(Diagnostic::error(
                codes::P006,
                format!(
                    "`flick.toml` has no `schema-version`; add `schema-version = {SCHEMA_VERSION}`"
                ),
                Span::new(file, 0, 0),
            ));
        }
    }
    let raw: Raw = toml::from_str(text).map_err(invalid)?;
    let project_span = span(raw.project.span());
    let project = raw.project.into_inner();
    let entries = entries(project.entry, project.entries, project.runner, &span)?;
    if project.kind == Kind::App && entries.is_empty() {
        return Err(Diagnostic::error(
            codes::P007,
            "`kind = \"app\"` needs `entry`, the module that exports `main`",
            project_span,
        )
        .with_fix(
            "name the entry module, e.g. `entry = \"App.Main\"`, or several with `entries = { server = \"App.Server\", cli = \"App.Cli\" }`",
            Vec::new(),
        ));
    }
    let sql_dialect = match raw.sql.dialect {
        None => Default::default(),
        Some(name) => sql_dialect(name.get_ref())
            .map_err(|(code, message)| Diagnostic::error(code, message, span(name.span())))?,
    };
    let allow = raw
        .lint
        .allow
        .iter()
        .map(|c| {
            c.get_ref().parse::<Code>().map_err(|()| {
                let message = format!(
                    "unknown diagnostic code `{}` in `[lint] allow`",
                    c.get_ref()
                );
                Diagnostic::error(codes::P005, message, span(c.span()))
            })
        })
        .collect::<Result<_, _>>()?;
    let threshold = |raw: &Option<Spanned<i64>>, key: &str, default: u32| match raw {
        None => Ok(default),
        Some(n) => u32::try_from(*n.get_ref())
            .ok()
            .filter(|&n| n > 0)
            .ok_or_else(|| {
                let message = format!("`[lint] {key}` is a positive integer");
                Diagnostic::error(codes::P005, message, span(n.span()))
            }),
    };
    let defaults = Thresholds::default();
    let lint = &raw.lint;
    let thresholds = Thresholds {
        function_lines: threshold(
            &lint.max_function_lines,
            "max-function-lines",
            defaults.function_lines,
        )?,
        nesting: threshold(&lint.max_nesting, "max-nesting", defaults.nesting)?,
        complexity: threshold(&lint.max_complexity, "max-complexity", defaults.complexity)?,
        params: threshold(&lint.max_params, "max-params", defaults.params)?,
    };
    Ok(Config {
        kind: project.kind,
        source_dir: project.source_dir,
        test_dir: project.test_dir,
        entries,
        build: raw.build,
        sql_dialect,
        allow,
        no_let_assert: raw.lint.no_let_assert,
        thresholds,
    })
}

/// The entries `entry` or `entries` name; both, an empty table or a name
/// that cannot name a runner's file is an error.
#[allow(
    clippy::type_complexity,
    reason = "the raw TOML shapes of the two keys"
)]
fn entries(
    entry: Option<Spanned<String>>,
    entries: Option<Spanned<BTreeMap<Spanned<String>, Spanned<toml::Value>>>>,
    runner: Option<Spanned<String>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Vec<Entry>, Diagnostic> {
    let runner = runner.map(|r| (r.get_ref().clone(), span(r.span())));
    let entry = entry.map(|e| (e.get_ref().clone(), span(e.span())));
    let entries = match (&entry, entries) {
        (Some(_), Some(table)) => {
            return Err(Diagnostic::error(
                codes::P005,
                "`entry` and `entries` both name entry points: keep `entries`, with the module of `entry` under a name",
                span(table.span()),
            ));
        }
        (Some((module, at)), None) => vec![Entry {
            name: MAIN_ENTRY.to_owned(),
            module: module.clone(),
            span: *at,
            runner,
            client: false,
        }],
        (None, Some(table)) => {
            let table_span = span(table.span());
            let entries: Vec<Entry> = table
                .into_inner()
                .into_iter()
                .map(|(name, value)| entry_point(name.into_inner(), &value, runner.as_ref(), span))
                .collect::<Result<_, _>>()?;
            if let Some(bad) = entries.iter().find(|e| !is_entry_name(&e.name)) {
                return Err(Diagnostic::error(
                    codes::P005,
                    format!(
                        "`{}` is not an entry name: lower-case letters, digits and `-`, starting with a letter, as it names the file `flick-{}.js`",
                        bad.name, bad.name
                    ),
                    table_span,
                ));
            }
            if entries.is_empty() {
                return Err(Diagnostic::error(
                    codes::P007,
                    "`entries` names no entry point",
                    table_span,
                ));
            }
            entries
        }
        (None, None) => Vec::new(),
    };
    Ok(entries)
}

/// The entry `name` of `entries`: its module, or `{ module, runner, target }`
/// with a runner of its own instead of the project's `runner`, and
/// `target = "client"` for one that runs in a browser.
fn entry_point(
    name: String,
    value: &Spanned<toml::Value>,
    runner: Option<&(String, Span)>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Entry, Diagnostic> {
    let at = span(value.span());
    let invalid = || {
        Diagnostic::error(
            codes::P005,
            format!(
                "entry `{name}` is a module name, or `{{ module = \"…\", runner = \"…\", target = \"server\" | \"client\" }}`"
            ),
            at,
        )
    };
    let text = |v: Option<&toml::Value>| v.and_then(toml::Value::as_str).map(str::to_owned);
    let (module, runner, client) = match value.get_ref() {
        toml::Value::String(module) => (module.clone(), runner.cloned(), false),
        toml::Value::Table(table) => {
            if table
                .keys()
                .any(|k| !["module", "runner", "target"].contains(&k.as_str()))
            {
                return Err(invalid());
            }
            let client = match table.get("target").map(toml::Value::as_str) {
                None | Some(Some("server")) => false,
                Some(Some("client")) => true,
                Some(_) => return Err(invalid()),
            };
            let own = match table.get("runner") {
                Some(r) => Some((text(Some(r)).ok_or_else(invalid)?, at)),
                None => runner.cloned(),
            };
            (text(table.get("module")).ok_or_else(invalid)?, own, client)
        }
        _ => return Err(invalid()),
    };
    Ok(Entry {
        name,
        module,
        span: at,
        runner,
        client,
    })
}

/// Whether `name` can name an entry, and so the file of its runner.
fn is_entry_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The dialect `[sql] dialect` names, or why it cannot be used.
fn sql_dialect(name: &str) -> Result<&'static Dialect, (Code, String)> {
    match dialect::DIALECTS.iter().find(|(n, _)| *n == name) {
        Some((_, Some(d))) => Ok(d),
        Some((_, None)) => Err((
            codes::P022,
            format!(
                "the SQL dialect `{name}` is not supported yet; supported: {}",
                names(true)
            ),
        )),
        None => Err((
            codes::P023,
            format!("unknown SQL dialect `{name}`; known: {}", names(false)),
        )),
    }
}

fn names(supported: bool) -> String {
    dialect::DIALECTS
        .iter()
        .filter(|(_, d)| !supported || d.is_some())
        .map(|(n, _)| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn offset(n: usize) -> u32 {
    u32::try_from(n).expect("config offsets fit in u32")
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
