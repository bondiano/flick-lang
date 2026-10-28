use super::super::tests::{Fixture, fixture_of};
use super::Action;

const SHAPES: &str = "module App.Shapes exposing (..)

type Shape = Circle Float | Rect Float Float | Empty
";

/// `App.Main` of `main` (with `App.Shapes` beside it) and its text.
fn main(main: &str) -> (Fixture, String) {
    let f = fixture_of(&[("src/App/Shapes.fl", SHAPES), ("src/App/Main.fl", main)]);
    (f, main.to_owned())
}

/// The action titled `title` for the range `start..end` of `App.Main`.
fn action(f: &Fixture, start: u32, end: u32, title: &str) -> Option<Action> {
    let file = f.file("App.Main");
    f.ide()
        .actions(file, start, end)
        .into_iter()
        .find(|a| a.title.starts_with(title))
}

fn applied(text: &str, action: &Action) -> String {
    let mut edits = action.edits.clone();
    edits.sort_by_key(|e| e.span.start);
    let mut out = text.to_owned();
    for e in edits.iter().rev() {
        out.replace_range(e.span.start as usize..e.span.end as usize, &e.replacement);
    }
    out
}

#[test]
fn add_signature_writes_the_inferred_type_of_a_value() {
    let (f, text) = main("module App.Main exposing (limit)\n\nlet limit = [1, 2]\n");
    let at = f.at("App.Main", "limit =", 0);
    let a = action(&f, at, at, "add the signature").expect("action");
    assert_eq!(
        applied(&text, &a),
        "module App.Main exposing (limit)\n\nlimit : List Int\nlet limit = [1, 2]\n"
    );
}

#[test]
fn pattern_match_on_a_variable_lists_its_constructors() {
    let src = "module App.Main exposing (area)\n\nimport App.Shapes exposing (..)\n\narea : Shape -> Float\nlet area shape =\n  shape\n";
    let (f, text) = main(src);
    let at = f.at("App.Main", "  shape", 0) + 2;
    let a = action(&f, at, at, "pattern match on `shape`").expect("action");
    assert_eq!(
        applied(&text, &a),
        "module App.Main exposing (area)\n\nimport App.Shapes exposing (..)\n\narea : Shape -> Float\nlet area shape =\n  match shape with\n    | App.Shapes.Circle _ -> todo\n    | App.Shapes.Rect _ _ -> todo\n    | App.Shapes.Empty -> todo\n"
    );
}

#[test]
fn qualify_and_unqualify_rewrite_every_use() {
    let src = "module App.Main exposing (twice)\n\nimport List exposing (map)\n\ntwice : List Float -> List Float\nlet twice xs = map (fun x -> x * 2) (map (fun x -> x) xs)\n";
    let (f, text) = main(src);
    let at = f.at("App.Main", "map (fun", 0);
    let a = action(&f, at, at, "qualify `map`").expect("qualify");
    let qualified = applied(&text, &a);
    assert_eq!(
        qualified,
        "module App.Main exposing (twice)\n\nimport List\n\ntwice : List Float -> List Float\nlet twice xs = List.map (fun x -> x * 2) (List.map (fun x -> x) xs)\n"
    );
    let (f, text) = main(&qualified);
    let at = f.at("App.Main", "List.map", 0) + 5;
    let a = action(&f, at, at, "import `map`").expect("unqualify");
    assert_eq!(
        applied(&text, &a),
        "module App.Main exposing (twice)\n\nimport List exposing (map)\n\ntwice : List Float -> List Float\nlet twice xs = map (fun x -> x * 2) (map (fun x -> x) xs)\n"
    );
}

#[test]
fn extract_and_inline_round_trip() {
    let src = "module App.Main exposing (total)\n\ntotal : Float -> Float\nlet total n =\n  let base = n + 1\n  base * (n - 2)\n";
    let (f, text) = main(src);
    let start = f.at("App.Main", "n - 2", 0);
    let a = action(&f, start, start + 5, "extract `value`").expect("extract");
    let extracted = applied(&text, &a);
    assert_eq!(
        extracted,
        "module App.Main exposing (total)\n\ntotal : Float -> Float\nlet total n =\n  let base = n + 1\n  let value = n - 2\n  base * value\n"
    );
    let at = f.at("App.Main", "base =", 0);
    let a = action(&f, at, at, "inline `base`").expect("inline");
    assert_eq!(
        applied(&text, &a),
        "module App.Main exposing (total)\n\ntotal : Float -> Float\nlet total n =\n  (n + 1) * (n - 2)\n"
    );
}
