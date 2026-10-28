use crate::diagnostics::codes;
#[cfg(feature = "diagnostic-fixes")]
use crate::types::lint_tests::fixed;
use crate::types::lint_tests::{HEAD, assert_fix, check};

const CMP: &str = "f : Float -> Float -> Bool\n";
const FLAG: &str = "f : Bool -> String\n";

#[test]
fn an_if_of_true_and_false_is_its_condition() {
    assert_fix(
        &format!("{CMP}let f a b = if a == b then true else false\n"),
        "FLICK-T040",
        &format!("{CMP}let f a b = a == b\n"),
    );
    assert_fix(
        &format!("{CMP}let f a b = if a < b then false else true\n"),
        "FLICK-T040",
        &format!("{CMP}let f a b = not (a < b)\n"),
    );
}

#[test]
fn an_if_not_swaps_its_branches() {
    assert_fix(
        &format!("{FLAG}let f done = if not done then \"pending\" else \"done\"\n"),
        "FLICK-T041",
        &format!("{FLAG}let f done = if done then \"done\" else \"pending\"\n"),
    );
    // `else ()` is left to T028
    #[cfg(feature = "diagnostic-fixes")]
    let unit = format!("{HEAD}f : Bool -> Unit\nlet f c = if not c then () else ()\n");
    #[cfg(feature = "diagnostic-fixes")]
    assert_eq!(fixed(&unit, "FLICK-T041"), None);
}

#[test]
fn an_if_alone_in_an_else_joins_the_chain() {
    assert_fix(
        "f : Float -> String\nlet f n =\n  if n > 0 then \"+\"\n  else\n    if n < 0 then \"-\" else \"0\"\n",
        "FLICK-T042",
        "f : Float -> String\nlet f n =\n  if n > 0 then \"+\"\n  else if n < 0 then \"-\" else \"0\"\n",
    );
}

#[test]
fn a_literal_moves_to_the_right_of_a_comparison() {
    assert_fix(
        "f : Float -> Bool\nlet f n = 10 >= n\n",
        "FLICK-T043",
        "f : Float -> Bool\nlet f n = n <= 10\n",
    );
    #[cfg(feature = "diagnostic-fixes")]
    let both = format!("{HEAD}f : Bool\nlet f = 1 == 2\n");
    #[cfg(feature = "diagnostic-fixes")]
    assert_eq!(fixed(&both, "FLICK-T043"), None);
}

#[test]
fn a_known_condition_keeps_its_branch_only_on_request() {
    let text = format!("{HEAD}f : Float\nlet f = if 1 > 2 then 1 else 2\n");
    assert!(check(&text).iter().any(|d| d.code == codes::T045));
    #[cfg(feature = "diagnostic-fixes")]
    assert_eq!(
        fixed(&text, "FLICK-T045").as_deref(),
        Some(format!("{HEAD}f : Float\nlet f = 2\n").as_str())
    );
}

#[test]
fn a_match_on_a_bool_is_an_if() {
    assert_fix(
        &format!("{FLAG}let f b = match b with\n  | false -> \"no\"\n  | _ -> \"yes\"\n"),
        "FLICK-T046",
        &format!("{FLAG}let f b = if b then \"yes\" else \"no\"\n"),
    );
}
