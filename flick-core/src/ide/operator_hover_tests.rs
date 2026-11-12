//! Hover on `|>`, `<|`, `>>` and `<<`: the type there and what they do.

use super::tests::fixture_of;

#[test]
fn operators_hover_with_their_type_and_what_they_do() {
    let text = "module App.Main exposing (..)

inc : Int -> Int
let inc n = n + 1

shown : Int -> String
let shown = inc >> inc >> show

back : Int -> String
let back = show << inc

piped : String
let piped = 1 |> inc |> show

left : String
let left = show <| inc 1
";
    let f = fixture_of(&[("src/App/Main.fl", text)]);
    let ide = f.ide();
    let file = f.file("App.Main");
    for (needle, nth, form, ty, note) in [
        (">>", 0, "f >> g", "Int -> String", "applies `f`, then `g`"),
        (">>", 1, "f >> g", "Int -> String", "evaluated once"),
        ("<<", 0, "g << f", "Int -> String", "as `f >> g` does"),
        ("|>", 0, "x |> f", "Int", "evaluating `x` first"),
        ("|>", 1, "x |> f", "String", "`x |> f` is `f x`"),
        (
            "<|",
            0,
            "f <| x",
            "String",
            "`f a <| x` is the call `f a x`",
        ),
    ] {
        let at = f.at("App.Main", needle, nth);
        let (markdown, range) = ide.hover(file, at + 1).expect(needle);
        assert_eq!(range, (at, at + 2), "{needle} {nth}");
        assert!(
            markdown.contains(&format!("{form} : {ty}")),
            "{needle} {nth}: {markdown}"
        );
        assert!(markdown.contains(note), "{needle} {nth}: {markdown}");
    }
}
