//! Outline, signature help, type definition, inlay hints, semantic tokens
//! and folding.

use super::tests::fixture_of;
use super::{TOKEN_TYPES, Target};

const SHAPES: &str = "module App.Shapes exposing (..)

type Shape = Circle Number | Rect Number Number | Empty
";

const MAIN: &str = "module App.Main exposing (add3, area, doubled)

import App.Shapes exposing (..)

add3 : Number -> Number -> Number -> Number
let add3 a b c = a + b + c

area : Shape -> Number
let area shape =
  let base = add3 1 2 3
  match shape with
  | Circle r -> r * base
  | _ -> 0

doubled : List Number -> List Number
let doubled xs =
  xs
    |> List.map (fun x -> x * 2)
    |> List.map (fun x -> x + 1)
";

fn at(needle: &str) -> u32 {
    u32::try_from(MAIN.find(needle).expect("occurs")).expect("small")
}

#[test]
fn outline_nests_constructors_and_finds_symbols_by_letters() {
    let f = fixture_of(&[("src/App/Shapes.fl", SHAPES), ("src/App/Main.fl", MAIN)]);
    let ide = f.ide();
    let outline = ide.document_symbols(f.file("App.Shapes"));
    assert_eq!(outline.len(), 1, "{outline:?}");
    let names: Vec<&str> = outline[0]
        .children
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Circle", "Rect", "Empty"]);
    let main: Vec<String> = ide
        .document_symbols(f.file("App.Main"))
        .into_iter()
        .map(|o| o.name)
        .collect();
    assert_eq!(main, ["add3", "area", "doubled"]);
    let found: Vec<String> = ide
        .workspace_symbols("crc")
        .into_iter()
        .map(|(_, o)| o.name)
        .collect();
    assert_eq!(found, ["Circle"]);
}

#[test]
fn signature_help_marks_the_argument_being_written() {
    let f = fixture_of(&[("src/App/Shapes.fl", SHAPES), ("src/App/Main.fl", MAIN)]);
    let help = f
        .ide()
        .signature_help(f.file("App.Main"), at("2 3") + 1)
        .expect("help");
    assert_eq!(help.label, "add3 : Number -> Number -> Number -> Number");
    assert_eq!(help.params.len(), 3);
    assert_eq!(help.active, 1);
    let (s, e) = help.params[1];
    assert_eq!(&help.label[s as usize..e as usize], "Number");
}

#[test]
fn type_definition_and_inlay_hints_come_from_the_checker() {
    let f = fixture_of(&[("src/App/Shapes.fl", SHAPES), ("src/App/Main.fl", MAIN)]);
    let ide = f.ide();
    let main = f.file("App.Main");
    let location = ide
        .type_definition(main, at("shape with") + 1)
        .expect("type definition");
    assert_eq!(location.target, Target::File(f.file("App.Shapes")));
    let hints: Vec<String> = ide.inlay_hints(main).into_iter().map(|(_, l)| l).collect();
    assert_eq!(
        hints,
        [": Number", " : List Number", " : List Number"],
        "{hints:?}"
    );
}

#[test]
fn semantic_tokens_classify_names_and_folding_spans_blocks() {
    let f = fixture_of(&[("src/App/Shapes.fl", SHAPES), ("src/App/Main.fl", MAIN)]);
    let ide = f.ide();
    let main = f.file("App.Main");
    let tokens = ide.semantic_tokens(main);
    let kind_at = |offset: u32| {
        tokens
            .iter()
            .find(|t| t.0 == offset)
            .map(|t| TOKEN_TYPES[t.2 as usize])
    };
    assert_eq!(kind_at(at("add3 1")), Some("function"));
    assert_eq!(kind_at(at("List.map")), Some("namespace"));
    assert_eq!(kind_at(at("Circle r")), Some("enumMember"));
    assert_eq!(kind_at(at("Shape ->")), Some("type"));
    assert_eq!(kind_at(at("r * base") + 4), Some("variable"));
    let folds = ide.folding_ranges(main);
    let matched = at("match shape");
    assert!(folds.iter().any(|(s, _)| *s == matched), "{folds:?}");
}

#[test]
fn the_mode_of_a_foreign_binding_is_a_keyword() {
    let text = "module App.Main exposing (double, now)

double : Number -> Number
foreign pure double from \"./double.js\" \"double\"

now : Unit -> Number
foreign sync now from global \"Date.now\"
";
    let f = fixture_of(&[("src/App/Main.fl", text)]);
    let tokens = f.ide().semantic_tokens(f.file("App.Main"));
    let kind_at = |needle: &str| {
        let at = u32::try_from(text.find(needle).expect("occurs")).expect("small");
        tokens
            .iter()
            .find(|t| t.0 == at)
            .map(|t| TOKEN_TYPES[t.2 as usize])
    };
    assert_eq!(kind_at("pure"), Some("keyword"));
    assert_eq!(kind_at("sync"), Some("keyword"));
    assert_eq!(kind_at("global"), Some("keyword"));
    assert_eq!(kind_at("double from"), Some("function"));
}

#[test]
fn component_is_a_keyword_there_only() {
    let text = "module App.Main exposing (badge)

component badge (render : String) = ui { text render }
";
    let f = fixture_of(&[("src/App/Main.fl", text)]);
    let tokens = f.ide().semantic_tokens(f.file("App.Main"));
    let kind_at = |needle: &str| {
        let at = u32::try_from(text.find(needle).expect("occurs")).expect("small");
        tokens
            .iter()
            .find(|t| t.0 == at)
            .map(|t| TOKEN_TYPES[t.2 as usize])
    };
    assert_eq!(kind_at("component"), Some("keyword"));
    assert_eq!(kind_at("render :"), Some("parameter"));
    assert_eq!(kind_at("render }"), Some("variable"));
}
