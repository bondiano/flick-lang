//! Diagnostics in LSP terms and the notification that publishes them.

use std::path::Path;

use flick_core::diagnostics::{Diagnostic, Severity};
use flick_core::source::SourceMap;
use serde_json::{Value, json};

use super::protocol;

pub fn publish(path: &Path, diagnostics: &[Value]) -> Value {
    protocol::notification(
        "textDocument/publishDiagnostics",
        &json!({ "uri": protocol::uri_of(path), "diagnostics": diagnostics }),
    )
}

/// A diagnostic in LSP terms; its code keeps the `FLICK-` spelling.
pub fn lsp_diagnostic(sources: &SourceMap, d: &Diagnostic) -> Value {
    let span = d.span.expect("only located diagnostics are published");
    let text = &sources.get(span.file).text;
    let severity = match d.severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Note => 3,
    };
    let related: Vec<Value> = d
        .related
        .iter()
        .map(|r| {
            let file = sources.get(r.span.file);
            json!({
                "location": {
                    "uri": protocol::uri_of(Path::new(&file.path)),
                    "range": protocol::range(&file.text, r.span.start, r.span.end),
                },
                "message": r.message,
            })
        })
        .collect();
    // fixes without edits are advice; editors show them under the message
    let message = d
        .fixes
        .iter()
        .filter(|f| f.edits.is_empty())
        .fold(d.message.clone(), |m, f| {
            format!("{m}\nhelp: {}", f.message)
        });
    json!({
        "range": protocol::range(text, span.start, span.end),
        "severity": severity,
        "code": d.code.to_string(),
        "source": "flick",
        "message": message,
        "relatedInformation": related,
        // 1 is `Unnecessary`: editors grey the code out
        "tags": if d.unnecessary() { json!([1]) } else { json!([]) },
    })
}
