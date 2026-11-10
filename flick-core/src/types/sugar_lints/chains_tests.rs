use crate::diagnostics::codes;
use crate::types::lint_tests::fixed;
use crate::types::lint_tests::{HEAD, assert_fix, check};

const SIG: &str = "f : Option Float -> Option Float -> Option Float\n";

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
    let result = "g : Result String Float -> Result String Float\n";
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
    assert!(!check(&text).iter().any(|d| d.code == codes::T035));
    assert_eq!(fixed(&text, "FLICK-T035"), None);
}

const LOOK: &str =
    "look : Float ->{} Option Float\nlet look n =\n  Platform.stdout_line \"look\"\n  Some n\n\n";

#[test]
fn a_runtime_call_in_a_do_is_bound_before_it() {
    assert_fix(
        &format!(
            "{LOOK}h : Float ->{{}} Option Float\nlet h n =\n  do {{\n    x <- Some n\n    y <- look 2\n    pure (x + y)\n  }}\n"
        ),
        "FLICK-E001",
        &format!(
            "{LOOK}h : Float ->{{}} Option Float\nlet h n =\n  let result = look 2\n  do {{\n    x <- Some n\n    y <- result\n    pure (x + y)\n  }}\n"
        ),
    );
    assert_fix(
        &format!(
            "{LOOK}h : Float ->{{}} Option Float\nlet h n =\n  do {{\n    x <- Some n\n    Platform.stdout_line \"no\"\n    pure x\n  }}\n"
        ),
        "FLICK-E001",
        &format!(
            "{LOOK}h : Float ->{{}} Option Float\nlet h n =\n  Platform.stdout_line \"no\"\n  do {{\n    x <- Some n\n    pure x\n  }}\n"
        ),
    );
}

#[test]
fn a_call_on_a_bound_variable_stays() {
    let text = format!(
        "{HEAD}{LOOK}h : Float ->{{}} Option Float\nlet h n =\n  do {{\n    x <- Some n\n    y <- look x\n    pure (x + y)\n  }}\n"
    );
    assert!(check(&text).iter().any(|d| d.code == codes::E001));
    assert_eq!(fixed(&text, "FLICK-E001"), None);
}
