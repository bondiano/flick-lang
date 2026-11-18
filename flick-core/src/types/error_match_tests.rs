use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::SourceMap;

fn check(text: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    crate::check(&sources, file)
}

/// The fix of the `FLICK-T009` of `text`: its title, and `text` after it.
fn fixed(text: &str) -> (String, String) {
    let diagnostics = check(text);
    let fix = diagnostics
        .iter()
        .filter(|d| d.code == codes::T009)
        .flat_map(|d| &d.fixes)
        .next()
        .unwrap_or_else(|| panic!("no fix of FLICK-T009 in {diagnostics:?}"));
    let mut out = text.to_owned();
    for edit in fix.edits.iter().rev() {
        out.replace_range(
            edit.span.start as usize..edit.span.end as usize,
            &edit.replacement,
        );
    }
    (fix.message.clone(), out)
}

fn errors(text: &str) -> Vec<Diagnostic> {
    check(text)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

/// An error of the row without an arm is the arm the fix adds, named as
/// the module writes it: qualified when it is of another module.
#[test]
fn the_fix_adds_the_arm_of_an_error_the_match_misses() {
    let text = "module Probe exposing (f)\n\nimport HttpApi\nimport Rpc\n\n\
                type error Gone = Gone String\n\n\
                f : Errors {HttpApi.ServeError, Rpc.ServeError, Gone} -> String\n\
                let f e =\n  match e with\n  | HttpApi.ServeError _ -> \"api\"\n  | Gone _ -> \"gone\"\n";
    let (title, out) = fixed(text);
    assert_eq!(title, "add an arm `| Rpc.ServeError _ -> todo`");
    assert!(
        out.ends_with("  | Gone _ -> \"gone\"\n  | Rpc.ServeError _ -> todo\n"),
        "{out}"
    );
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
}
