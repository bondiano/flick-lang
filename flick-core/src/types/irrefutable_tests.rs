use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::SourceMap;

const HEAD: &str = "module Probe exposing (f)\n\n";

fn check(text: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    crate::check(&sources, file)
}

/// `text` after the fix titled `title` of its `FLICK-T009`.
fn fixed(text: &str, title: &str) -> String {
    let diagnostics = check(text);
    let fix = diagnostics
        .iter()
        .filter(|d| d.code.to_string() == "FLICK-T009")
        .flat_map(|d| &d.fixes)
        .find(|f| f.message == title)
        .unwrap_or_else(|| panic!("no fix {title:?} in {diagnostics:?}"));
    let mut out = text.to_owned();
    for edit in fix.edits.iter().rev() {
        out.replace_range(
            edit.span.start as usize..edit.span.end as usize,
            &edit.replacement,
        );
    }
    out
}

/// Fixes `body` with `title` and checks that the result compiles.
fn assert_fix(body: &str, title: &str, want: &str) {
    let text = format!("{HEAD}{body}");
    let diagnostics = check(&text);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == codes::T009 && d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    let out = fixed(&text, title);
    assert_eq!(out, format!("{HEAD}{want}"));
    let errors: Vec<_> = check(&out)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn a_list_let_becomes_a_match_over_the_rest_of_its_block() {
    assert_fix(
        "f : List Float -> Float\nlet f xs =\n  let [x, y] = xs\n  let s = x + y\n\n  s * 2\n",
        "use `match`",
        "f : List Float -> Float\nlet f xs =\n  match xs with\n  | [x, y] ->\n    let s = x + y\n\n    s * 2\n  | _ -> todo\n",
    );
    assert_fix(
        "f : List Float -> Float\nlet f xs =\n  let [x, ...rest] = xs // head\n  x\n",
        "use `match`",
        "f : List Float -> Float\nlet f xs =\n  match xs with\n  | [x, ...rest] -> // head\n    x\n  | [] -> todo\n",
    );
}

#[test]
fn constructor_and_record_lets_get_the_missing_arms() {
    assert_fix(
        "f : Option Float -> Float\nlet f value =\n  let Some n = value\n  n\n",
        "use `match`",
        "f : Option Float -> Float\nlet f value =\n  match value with\n  | Some n ->\n    n\n  | None -> todo\n",
    );
    assert_fix(
        "f : { label : String; size : Float } -> Float\nlet f r =\n  let { label = \"big\"; size } = r\n  size\n",
        "use `match`",
        "f : { label : String; size : Float } -> Float\nlet f r =\n  match r with\n  | { label = \"big\"; size } ->\n    size\n  | _ -> todo\n",
    );
}

#[test]
fn a_list_literal_let_can_become_a_tuple() {
    assert_fix(
        "f : Float\nlet f =\n  let [x, y] = [1, 2]\n  x + y\n",
        "use a tuple",
        "f : Float\nlet f =\n  let (x, y) = (1, 2)\n  x + y\n",
    );
}

#[test]
fn a_refutable_let_can_become_a_let_assert() {
    assert_fix(
        "f : List Float -> Float\nlet f xs =\n  let [x, ...rest] = xs\n  x\n",
        "use `let assert`",
        "f : List Float -> Float\nlet f xs =\n  let assert [x, ...rest] = xs\n  x\n",
    );
}

#[test]
fn an_assert_that_cannot_fail_becomes_a_let() {
    let text = format!(
        "{HEAD}f : (Float, Float) -> Float\nlet f pair =\n  let assert (a, b) = pair\n  a + b\n"
    );
    let d = check(&text)
        .into_iter()
        .find(|d| d.code.to_string() == "FLICK-T025")
        .expect("T025");
    assert_eq!(d.severity, Severity::Warning);
    let mut out = text.clone();
    for edit in d.fixes[0].edits.iter().rev() {
        out.replace_range(
            edit.span.start as usize..edit.span.end as usize,
            &edit.replacement,
        );
    }
    assert_eq!(out, text.replace("let assert (a, b)", "let (a, b)"));
}

#[test]
fn no_match_fix_without_a_rest_of_the_block() {
    let text = format!("{HEAD}f : Option Float -> Float\nlet f (Some n) = n\n");
    let fixes: Vec<_> = check(&text).into_iter().flat_map(|d| d.fixes).collect();
    assert!(
        fixes.iter().all(|f| f.message != "use `match`"),
        "{fixes:?}"
    );
}
