//! A composition is a function of the types of what it composes.

use super::tests::app;

#[test]
fn a_composition_takes_and_returns_numbers_in_their_go_type() {
    let go = app("module App exposing (main)\n\n\
         inc : Int -> Int\nlet inc n = n + 1\n\n\
         composed : Int -> Int\nlet composed = inc >> inc\n\n\
         written : Int -> Int\nlet written = fun x -> inc (inc x)\n\n\
         main : Unit ->{} Unit\n\
         let main () = Platform.stdout_line (show (composed 1 + written 1))\n");
    assert!(
        go.contains("func F_App__composed(fb *flickrt.Fiber, l__x24_x int32) int32 {"),
        "{go}"
    );
    assert!(
        go.contains("return F_App__inc(fb, F_App__inc(fb, l__x24_x))"),
        "{go}"
    );
    assert!(
        go.contains("func F_App__written(fb *flickrt.Fiber, l_x int32) int32 {"),
        "{go}"
    );
}
