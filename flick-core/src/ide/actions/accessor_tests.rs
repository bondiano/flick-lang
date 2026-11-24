//! `fun x -> x.a.b` and `.a.b` rewritten into each other: offered only
//! where the meaning stays, and the result checks clean.

use super::tests::{action, applied, main};
use crate::diagnostics::Severity;

const PROGRAM: &str = "module App.Main exposing (..)

type alias Address = { city : String }

type alias User = { name : String; address : Address }

cities : List User -> List String
let cities users = List.map (fun u -> u.address.city) users

get_city : User -> String
let get_city = .address.city

named : List User -> List String
let named users =
  let x = 1
  users |> List.map .name |> List.map (fun s -> s ++ show x)
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

fn offered(text: &str, needle: &str, shift: u32, title: &str) -> bool {
    let (f, _) = main(text);
    let at = f.at("App.Main", needle, 0) + shift;
    action(&f, at, at, title).is_some()
}

#[test]
fn a_function_reading_a_path_of_its_parameter_becomes_an_accessor() {
    let fixed = rewritten(PROGRAM, "fun u", 1, "use the accessor `.address.city`");
    assert!(
        fixed.contains("let cities users = List.map (.address.city) users\n"),
        "{fixed}"
    );
}

#[test]
fn an_accessor_becomes_a_function_with_a_fresh_parameter() {
    let fixed = rewritten(
        PROGRAM,
        ".address.city\n",
        1,
        "write `.address.city` as a function",
    );
    assert!(
        fixed.contains("let get_city = fun x -> x.address.city\n"),
        "{fixed}"
    );
    // `x` is a local there: the parameter takes another name, and the
    // function is parenthesized before what follows it
    let fixed = rewritten(PROGRAM, ".name |>", 1, "write `.name` as a function");
    assert!(
        fixed.contains("users |> List.map (fun x1 -> x1.name) |> List.map"),
        "{fixed}"
    );
}

#[test]
fn the_rewrites_undo_each_other() {
    let function = rewritten(PROGRAM, ".address.city\n", 1, "write");
    let back = rewritten(&function, "fun x", 1, "use the accessor");
    assert_eq!(back, PROGRAM);
}

/// A function that does more than read a path of its one parameter, or
/// whose rewrite would lose an annotation or a comment, is left.
#[test]
fn other_functions_are_left() {
    for (from, to) in [
        (
            "fun u -> u.address.city",
            "fun (u : User) -> u.address.city",
        ),
        ("fun u -> u.address.city", "fun u -> (u.address).city"),
        (
            "fun u -> u.address.city",
            "fun u -> /* where */ u.address.city",
        ),
        (
            "fun u -> u.address.city",
            "fun u -> u.address.city ++ u.name",
        ),
        (
            "(fun u -> u.address.city) users",
            "(fun u v -> u.address.city) users users",
        ),
        ("fun u -> u.address.city", "fun u -> u.name ++ \"\""),
    ] {
        let text = PROGRAM.replace(from, to);
        assert!(!offered(&text, "fun ", 1, "use the accessor"), "{to}");
    }
}

#[test]
fn an_unfinished_accessor_is_left() {
    let text = PROGRAM.replace("let get_city = .address.city", "let get_city = .address.");
    assert!(!offered(&text, ".address.", 1, "write"));
}
