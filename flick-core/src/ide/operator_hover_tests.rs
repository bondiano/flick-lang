//! Hover on `|>`, `<|`, `>>` and `<<`: the type there and what they do.

use super::CompletionKind;
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

#[test]
fn orelse_hover_shows_result_type_laziness_and_parsed_grouping() {
    let text = "module App.Main exposing (..)

find : Int -> Option String
let find n = if n > 0 then Some (show n) else None

simple : Option String -> String
let simple host = host orelse \"localhost\"

chained : Option Int -> Option Int -> Int
let chained a b = a orelse b orelse 0

mixed_right : Int -> String
let mixed_right n = n |> find orelse \"none\"

mixed_left : Option Int -> String
let mixed_left opt = show <| opt orelse 0
";
    let f = fixture_of(&[("src/App/Main.fl", text)]);
    let ide = f.ide();
    let file = f.file("App.Main");

    let at_simple = f.at("App.Main", "orelse \"localhost\"", 0);
    let (md, range) = ide.hover(file, at_simple + 2).expect("simple orelse");
    assert_eq!(range, (at_simple, at_simple + 6));
    assert!(md.contains("option orelse fallback : String"), "{md}");
    assert!(md.contains("without evaluating `fallback`"), "{md}");
    assert!(!md.contains("Parsed as"), "{md}");

    let at_chain = f.at("App.Main", "orelse b", 0);
    let (md, _) = ide.hover(file, at_chain + 2).expect("chained orelse");
    assert!(md.contains("option orelse fallback : Int"), "{md}");
    assert!(md.contains("Parsed as `a orelse (b orelse 0)`."), "{md}");

    let at_mix_r = f.at("App.Main", "orelse \"none\"", 0);
    let (md, _) = ide.hover(file, at_mix_r + 2).expect("mixed |> orelse");
    assert!(md.contains("option orelse fallback : String"), "{md}");
    assert!(
        md.contains("Parsed as `(n |> find) orelse \"none\"`."),
        "{md}"
    );

    let at_pipe_r = f.at("App.Main", "|> find", 0);
    let (md, _) = ide.hover(file, at_pipe_r + 1).expect("mixed |>");
    assert!(
        md.contains("Parsed as `(n |> find) orelse \"none\"`."),
        "{md}"
    );

    let at_mix_l = f.at("App.Main", "orelse 0", 1);
    let (md, _) = ide.hover(file, at_mix_l + 2).expect("mixed <| orelse");
    assert!(md.contains("option orelse fallback : Int"), "{md}");
    assert!(md.contains("Parsed as `show <| (opt orelse 0)`."), "{md}");

    let at_pipe_l = f.at("App.Main", "<| opt", 0);
    let (md, _) = ide.hover(file, at_pipe_l + 1).expect("mixed <|");
    assert!(md.contains("Parsed as `show <| (opt orelse 0)`."), "{md}");
}

#[test]
fn orelse_completion_ranks_unwrapped_type_and_keeps_record_fields_and_tokens_clean() {
    let partial = "module App.Main exposing (pick)\n\n\
                   default_host : String\nlet default_host = \"localhost\"\n\n\
                   default_port : Int\nlet default_port = 8080\n\n\
                   pick : Option String -> String\nlet pick host = host orelse \n";
    let f = fixture_of(&[("src/App/Main.fl", partial)]);
    let ide = f.ide();
    let main = f.file("App.Main");
    let cursor = f.at("App.Main", "orelse ", 0) + 7;
    let items = ide.complete(main, cursor, false);
    let host_sort = items
        .iter()
        .find(|c| c.label == "default_host")
        .expect("default_host")
        .sort;
    let port_sort = items
        .iter()
        .find(|c| c.label == "default_port")
        .expect("default_port")
        .sort;
    assert!(
        host_sort < port_sort,
        "String candidate must rank ahead of Int after `Option String orelse `: host={host_sort}, port={port_sort}"
    );

    // Record field named `orelse` completes after `.` and semantic tokens expose no synthetic binder.
    let valid = "module App.Main exposing (read)\n\n\
                 type alias Box = { orelse : Option Int; name : String }\n\n\
                 read : Box -> Int\nlet read box = box.orelse orelse 0\n";
    let f = fixture_of(&[("src/App/Main.fl", valid)]);
    let ide = f.ide();
    let main = f.file("App.Main");
    let fields: Vec<_> = ide
        .complete(main, f.at("App.Main", "box.orelse", 0) + 3, true)
        .into_iter()
        .map(|c| (c.label, c.kind))
        .collect();
    assert!(
        fields.contains(&("orelse".to_owned(), CompletionKind::Field)),
        "{fields:?}"
    );
    let tokens = ide.semantic_tokens(main);
    let orelse_op = f.at("App.Main", "orelse 0", 0);
    assert!(
        !tokens
            .iter()
            .any(|&(s, e, _)| s == orelse_op && e == orelse_op + 6),
        "operator `orelse` should not be emitted as a variable semantic token"
    );
    assert!(
        tokens
            .iter()
            .all(|&(s, e, _)| !valid[s as usize..e as usize].contains('$')),
        "no synthetic binder in semantic tokens"
    );
}
