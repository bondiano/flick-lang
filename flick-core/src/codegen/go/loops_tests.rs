//! Mutual tail-call loops in Go codegen.

use super::tests::app;

#[test]
fn mutually_recursive_functions_of_different_arities_compile() {
    let go = app("module App exposing (main)\n\n\
         rec {\n\
         \x20 f : Int -> Int\n\
         \x20 let f x = g x 0\n\n\
         \x20 g : Int -> Int -> Int\n\
         \x20 let g x y = if y == 0 then f x else y\n\
         }\n\n\
         main : Unit ->{Host.Console} Unit\n\
         let main () = Platform.stdout_line (show (f 3))\n");
    assert!(
        go.contains(
            "func G_App__f(fb *flickrt.Fiber, state int, v0, v1 flickrt.Value) flickrt.Value"
        ),
        "{go}"
    );
    assert!(go.contains("G_App__f(fb, 0, l_x, nil)"), "{go}");
    assert!(go.contains("G_App__f(fb, 1, l_x, l_y)"), "{go}");
    assert!(go.contains("v1 = int32(0)"), "{go}");
    assert!(go.contains("state = 1"), "{go}");
    assert!(go.contains("state = 0"), "{go}");
    assert!(!go.contains("flickrt.Check(fb)"), "{go}");
}

#[test]
fn a_grouped_function_with_a_unit_or_wildcard_parameter_passes_nil_to_its_group() {
    let go = app("module App exposing (main)\n\n\
         rec {\n\
         \x20 start : Unit -> Int\n\
         \x20 let start () = step 0 1\n\n\
         \x20 step : Int -> Int -> Int\n\
         \x20 let step _ acc = if acc == 0 then start () else acc\n\
         }\n\n\
         main : Unit ->{Host.Console} Unit\n\
         let main () = Platform.stdout_line (show (start ()))\n");
    assert!(!go.contains("G_App__start(fb, 0, _"), "{go}");
    assert!(!go.contains("G_App__start(fb, 1, _"), "{go}");
    assert!(go.contains("G_App__start(fb, 0, nil, nil)"), "{go}");
    assert!(go.contains("G_App__start(fb, 1, nil, l_acc)"), "{go}");
}

#[test]
fn an_effectful_mutual_group_checkpoints_each_iteration() {
    let go = app("module App exposing (main)\n\n\
         rec {\n\
         \x20 log_one : Int ->{Host.Console} Int\n\
         \x20 let log_one n =\n\
         \x20   if n <= 0 then 0\n\
         \x20   else\n\
         \x20     Platform.stdout_line (show n)\n\
         \x20     log_two (n - 1) n\n\n\
         \x20 log_two : Int -> Int ->{Host.Console} Int\n\
         \x20 let log_two n acc = if n <= 0 then acc else log_one (n - 1)\n\
         }\n\n\
         main : Unit ->{Host.Console} Unit\n\
         let main () = Platform.stdout_line (show (log_one 2))\n");
    let group = go
        .split("func G_App__log_one(")
        .nth(1)
        .and_then(|rest| rest.split("\nfunc ").next())
        .expect("effectful group function");
    assert!(group.contains("flickrt.Check(fb)"), "{group}");
}

#[test]
fn locals_of_one_grouped_member_do_not_leak_into_another() {
    let go = app("module App exposing (main)\n\n\
         type Box = Box { a : Int; b : Int }\n\n\
         rec {\n\
         \x20 first : Box -> Int -> Int\n\
         \x20 let first box n =\n\
         \x20   let x = n + 1\n\
         \x20   match box with\n\
         \x20   | Box r -> second (x, r.a) { z = 9; a = r.b } (n - 1)\n\n\
         \x20 second : (Int, Int) -> { z : Int; a : Int } -> Int -> Int\n\
         \x20 let second pair r n =\n\
         \x20   match pair with\n\
         \x20   | (x, y) -> if n <= 0 then x + y + r.a else first (Box { a = x; b = r.a }) n\n\
         }\n\n\
         main : Unit ->{Host.Console} Unit\n\
         let main () = Platform.stdout_line (show (first (Box { a = 1; b = 2 }) 1))\n");
    let second_case = go.split("case 1:").nth(1).expect("second member case");
    // `x` in `second` is a `flickrt.Value` bound by a tuple pattern, not `first`'s `int32`
    assert!(second_case.contains("flickrt.AsInt(l_x)"), "{second_case}");
    // `r` in `second` is a plain record, not `first`'s constructor-held record `Box`
    assert!(
        second_case.contains("flickrt.Field(l_r, \"a\")"),
        "{second_case}"
    );
    assert!(
        !second_case.contains("flickrt.FieldAt(l_r,"),
        "{second_case}"
    );
}
