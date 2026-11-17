use crate::diagnostics::codes;
use crate::types::lint_tests::{HEAD, assert_fix, check};

const SIG: &str = "type error NotFound = NotFound Int\n\nfind : Int -> Option String\nlet find id = None\n\nname : Int ->{; NotFound} String\n";

#[test]
fn from_result_of_err_becomes_fail() {
    assert_fix(
        &format!(
            "{SIG}let name id =\n  match find id with\n  | Some found -> found\n  | None -> from_result (Err (NotFound id))\n"
        ),
        "FLICK-T075",
        &format!(
            "{SIG}let name id =\n  match find id with\n  | Some found -> found\n  | None -> Fail.fail (NotFound id)\n"
        ),
    );
}

/// A `match` that fails with `Fail.fail e` is `r?` as well.
#[test]
fn a_match_failing_with_fail_is_a_question_mark() {
    let sig = "type error Invalid = Invalid String\n\nparse : String -> Result Invalid Float\nlet parse s = Ok 0.0\n\ntotal : String ->{; Invalid} Float\n";
    assert_fix(
        &format!(
            "{sig}let total s =\n  match parse s with\n  | Ok n -> n\n  | Err e -> Fail.fail e\n"
        ),
        "FLICK-T034",
        &format!("{sig}let total s =\n  (parse s)?\n"),
    );
}

/// A `Result` that is not built right there is no `Err` to raise.
#[test]
fn from_result_of_a_value_is_left() {
    let text = format!(
        "{HEAD}type error NotFound = NotFound Int\n\nlookup : Result NotFound Int -> Int ->{{; NotFound}} Int\nlet lookup r id = from_result r\n"
    );
    assert!(check(&text).iter().all(|d| d.code != codes::T075));
}
