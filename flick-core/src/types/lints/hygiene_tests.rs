use crate::types::lint_tests::{HEAD, assert_fix, fixed};

const MAP: &str = "inc : Number -> Number\nlet inc n = n + 1\n\nf : List Number -> List Number\n";

#[test]
fn a_fun_that_passes_its_parameter_is_the_function() {
    assert_fix(
        &format!("{MAP}let f xs = List.map (fun x -> inc x) xs\n"),
        "FLICK-T052",
        &format!("{MAP}let f xs = List.map inc xs\n"),
    );
    let used = format!("{HEAD}{MAP}let f xs = List.map (fun x -> inc (x + x)) xs\n");
    assert_eq!(fixed(&used, "FLICK-T052"), None);
}

#[test]
fn two_literals_are_one() {
    assert_fix(
        "f : String\nlet f = \"Hello, \" ++ \"world\"\n",
        "FLICK-T053",
        "f : String\nlet f = \"Hello, world\"\n",
    );
}
