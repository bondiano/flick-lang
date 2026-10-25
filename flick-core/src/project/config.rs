//! `flick.toml`: the schema version, `[project]`, `[build]` and `[sql]`. Unknown
//! keys are errors so that a typo never silently means a default.

use std::collections::BTreeMap;
use std::ops;

use serde::Deserialize;
use toml::{Spanned, de};

use crate::codegen::Labels;
use crate::diagnostics::{Code, Diagnostic, Severity, codes};
use crate::source::{FileId, Span};
use crate::types::{self, Dialect, dialect};

pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub kind: Kind,
    pub source_dir: String,
    pub test_dir: String,
    /// The entry points of an app, by name: `entries`, or `entry` as the
    /// one named `main`. Each builds a runner `flick-<name>.js`.
    pub entries: Vec<Entry>,
    /// `runner`: the module whose `run` starts each entry and whose `Main`
    /// is the type of its `main`, with the span of its name; without one, a
    /// pure `main` is called and an effect is run on Node or in a browser.
    pub runner: Option<(String, Span)>,
    pub build: Build,
    /// `[sql] dialect`: what `sql table` and `sql { … }` are checked against.
    pub sql_dialect: &'static Dialect,
    /// `[lint] allow`: warnings that are not reported.
    pub allow: Vec<Code>,
}

/// An entry point of an app: its name and the module whose `main` it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub module: String,
    /// The span of the module's name in `flick.toml`.
    pub span: Span,
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
struct RawLint {
    #[serde(default)]
    allow: Vec<Spanned<String>>,
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
    entries: Option<Spanned<BTreeMap<Spanned<String>, Spanned<String>>>>,
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
    let entries = entries(project.entry, project.entries, &span)?;
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
    Ok(Config {
        kind: project.kind,
        source_dir: project.source_dir,
        test_dir: project.test_dir,
        entries,
        runner: project
            .runner
            .map(|r| (r.get_ref().clone(), span(r.span()))),
        build: raw.build,
        sql_dialect,
        allow,
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
    entries: Option<Spanned<BTreeMap<Spanned<String>, Spanned<String>>>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Vec<Entry>, Diagnostic> {
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
        }],
        (None, Some(table)) => {
            let table_span = span(table.span());
            let entries: Vec<Entry> = table
                .into_inner()
                .into_iter()
                .map(|(name, module)| Entry {
                    name: name.get_ref().clone(),
                    module: module.get_ref().clone(),
                    span: span(module.span()),
                })
                .collect();
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
mod tests {
    use super::*;
    use crate::source::SourceMap;

    fn parse_text(text: &str) -> Result<Config, Diagnostic> {
        let mut sources = SourceMap::default();
        let file = sources.add("flick.toml", text);
        parse(text, file)
    }

    #[test]
    fn spec_example_parses_with_defaults() {
        let config = parse_text(
            "schema-version = 1\n[project]\nkind = \"app\"\nsource-dir = \"src\"\ntest-dir = \"test\"\n\
             entry = \"App.Main\"\n[build]\nout-dir = \"dist\"\nprofile = \"debug\"\nemit = \"js\"\n\
             emit-dts = false\nschema-aot = false\nlabels = \"all\"\n",
        )
        .expect("valid");
        assert_eq!(config.kind, Kind::App);
        let named: Vec<_> = config
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.module.as_str()))
            .collect();
        assert_eq!(named, [("main", "App.Main")]);
        assert_eq!(config.build, Build::default());
        let lib = parse_text("schema-version = 1\n[project]\nkind = \"lib\"\n").expect("valid");
        assert_eq!(
            (lib.source_dir.as_str(), lib.test_dir.as_str()),
            ("src", "test")
        );
        assert_eq!(lib.sql_dialect.name, "postgresql");
        let components = parse_text(
            "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\nlabels = \"components\"\n",
        );
        assert_eq!(components.expect("valid").build.labels, Labels::Components);
    }

    #[test]
    fn several_entries_by_name() {
        let config = parse_text(
            "schema-version = 1\n[project]\nkind = \"app\"\nentries = { server = \"App.Server\", cli-2 = \"App.Cli\" }\n",
        )
        .expect("valid");
        let named: Vec<_> = config
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.module.as_str()))
            .collect();
        assert_eq!(named, [("cli-2", "App.Cli"), ("server", "App.Server")]);
    }

    #[test]
    fn sql_dialect() {
        let pg = parse_text(
            "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"postgresql\"\n",
        )
        .expect("valid");
        assert_eq!(pg.sql_dialect, &dialect::POSTGRESQL);
        let err = parse_text(
            "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"oracle\"\n",
        )
        .expect_err("unsupported");
        assert_eq!(
            err.message,
            "the SQL dialect `oracle` is not supported yet; supported: `postgresql`"
        );
        let err = parse_text(
            "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"postgres\"\n",
        )
        .expect_err("unknown");
        assert_eq!(
            err.message,
            "unknown SQL dialect `postgres`; known: `postgresql`, `mysql`, `sqlite`, `clickhouse`, `oracle`"
        );
    }

    #[test]
    fn rejects_with_spans() {
        let cases = [
            (
                "schema-version = 1\n[project]\nkind = \"lib\"\nsrc = \"x\"\n",
                codes::P005,
                "src",
            ),
            (
                "schema-version = 1\n[project]\nkind = \"tool\"\n",
                codes::P005,
                "\"tool\"",
            ),
            (
                "schema-version = 1\n[project]\nkind = \"app\"\n",
                codes::P007,
                "",
            ),
            (
                "schema-version = 2\n[project]\nkind = \"lib\"\n",
                codes::P006,
                "2",
            ),
            (
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"A\"\nentries = { b = \"B\" }\n",
                codes::P005,
                "{ b = \"B\" }",
            ),
            (
                "schema-version = 1\n[project]\nkind = \"app\"\nentries = { Server = \"B\" }\n",
                codes::P005,
                "{ Server = \"B\" }",
            ),
            (
                "schema-version = 1\n[project]\nkind = \"app\"\nentries = {}\n",
                codes::P007,
                "{}",
            ),
            ("[project]\nkind = \"lib\"\n", codes::P006, ""),
            (
                "schema-version = 1\n[build]\nprofile = \"fast\"\n[project]\nkind = \"lib\"\n",
                codes::P005,
                "\"fast\"",
            ),
            ("schema-version = 1\n[project\n", codes::P005, ""),
            (
                "schema-version = 1\n[project]\nkind = \"lib\"\n[lint]\nallow = [\"FLICK-T999\"]\n",
                codes::P005,
                "\"FLICK-T999\"",
            ),
            (
                "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"mysql\"\n",
                codes::P022,
                "\"mysql\"",
            ),
            (
                "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\ndialect = \"pg\"\n",
                codes::P023,
                "\"pg\"",
            ),
            (
                "schema-version = 1\n[project]\nkind = \"lib\"\n[sql]\nname = \"x\"\n",
                codes::P005,
                "name",
            ),
        ];
        for (text, code, spanned) in cases {
            let err = parse_text(text).expect_err(text);
            assert_eq!(err.code, code, "{text}: {}", err.message);
            if !spanned.is_empty() {
                let span = err.span.expect("has a span");
                assert_eq!(&text[span.range()], spanned, "{text}");
            }
        }
    }
}
