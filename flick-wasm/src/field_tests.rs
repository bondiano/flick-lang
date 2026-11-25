//! Field paths in the playground: completion after each `.` of a path,
//! hover, tokens and rewrites answer as the language server's queries do
//! on the same text, at UTF-16 offsets after non-ASCII text.

use serde_json::{Value, json};

use flick_core::ide::{Ide, TOKEN_TYPES};

use crate::{Session, utf16_offset};

const SOURCE: &str = "module Main exposing (main)

type alias Address = { city : String; zip : String }

type alias User = { name : String; address : Address }

cities : List User -> List String
let cities users = List.map .address. users

/// «Привет» 🎉
greet : User -> String
let greet user = \"Привет 🎉 \" ++ user.name ++ (user |> .address.city)

main : Unit -> Unit
let main () = ()
";

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("JSON")
}

fn units(at: usize) -> u32 {
    utf16_offset(SOURCE, u32::try_from(at).expect("small"))
}

/// The native queries over the session's own analysis, at byte offsets.
fn native(session: &Session) -> Ide<'_> {
    Ide {
        analysis: &session.analysis,
        sources: &session.sources,
    }
}

#[test]
fn field_paths_answer_as_the_native_queries() {
    let mut session = Session::new();
    let _ = session.update(SOURCE);
    let file = session.file().expect("the module");
    let ide = native(&session);

    // after the `.` of an unfinished step, the fields its record has
    let dot = SOURCE.find(". users").expect("occurs");
    let items = json(&session.complete(units(dot), true));
    let labels: Vec<&str> = (items.as_array().expect("items").iter())
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert_eq!(labels, ["city", "zip"]);
    let natively: Vec<String> = (ide
        .complete(file, u32::try_from(dot).expect("small"), true)
        .into_iter())
    .map(|c| c.label)
    .collect();
    assert_eq!(labels, natively);

    // hover on a label after non-ASCII text, its range in UTF-16 units
    let name = SOURCE.find("user.name").expect("occurs") + 5;
    let hover = json(&session.hover(units(name + 1)));
    let (text, range) = ide
        .hover(file, u32::try_from(name + 1).expect("small"))
        .expect("hover");
    assert_eq!(
        hover,
        json!({ "text": text, "from": units(name), "to": units(name + 4) })
    );
    assert_eq!(range.0 as usize, name);

    // the labels of a path are properties, at UTF-16 offsets
    let tokens = json(&session.semantic_tokens());
    let properties: Vec<(u64, u64)> = (tokens["tokens"].as_array().expect("tokens").iter())
        .filter(|t| t["type"] == "property")
        .map(|t| {
            (
                t["from"].as_u64().expect("from"),
                t["to"].as_u64().expect("to"),
            )
        })
        .collect();
    let city = SOURCE.find(".address.city)").expect("occurs") + 9;
    for (start, len) in [(name, 4), (city, 4)] {
        let want = (u64::from(units(start)), u64::from(units(start + len)));
        assert!(properties.contains(&want), "{want:?} in {properties:?}");
    }
    let shown = (ide.semantic_tokens(file).into_iter())
        .filter(|(_, _, k)| TOKEN_TYPES[*k as usize] == "property")
        .count();
    assert_eq!(properties.len(), shown);

    // the rewrite of an accessor, its edit at UTF-16 offsets
    let accessor = SOURCE.find(".address.city)").expect("occurs");
    let at = units(accessor + 1);
    let actions = json(&session.code_actions(at, at));
    let rewrite = (actions.as_array().expect("actions").iter())
        .find(|a| a["title"] == "write `.address.city` as a function")
        .unwrap_or_else(|| panic!("{actions}"));
    assert_eq!(
        rewrite["edits"],
        json!([{ "from": units(accessor), "to": units(accessor + 13), "text": "(fun x -> x.address.city)" }])
    );
}

const OPTIONAL: &str = "module Main exposing (main)

type alias Address = { city : Option String; zip : String }

type alias User = { name : String; address : Option Address }

/// «Привет» 🎉
cities : List User -> List (Option (Option String))
let cities users = List.map .address.? users

city : User -> Option (Option String)
let city user = user.address.?city

main : Unit -> Unit
let main () = ()
";

/// After `.?` the playground completes the record inside the `Option`, and
/// `.?` is an operator, at UTF-16 offsets after non-ASCII text.
#[test]
fn optional_steps_answer_as_the_native_queries() {
    let units = |at: usize| utf16_offset(OPTIONAL, u32::try_from(at).expect("small"));
    let mut session = Session::new();
    let _ = session.update(OPTIONAL);
    let dot = OPTIONAL.find(".? users").expect("occurs");
    let items = json(&session.complete(units(dot), true));
    let labels: Vec<&str> = (items.as_array().expect("items").iter())
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert_eq!(labels, ["city", "zip"]);
    let mark = OPTIONAL.find(".?city").expect("occurs");
    let tokens = json(&session.semantic_tokens());
    let operator = (tokens["tokens"].as_array().expect("tokens").iter()).any(|t| {
        t["type"] == "operator"
            && t["from"].as_u64() == Some(u64::from(units(mark)))
            && t["to"].as_u64() == Some(u64::from(units(mark + 2)))
    });
    assert!(operator, "{tokens}");
}
