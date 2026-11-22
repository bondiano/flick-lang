//! Scalar unboxed Go representation tests for `codegen::go`.

use super::app;

#[test]
fn a_scalar_stays_in_its_go_type_through_a_choice_and_a_test() {
    let go = app("module App exposing (main)\n\n\
         spread : Int -> Int -> String -> Int\n\
         let spread a b word =\n  \
         let wide = if a > b && not (word < \"m\") then a - b else b - a\n  \
         let mark =\n    match word with\n    | \"none\" -> 0\n    | _ -> 1\n  \
         wide * 2 + mark\n\n\
         main : Unit ->{Host.Console; AppError} Unit\n\
         let main () = Platform.stdout_line (show (spread 5 2 \"word\"))\n");
    // the result of an `if` and of a `match` whose branches are numbers is a number
    assert!(
        go.contains("var t1 int32\n") && go.contains("var t2 int32\n"),
        "{go}"
    );
    assert!(
        go.contains("var l_wide int32 = t1") && go.contains("var l_mark int32 = t2"),
        "{go}"
    );
    // a condition Go holds as a `bool` is tested as it is, strings are
    // ordered by the runtime with no value made of them, and a literal in a
    // pattern is compared as Go compares
    assert!(
        go.contains("if ((l_a > l_b) && !(flickrt.CompareStrings(l_word, \"m\") < 0)) {"),
        "{go}"
    );
    assert!(go.contains("if (l_word == \"none\") {"), "{go}");
    for call in [
        "flickrt.Bool(",
        "flickrt.Not(",
        "flickrt.Same(",
        "flickrt.Lt(",
    ] {
        assert!(!go.contains(call), "{call} in {go}");
    }
}

const TYPED: &str = "module App exposing (main)\n\n\
fib : Int -> Int\nlet rec fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)\n\n\
first : a -> b -> a\nlet first x _ = x\n\n\
shout : Int -> List String -> Int\n\
let shout n words = n + List.length (List.map (fun word -> word ++ \"!\") words)\n\n\
tick : Int ->{Host.Console} Int\nlet tick n =\n  Platform.stdout_line \"tick\"\n  n + 1\n\n\
main : Unit ->{Host.Console; AppError} Unit\n\
let main () =\n  \
let folded = List.fold_left first 0 [fib 5, shout 1 [\"a\"]]\n  \
Platform.stdout_line (show (tick folded))\n";

#[test]
fn a_function_takes_and_returns_numbers_in_their_go_type() {
    let go = app(TYPED);
    // a number is passed and returned as it is: no value is made for a call
    assert!(
        go.contains("func F_App__fib(fb *flickrt.Fiber, l_n int32) int32 {"),
        "{go}"
    );
    assert!(
        go.contains("F_App__fib(fb, flickrt.SubInt(l_n, int32(1))),"),
        "{go}"
    );
    assert!(!go.contains("flickrt.AsInt(F_App__fib("), "{go}");
    // so is one of a function that runs effects
    assert!(
        go.contains("func F_App__tick(fb *flickrt.Fiber, l_n int32) int32 {"),
        "{go}"
    );
    // a function of values is still a value as it is
    assert!(go.contains("flickrt.Fn2(F_App__first)"), "{go}");
}

#[test]
fn a_function_inside_a_function_of_a_go_type_returns_its_own_values() {
    let go = app(TYPED);
    // `shout` returns an `int32`; the function it passes on returns a text,
    // as a value, and is not made to return a number
    assert!(
        go.contains(
            "func F_App__shout(fb *flickrt.Fiber, l_n int32, l_words flickrt.Value) int32 {"
        ),
        "{go}"
    );
    assert!(go.contains("return flickrt.Concat(l_word, \"!\")"), "{go}");
}
