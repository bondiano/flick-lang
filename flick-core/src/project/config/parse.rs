//! Native TOML parsing and validation of the shared project configuration.

use std::collections::BTreeMap;
use std::{mem, ops};

use serde::Deserialize;
use toml::{Spanned, de};

use super::deps::{RawDependency, RawGrants, RawPackage, dependencies, grants, package};
use super::{Build, Config, Entry, Kind, MAIN_ENTRY, Manifest, Script};
use crate::diagnostics::{Code, Diagnostic, Severity, codes};
use crate::source::{FileId, Span};
use crate::types::Thresholds;

pub const SCHEMA_VERSION: i64 = 1;

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
    project: Option<Spanned<RawProject>>,
    build: Option<Spanned<Build>>,
    sql: Option<Spanned<RawSql>>,
    lint: Option<Spanned<RawLint>>,
    dependencies: Option<Spanned<BTreeMap<String, Spanned<RawDependency>>>>,
    package: Option<Spanned<RawPackage>>,
    plugins: Option<Spanned<BTreeMap<String, Spanned<RawGrants>>>>,
    workspace: Option<RawWorkspace>,
    #[serde(default)]
    scripts: BTreeMap<String, Spanned<RawScript>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspace {
    members: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawScript {
    One(String),
    Many(Vec<String>),
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

/// Parses the `flick.toml` of a project; reports the first problem. A
/// workspace root without `[project]` is not one.
pub fn parse(text: &str, file: FileId) -> Result<Config, Diagnostic> {
    parse_manifest(text, file)?.project.ok_or_else(|| {
        Diagnostic::error(
            codes::P031,
            "this `flick.toml` is a workspace root, not a project: choose a member with `--package <name>`",
            Span::new(file, 0, 0),
        )
    })
}

/// Parses a `flick.toml`: a project, a workspace or both. The schema version
/// is checked first: a file of another version is not judged by this schema.
pub fn parse_manifest(text: &str, file: FileId) -> Result<Manifest, Diagnostic> {
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
    let mut raw: Raw = toml::from_str(text).map_err(invalid)?;
    let scripts = scripts(mem::take(&mut raw.scripts), &span)?;
    let members = raw.workspace.take().map(|w| w.members);
    let project = if let Some(project) = raw.project.take() {
        Some(config(project, raw, &span)?)
    } else {
        if members.is_none() {
            return Err(Diagnostic::error(
                codes::P005,
                "`flick.toml` needs `[project]`, or `[workspace]` at the root of a workspace",
                Span::new(file, 0, 0),
            ));
        }
        let settings = [
            ("build", raw.build.map(|b| b.span())),
            ("sql", raw.sql.map(|s| s.span())),
            ("lint", raw.lint.map(|l| l.span())),
            ("dependencies", raw.dependencies.map(|d| d.span())),
            ("package", raw.package.map(|p| p.span())),
            ("plugins", raw.plugins.map(|p| p.span())),
        ];
        if let Some((table, at)) = settings.into_iter().find_map(|(t, at)| Some((t, at?))) {
            return Err(Diagnostic::error(
                codes::P005,
                format!("`[{table}]` belongs to a project: this workspace root has no `[project]`"),
                span(at),
            ));
        }
        None
    };
    Ok(Manifest {
        project,
        members,
        scripts,
    })
}

/// The scripts of `[scripts]`; a step `@name` must name one of them.
fn scripts(
    raw: BTreeMap<String, Spanned<RawScript>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Vec<Script>, Diagnostic> {
    let scripts: Vec<(Script, Span)> = raw
        .into_iter()
        .map(|(name, value)| {
            let at = span(value.span());
            let steps = match value.into_inner() {
                RawScript::One(step) => vec![step],
                RawScript::Many(steps) => steps,
            };
            (Script { name, steps }, at)
        })
        .collect();
    for (script, at) in &scripts {
        let unknown = script.steps.iter().find_map(|step| {
            let name = step.strip_prefix('@')?;
            (!scripts.iter().any(|(s, _)| s.name == name)).then_some(name)
        });
        if let Some(name) = unknown {
            return Err(Diagnostic::error(
                codes::P005,
                format!(
                    "script `{}` runs `@{name}`, which `[scripts]` does not name",
                    script.name
                ),
                *at,
            ));
        }
    }
    Ok(scripts.into_iter().map(|(s, _)| s).collect())
}

/// The project of `[project]` with its other tables.
fn config(
    project: Spanned<RawProject>,
    raw: Raw,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Config, Diagnostic> {
    let project_span = span(project.span());
    let project = project.into_inner();
    let entries = entries(project.entry, project.entries, project.runner, span)?;
    if project.kind == Kind::App && entries.is_empty() {
        return Err(Diagnostic::error(
            codes::P007,
            "`kind = \"app\"` needs `entry`, the module that exports `main`",
            project_span,
        )
        .with_fix(|| (
            "name the entry module, e.g. `entry = \"App.Main\"`, or several with `entries = { server = \"App.Server\", cli = \"App.Cli\" }`",
            Vec::new(),
        )));
    }
    let sql = raw.sql.map(Spanned::into_inner).unwrap_or_default();
    let sql_dialect = match sql.dialect {
        None => DIALECTS[0].0,
        Some(name) => sql_dialect(name.get_ref())
            .map_err(|(code, message)| Diagnostic::error(code, message, span(name.span())))?,
    };
    let lint = raw.lint.map(Spanned::into_inner).unwrap_or_default();
    let allow = lint
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
    let dependencies = dependencies(
        raw.dependencies
            .map(Spanned::into_inner)
            .unwrap_or_default(),
        span,
    )?;
    let package = match raw.package {
        Some(raw) => {
            let at = span(raw.span());
            Some(package(raw, at)?)
        }
        None => None,
    };
    let plugins = grants(
        raw.plugins.map(Spanned::into_inner).unwrap_or_default(),
        span,
    )?;
    Ok(Config {
        kind: project.kind,
        source_dir: project.source_dir,
        test_dir: project.test_dir,
        entries,
        dependencies,
        package,
        build: raw.build.map(Spanned::into_inner).unwrap_or_default(),
        sql_dialect,
        allow,
        no_let_assert: lint.no_let_assert,
        thresholds,
        plugins,
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

/// The SQL dialects `[sql] dialect` may name, and whether each is
/// supported yet.
const DIALECTS: [(&str, bool); 5] = [
    ("postgresql", true),
    ("mysql", false),
    ("sqlite", false),
    ("clickhouse", false),
    ("oracle", false),
];

/// The dialect `[sql] dialect` names, or why it cannot be used.
fn sql_dialect(name: &str) -> Result<&'static str, (Code, String)> {
    match DIALECTS.iter().find(|(n, _)| *n == name) {
        Some((n, true)) => Ok(n),
        Some((_, false)) => Err((
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
    DIALECTS
        .iter()
        .filter(|(_, d)| !supported || *d)
        .map(|(n, _)| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn offset(n: usize) -> u32 {
    u32::try_from(n).expect("config offsets fit in u32")
}
