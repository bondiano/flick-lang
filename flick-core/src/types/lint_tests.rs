//! Helpers for the tests of the lints and their fixes.

use crate::diagnostics::Diagnostic;
use crate::diagnostics::Severity;
use crate::source::SourceMap;

pub const HEAD: &str = "module Probe exposing (..)\n\n";

pub fn check(text: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    crate::check(&sources, file)
}

/// `text` after the fixes of its diagnostics coded `code`, if any.
pub fn fixed(text: &str, code: &str) -> Option<String> {
    let diagnostics = check(text);
    let mut edits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.to_string() == code)
        .flat_map(|d| d.fixes.iter().flat_map(|f| &f.edits))
        .collect();
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|e| e.span.start);
    let mut out = text.to_owned();
    for edit in edits.iter().rev() {
        out.replace_range(
            edit.span.start as usize..edit.span.end as usize,
            &edit.replacement,
        );
    }
    Some(out)
}

/// Fixes the `code` diagnostics of `body` and checks the result compiles.
pub fn assert_fix(body: &str, code: &str, want: &str) {
    let text = format!("{HEAD}{body}");
    let out = fixed(&text, code).unwrap_or_else(|| panic!("no {code} fix"));
    assert_eq!(out, format!("{HEAD}{want}"));
    let errors: Vec<_> = check(&out)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn syntax_recovery_does_not_depend_on_optional_fix_text() {
    use crate::diagnostics::codes::{self, Area};
    let diagnostics = check("module Probe exposing (..)\nlet f () = missing\nlet broken = )\n");
    assert!(
        diagnostics.iter().any(|d| d.code.area == Area::Syntax),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T001),
        "{diagnostics:?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code == codes::T004),
        "{diagnostics:?}"
    );
}
