//! `flick.toml`: the schema version, `[project]` and `[build]`. Unknown
//! keys are errors so that a typo never silently means a default.

use serde::Deserialize;
use toml::Spanned;

use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};

pub const SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub kind: Kind,
    pub source_dir: String,
    pub test_dir: String,
    /// Entry module of an app, with the span of its value.
    pub entry: Option<(String, Span)>,
    pub build: Build,
}

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
    pub schema_aot: bool,
    /// Instance resolution budget per definition.
    pub solver_steps: u32,
    pub solver_depth: u32,
}

impl Default for Build {
    fn default() -> Self {
        Build {
            out_dir: "dist".into(),
            profile: Profile::Debug,
            emit: Emit::Js,
            emit_dts: false,
            schema_aot: false,
            solver_steps: crate::types::Limits::default().steps,
            solver_depth: crate::types::Limits::default().depth,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Debug,
    Release,
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
    let span =
        |range: std::ops::Range<usize>| Span::new(file, offset(range.start), offset(range.end));
    let invalid = |err: toml::de::Error| {
        let message = format!("invalid `flick.toml`: {}", err.message().trim_end());
        let diagnostic = Diagnostic::new(codes::P005, crate::diagnostics::Severity::Error, message);
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
    let entry = project.entry.map(|e| (e.get_ref().clone(), span(e.span())));
    if project.kind == Kind::App && entry.is_none() {
        return Err(Diagnostic::error(
            codes::P007,
            "`kind = \"app\"` needs `entry`, the module that exports `main`",
            project_span,
        )
        .with_fix(
            "name the entry module, e.g. `entry = \"App.Main\"`",
            Vec::new(),
        ));
    }
    Ok(Config {
        kind: project.kind,
        source_dir: project.source_dir,
        test_dir: project.test_dir,
        entry,
        build: raw.build,
    })
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
             emit-dts = false\nschema-aot = false\n",
        )
        .expect("valid");
        assert_eq!(config.kind, Kind::App);
        assert_eq!(config.entry.map(|e| e.0).as_deref(), Some("App.Main"));
        assert_eq!(config.build, Build::default());
        let lib = parse_text("schema-version = 1\n[project]\nkind = \"lib\"\n").expect("valid");
        assert_eq!(
            (lib.source_dir.as_str(), lib.test_dir.as_str()),
            ("src", "test")
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
            ("[project]\nkind = \"lib\"\n", codes::P006, ""),
            (
                "schema-version = 1\n[build]\nprofile = \"fast\"\n[project]\nkind = \"lib\"\n",
                codes::P005,
                "\"fast\"",
            ),
            ("schema-version = 1\n[project\n", codes::P005, ""),
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
