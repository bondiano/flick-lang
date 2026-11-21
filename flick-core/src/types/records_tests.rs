//! Accessors on records of different fields, which the model does not
//! check: its records are exact.

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::SourceMap;

fn check(text: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    crate::check(&sources, file)
}

/// A local accessor is generic over the other fields of a record as well,
/// as an explicit `fun` is. The model rejects one local function applied to
/// records of different fields (`records are exact`); `tests/check/pass`,
/// which it checks too, keeps to records of the same fields.
#[test]
fn a_local_accessor_takes_records_of_different_fields() {
    for get in [".value", "fun r -> r.value"] {
        let text = format!(
            "module Probe exposing (values)\n\nvalues : Unit ->{{}} (Int, Bool)\nlet values () =\n  let get = {get}\n  (get {{ value = 1 }}, get {{ value = true; other = 2 }})\n"
        );
        let diagnostics = check(&text);
        assert!(diagnostics.is_empty(), "{get}: {diagnostics:?}");
        // a record without the field is still rejected
        let missing = text.replace("value = true; other = 2", "other = 2");
        let diagnostics = check(&missing);
        assert!(
            (diagnostics.iter()).any(|d| d.code == codes::T003 && d.severity == Severity::Error),
            "{get}: {diagnostics:?}"
        );
    }
}
