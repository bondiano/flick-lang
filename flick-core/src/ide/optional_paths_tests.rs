//! Optional steps `.?label` in the editor: the record completed after
//! `.?` is the one inside the `Option`, for rooted paths, accessors and a
//! bare `.?`; hover on `.?`; and the `operator` token.

use super::TOKEN_TYPES;
use super::tests::{Fixture, fixture_of};

const MAIN: &str = "module App.Main exposing (..)

type alias Address = { city : Option String; zip : String }

type alias User = { name : String; address : Option Address }

city : User -> Option (Option String)
let city user = user.address.?city

cities : List User -> List (Option (Option String))
let cities users = List.map .address.?city users

first : Option User -> Option (Option String)
let first maybe = maybe |> .?address.?city
";

fn main(text: &str) -> Fixture {
    fixture_of(&[("src/App/Main.fl", text)])
}

/// The labels completed after the `.` at `needle` plus `shift`.
fn completed(text: &str, needle: &str, shift: u32) -> Vec<String> {
    let f = main(text);
    let at = f.at("App.Main", needle, 0) + shift;
    assert_eq!(&text[at as usize..at as usize + 2], ".?", "{needle}");
    let mut labels: Vec<String> = (f.ide().complete(f.file("App.Main"), at, true).into_iter())
        .map(|c| c.label)
        .collect();
    labels.sort();
    labels
}

#[test]
fn completion_after_an_optional_step_reads_inside_the_option() {
    let address = ["city", "zip"];
    let user = ["address", "name"];
    let text = MAIN.replace("user.address.?city", "user.address.?");
    assert_eq!(completed(&text, ".?\n", 0), address);
    let text = MAIN.replace(".address.?city users", ".address.? users");
    assert_eq!(completed(&text, ".? users", 0), address);
    let text = MAIN.replace("maybe |> .?address.?city", "maybe |> .?address.?");
    assert_eq!(completed(&text, ".?\n", 0), address);
    let text = MAIN.replace("maybe |> .?address.?city", "maybe |> .?");
    assert_eq!(completed(&text, ".?\n", 0), user);
    let text = MAIN.replace("maybe |> .?address.?city", "maybe.?");
    assert_eq!(completed(&text, ".?\n", 0), user);
}

#[test]
fn an_optional_step_is_an_operator_with_its_hover() {
    let f = main(MAIN);
    let file = f.file("App.Main");
    let at = f.at("App.Main", ".?city", 0);
    let (shown, range) = f.ide().hover(file, at + 1).expect("a hover");
    assert_eq!(range, (at, at + 2));
    assert!(
        shown.starts_with("```flick\n.?city : Option Address\n```"),
        "{shown}"
    );
    let tokens = f.ide().semantic_tokens(file);
    assert!(
        tokens.contains(&(at, at + 2, 10)) && TOKEN_TYPES[10] == "operator",
        "{tokens:?}"
    );
    assert!(tokens.contains(&(at + 2, at + 6, 9)), "{tokens:?}");
}
