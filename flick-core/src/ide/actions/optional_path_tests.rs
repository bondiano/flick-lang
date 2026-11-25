//! Rewrites of optional steps: a path written as its `match`es, checked
//! clean and with the same type; `.?` added or removed only where the value
//! before the step is, or is not, an `Option`; and `fun x -> x.?a` and
//! `.?a` rewritten into each other.

use super::tests::{action, applied, main};
use crate::diagnostics::Severity;

const PROGRAM: &str = "module App.Main exposing (..)

type alias Address = { city : Option String; street : String }

type alias User = { name : String; address : Option Address }

city : Option User -> Option (Option String)
let city maybe_user = maybe_user.?address.?city

street : User -> Option String
let street user = user.address.?street

cities : List User -> List (Option (Option String))
let cities users = List.map (fun u -> u.address.?city) users
";

/// The program after the action titled `title` at `needle` plus `shift`,
/// checked to have no errors.
fn rewritten(text: &str, needle: &str, shift: u32, title: &str) -> String {
    let (f, text) = main(text);
    let at = f.at("App.Main", needle, 0) + shift;
    let a = action(&f, at, at, title).unwrap_or_else(|| panic!("{title} at {needle}"));
    assert_eq!(a.kind, "refactor.rewrite");
    let fixed = applied(&text, &a);
    let checked = main(&fixed).0;
    let errors: Vec<_> = (checked.ide().analysis.diagnostics.iter())
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{fixed}\n{errors:?}");
    fixed
}

#[test]
fn a_path_expands_to_its_matches() {
    let fixed = rewritten(PROGRAM, ".?city", 1, "expand `.?` to `match`");
    assert!(
        fixed.contains(
            "let city maybe_user = match maybe_user with
  | None -> None
  | Some value ->
      match value.address with
        | None -> None
        | Some value2 ->
            Some value2.city
"
        ),
        "{fixed}"
    );
    let fixed = rewritten(PROGRAM, ".?street", 1, "expand `.?` to `match`");
    assert!(
        fixed.contains(
            "let street user = match user.address with
  | None -> None
  | Some value ->
      Some value.street
"
        ),
        "{fixed}"
    );
}

#[test]
fn a_step_gains_or_loses_its_question_mark_by_the_value_before_it() {
    let text = PROGRAM.replace("user.address.?street", "user.address.street");
    let (f, _) = main(&text);
    let at = f.at("App.Main", ".street", 0) + 1;
    let added = action(&f, at, at, "read `.?street`: the path becomes an `Option`")
        .expect("offered on an `Option`");
    assert_eq!(added.kind, "refactor.rewrite");
    // the value before `.?address` is a `User`, and before `.address` no `Option`
    let text = PROGRAM.replace("user.address.?street", "user.?address.?street");
    let (f, text) = main(&text);
    let at = f.at("App.Main", ".?address.?street", 0) + 1;
    let removed = action(
        &f,
        at,
        at,
        "read `.address` without `.?`: the value before it is no `Option`",
    )
    .expect("offered on a record");
    assert!(applied(&text, &removed).contains("user.address.?street"));
    assert!(action(&f, at, at, "read `.?address`: the path becomes an `Option`").is_none());
    // a step of an `Option` keeps its `?`
    let (f, _) = main(PROGRAM);
    let at = f.at("App.Main", ".?city", 0) + 1;
    assert!(
        action(
            &f,
            at,
            at,
            "read `.city` without `.?`: the value before it is no `Option`"
        )
        .is_none()
    );
}

#[test]
fn a_function_reading_an_optional_path_is_an_accessor_and_back() {
    let fixed = rewritten(PROGRAM, "fun u", 0, "use the accessor `.address.?city`");
    assert!(fixed.contains("List.map (.address.?city) users"), "{fixed}");
    let back = rewritten(
        &fixed,
        ".address.?city) users",
        1,
        "write `.address.?city` as a function",
    );
    assert!(
        back.contains("List.map (fun x -> x.address.?city) users"),
        "{back}"
    );
}
