//! The Go of a `match` through its decision graph: the arms of one
//! constructor test it once, an arm reached from several branches is
//! written once, and a match whose graph would be too big tries its arms
//! one by one.

use super::tests::app;

const SHAPE: &str = "module App exposing (main)\n\n\
     type Shape =\n  | Dot\n  | Line Int\n  | Pair Shape Shape\n\n";

#[test]
fn the_arms_of_one_constructor_test_it_once() {
    let go = app(&format!(
        "{SHAPE}name : Shape -> String\nlet name s =\n  match s with\n  \
         | Pair Dot _ -> \"dot first\"\n  | Pair (Line n) _ -> show n\n  \
         | Pair (Pair _ _) Dot -> \"nested\"\n  | _ -> \"other\"\n\n\
         main : Unit ->{{Host.Console; AppError}} Unit\nlet main () = Platform.stdout_line (name Dot)\n"
    ));
    // one test of `Pair` for its three arms, then of its first field
    assert_eq!(go.matches("Is_T_App__Pair(l_s)").count(), 1, "{go}");
    assert_eq!(
        go.matches("Is_T_App__Line(l_s.(*T_App__Pair).f[0])")
            .count(),
        1,
        "{go}"
    );
    // the catch-all is written once: the tests that fail fall to it
    assert_eq!(go.matches("return \"other\"").count(), 1, "{go}");
    assert!(!go.contains("goto arm_"), "{go}");
}

#[test]
fn a_match_too_big_for_a_graph_tries_its_arms_in_turn() {
    let either = "(Dot | Line _)";
    let all = [either; 12].join(", ");
    let shapes = ["Shape"; 12].join(", ");
    let dots = ["Dot"; 12].join(", ");
    let go = app(&format!(
        "{SHAPE}many : ({shapes}) -> Int\n\
         let many t =\n  match t with\n  | ({all}) -> 1\n  | _ -> 2\n\n\
         main : Unit ->{{Host.Console; AppError}} Unit\n\
         let main () = Platform.stdout_line (show (many ({dots})))\n"
    ));
    // an or-pattern tried as a function, as the arms one by one test it
    assert!(go.contains("func(_ *flickrt.Fiber) bool"), "{go}");
    assert!(!go.contains("goto arm_"), "{go}");
}

#[test]
fn an_arm_on_several_branches_is_written_once_and_gone_to() {
    let go = app(&format!(
        "{SHAPE}first : Shape -> Int\nlet first s =\n  match s with\n  \
         | Line n | Pair (Line n) _ when n > 0 -> n\n  | _ -> 0\n\n\
         main : Unit ->{{Host.Console; AppError}} Unit\nlet main () = Platform.stdout_line (show (first Dot))\n"
    ));
    // the guard under `Line` and under `Pair`, one body; `n` stays native `int32`
    assert_eq!(go.matches("goto arm_").count(), 2, "{go}");
    assert_eq!(go.matches("return l_n").count(), 1, "{go}");
    assert!(go.contains("var l_n int32\n"), "{go}");
    assert!(!go.contains("flickrt.AsInt(l_n)"), "{go}");
}
