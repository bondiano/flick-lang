//! The JS of patterns.

use super::Labels;
use super::tests::compile_with;

#[test]
fn the_last_arm_of_a_release_build_still_binds_the_names_of_an_or_pattern() {
    let text = "module App exposing (extent)\n\n\
         type Shape = Circle Float | Rect Float Float | Empty\n\n\
         extent : Shape -> Float\n\
         let extent shape =\n  match shape with\n  | Empty -> 0\n  | Circle r | Rect r _ -> r\n";
    for release in [false, true] {
        let js = compile_with(text, release, Labels::default()).code;
        // the name is given its value in the test of each alternative
        assert_eq!(js.matches("r = shape._0").count(), 2, "{js}");
    }
    // the last arm needs no test in a release build when it binds nothing there: one `if`, of the first arm
    let js = compile_with(
        &text.replace("Circle r | Rect r _ -> r", "_ -> 1"),
        true,
        Labels::default(),
    )
    .code;
    assert_eq!(js.matches("if (").count(), 1, "{js}");
}
