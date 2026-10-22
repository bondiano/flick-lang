use serde::Serialize;

use crate::diagnostics::{Diagnostic, Severity};
use crate::source::{SourceMap, Span};

/// Version of the NDJSON event schema on stderr (`--diagnostics json`).
pub const SCHEMA_VERSION: u32 = 1;

/// One JSON object per line. Other event types (`log`, `application-stderr`,
/// `package-manager-output`) share the envelope once the CLI emits them.
pub fn render_json(sources: &SourceMap, diagnostics: &[Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| {
            let event = Event {
                schema_version: SCHEMA_VERSION,
                kind: "diagnostic",
                code: d.code.to_string(),
                severity: match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Note => "note",
                },
                message: &d.message,
                path: d.span.map(|s| sources.get(s.file).path.as_str()),
                range: d.span.map(Range::from),
                related: d
                    .related
                    .iter()
                    .map(|r| Located {
                        path: &sources.get(r.span.file).path,
                        range: r.span.into(),
                        message: &r.message,
                    })
                    .collect(),
                fixes: d
                    .fixes
                    .iter()
                    .map(|f| JsonFix {
                        message: &f.message,
                        edits: f
                            .edits
                            .iter()
                            .map(|e| JsonEdit {
                                path: &sources.get(e.span.file).path,
                                range: e.span.into(),
                                replacement: &e.replacement,
                            })
                            .collect(),
                    })
                    .collect(),
            };
            serde_json::to_string(&event).expect("diagnostic event serializes") + "\n"
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct Event<'a> {
    schema_version: u32,
    #[serde(rename = "type")]
    kind: &'static str,
    code: String,
    severity: &'static str,
    message: &'a str,
    path: Option<&'a str>,
    range: Option<Range>,
    related: Vec<Located<'a>>,
    fixes: Vec<JsonFix<'a>>,
}

/// UTF-8 byte range, end exclusive.
#[derive(Serialize)]
struct Range {
    start: u32,
    end: u32,
}

impl From<Span> for Range {
    fn from(span: Span) -> Self {
        Range {
            start: span.start,
            end: span.end,
        }
    }
}

#[derive(Serialize)]
struct Located<'a> {
    path: &'a str,
    range: Range,
    message: &'a str,
}

#[derive(Serialize)]
struct JsonFix<'a> {
    message: &'a str,
    edits: Vec<JsonEdit<'a>>,
}

#[derive(Serialize)]
struct JsonEdit<'a> {
    path: &'a str,
    range: Range,
    replacement: &'a str,
}
