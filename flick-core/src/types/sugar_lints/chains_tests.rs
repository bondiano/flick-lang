use crate::diagnostics::{Diagnostic, Severity};
use crate::source::SourceMap;

const HEAD: &str = "module Probe exposing (..)\n\n";

fn check(text: &str) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    crate::check(&sources, file)
}

/// `text` after the fixes of its diagnostics coded `code`, if any.
fn fixed(text: &str, code: &str) -> Option<String> {
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
fn assert_fix(body: &str, code: &str, want: &str) {
    let out = fixed(&format!("{HEAD}{body}"), code).unwrap_or_else(|| panic!("no {code} fix"));
    assert_eq!(out, format!("{HEAD}{want}"));
    let errors: Vec<_> = check(&out)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

const SIG: &str = "f : Option Number -> Option Number -> Option Number\n";

#[test]
fn nested_binds_become_one_do() {
    assert_fix(
        &format!(
            "{SIG}let f a b = Option.bind (fun x -> Option.bind (fun y -> Some (x + y)) b) a\n"
        ),
        "FLICK-T035",
        &format!("{SIG}let f a b = do {{ x <- a; y <- b; Some (x + y) }}\n"),
    );
    assert_fix(
        &format!(
            "{SIG}let f a b = a |> flat_map (fun x -> b |> flat_map (fun y -> pure (x * y)))\n"
        ),
        "FLICK-T035",
        &format!("{SIG}let f a b = do {{ x <- a; y <- b; pure (x * y) }}\n"),
    );
}

#[test]
fn matches_that_pass_the_failure_on_are_steps() {
    assert_fix(
        &format!(
            "{SIG}let f a b =\n  match a with\n  | Some x -> Option.bind (fun y -> Some (x - y)) b\n  | None -> None\n"
        ),
        "FLICK-T035",
        &format!("{SIG}let f a b =\n  do {{ x <- a; y <- b; Some (x - y) }}\n"),
    );
    let result = "g : Result String Number -> Result String Number\n";
    assert_fix(
        &format!(
            "{result}let g r =\n  match r with\n  | Err e -> Err e\n  | Ok x -> Result.bind (fun y -> Ok (x + y)) (Ok 1)\n"
        ),
        "FLICK-T035",
        &format!("{result}let g r =\n  do {{ x <- r; y <- Ok 1; Ok (x + y) }}\n"),
    );
}

#[test]
fn a_chain_in_an_argument_keeps_its_parentheses() {
    assert_fix(
        &format!(
            "{SIG}let f a b = Option.with_default 0 (Option.bind (fun x -> Option.bind (fun y -> Some y) b) a) |> Some\n"
        ),
        "FLICK-T035",
        &format!(
            "{SIG}let f a b = Option.with_default 0 (do {{ x <- a; y <- b; Some y }}) |> Some\n"
        ),
    );
}

#[test]
fn one_step_is_left_alone() {
    let text = format!("{HEAD}{SIG}let f a b = Option.bind (fun x -> Some (x + 1)) a\n");
    assert_eq!(fixed(&text, "FLICK-T035"), None);
}

const LOOK: &str =
    "look : Number ->{} Option Number\nlet look n =\n  Platform.stdout_line \"look\"\n  Some n\n\n";

#[test]
fn a_runtime_call_in_a_do_is_bound_before_it() {
    assert_fix(
        &format!(
            "{LOOK}h : Number ->{{}} Option Number\nlet h n =\n  do {{\n    x <- Some n\n    y <- look 2\n    pure (x + y)\n  }}\n"
        ),
        "FLICK-E001",
        &format!(
            "{LOOK}h : Number ->{{}} Option Number\nlet h n =\n  let result = look 2\n  do {{\n    x <- Some n\n    y <- result\n    pure (x + y)\n  }}\n"
        ),
    );
    assert_fix(
        &format!(
            "{LOOK}h : Number ->{{}} Option Number\nlet h n =\n  do {{\n    x <- Some n\n    Platform.stdout_line \"no\"\n    pure x\n  }}\n"
        ),
        "FLICK-E001",
        &format!(
            "{LOOK}h : Number ->{{}} Option Number\nlet h n =\n  Platform.stdout_line \"no\"\n  do {{\n    x <- Some n\n    pure x\n  }}\n"
        ),
    );
}

#[test]
fn a_call_on_a_bound_variable_stays() {
    let text = format!(
        "{HEAD}{LOOK}h : Number ->{{}} Option Number\nlet h n =\n  do {{\n    x <- Some n\n    y <- look x\n    pure (x + y)\n  }}\n"
    );
    assert_eq!(fixed(&text, "FLICK-E001"), None);
}
