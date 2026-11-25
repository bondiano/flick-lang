//! Optional steps `.?label` over the protocol: completion after `.?` from
//! the record inside the `Option`, hover on `.?`, the `operator` token for
//! it, and renaming a field read through it.

use std::path::PathBuf;

use super::*;

const DOMAIN: &str = "module App.Domain exposing (..)

type alias Address = { city : Option String; zip : String }

type alias User = { name : String; address : Option Address }
";

const MAIN: &str = "module App.Main exposing (..)

import App.Domain exposing (User)

city : User -> Option (Option String)
let city user = user.address.?city

cities : List User -> List (Option (Option String))
let cities users = List.map .address.?city users

first : Option User -> Option (Option String)
let first maybe = maybe |> .?address.?city
";

fn open(name: &str, main_text: &str) -> (Lsp, PathBuf, PathBuf) {
    let dir = project(
        name,
        &[
            ("flick.toml", LIB),
            ("src/App/Domain.fl", DOMAIN),
            ("src/App/Main.fl", main_text),
        ],
    );
    let (main, domain) = (dir.join("src/App/Main.fl"), dir.join("src/App/Domain.fl"));
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&main), "languageId": "flick", "version": 1, "text": main_text } }));
    assert_eq!(lsp.diagnostics(&uri(&main)), Vec::<Value>::new());
    (lsp, main, domain)
}

fn labels(reply: &Value) -> Vec<String> {
    let mut out: Vec<String> = (reply["result"].as_array())
        .unwrap_or_else(|| panic!("{reply}"))
        .iter()
        .filter_map(|i| i["label"].as_str().map(str::to_owned))
        .collect();
    out.sort();
    out
}

/// After `.?` the fields are those of the record inside the `Option`, in a
/// rooted path and in an accessor alike; `?` alone completes nothing.
#[test]
fn completion_after_an_optional_step_reads_inside_the_option() {
    let (mut lsp, main, _) = open("lsp_optional_completion", MAIN);
    let doc = json!({ "uri": uri(&main) });
    for (version, (from, written, want)) in (2..).zip([
        ("user.address.?city", "user.address.?", vec!["city", "zip"]),
        (
            ".address.?city users",
            ".address.? users",
            vec!["city", "zip"],
        ),
        (
            "maybe |> .?address.?city",
            "maybe |> .?",
            vec!["address", "name"],
        ),
    ]) {
        let text = MAIN.replace(from, written);
        lsp.notify(
            "textDocument/didChange",
            &json!({ "textDocument": { "uri": uri(&main), "version": version }, "contentChanges": [{ "text": text }] }),
        );
        let at = text.find(written).expect("occurs") + written.trim_end_matches(" users").len();
        let reply = lsp.request(
            "textDocument/completion",
            &json!({ "textDocument": doc, "position": position(&text, at) }),
        );
        assert_eq!(labels(&reply), want, "{written}");
    }
    let at = MAIN.find("first maybe").expect("occurs") + "first maybe".len();
    let text = format!("{}?{}", &MAIN[..at], &MAIN[at..]);
    lsp.notify(
        "textDocument/didChange",
        &json!({ "textDocument": { "uri": uri(&main), "version": 9 }, "contentChanges": [{ "text": text }] }),
    );
    let reply = lsp.request(
        "textDocument/completion",
        &json!({ "textDocument": doc, "position": position(&text, at + 1) }),
    );
    assert_eq!(labels(&reply), Vec::<String>::new());
    lsp.close();
}

#[test]
fn an_optional_step_is_an_operator_with_its_own_hover() {
    let (mut lsp, main, _) = open("lsp_optional_hover", MAIN);
    let doc = json!({ "uri": uri(&main) });
    let mark = MAIN.find(".?city").expect("occurs");
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": doc, "position": position(MAIN, mark + 1) }),
    );
    assert_eq!(
        hover["result"]["range"],
        json!({ "start": position(MAIN, mark), "end": position(MAIN, mark + 2) })
    );
    let shown = hover["result"]["contents"]["value"]
        .as_str()
        .expect("hover");
    assert!(
        shown.starts_with("```flick\n.?city : Option Address\n```"),
        "{shown}"
    );
    assert!(shown.contains("never returns from the function"), "{shown}");
    let init = lsp.request("initialize", &json!({}));
    let legend =
        init["result"]["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"].clone();
    let triggers =
        init["result"]["capabilities"]["completionProvider"]["triggerCharacters"].clone();
    assert_eq!(triggers, json!([".", "?"]));
    let reply = lsp.request(
        "textDocument/semanticTokens/full",
        &json!({ "textDocument": doc }),
    );
    let data = reply["result"]["data"].as_array().expect("tokens");
    let (mut line, mut column, mut decoded) = (0, 0, Vec::new());
    for token in data.chunks_exact(5) {
        let delta = token[0].as_u64().expect("line delta");
        line += delta;
        column = if delta == 0 { column } else { 0 } + token[1].as_u64().expect("column");
        let kind = usize::try_from(token[3].as_u64().expect("kind")).expect("small");
        let length = token[2].as_u64().expect("length");
        decoded.push((
            line,
            column,
            length,
            legend[kind].as_str().expect("kind").to_owned(),
        ));
    }
    for (needle, shift, length, kind) in [
        ("user.address.?city", 12, 2, "operator"),
        ("user.address.?city", 14, 4, "property"),
        (".?address.?city", 0, 2, "operator"),
        (".?address.?city", 2, 7, "property"),
    ] {
        let at = position(MAIN, MAIN.find(needle).expect("occurs") + shift);
        let want = (
            at["line"].as_u64().expect("line"),
            at["character"].as_u64().expect("column"),
            length,
            kind.to_owned(),
        );
        assert!(decoded.contains(&want), "{needle}: {want:?} in {decoded:?}");
    }
    lsp.close();
}

/// Renaming a field renames the labels optional steps read it by. An
/// accessor `List.map` is given before its list types its record late,
/// which no alias is shown to be, as for a plain step.
#[test]
fn a_field_read_through_an_optional_step_is_renamed() {
    let text = MAIN.replace(
        "cities : List User -> List (Option (Option String))\nlet cities users = List.map .address.?city users\n\n",
        "",
    );
    let (mut lsp, main, _) = open("lsp_optional_rename", &text);
    let doc = json!({ "uri": uri(&main) });
    let at = text.find("user.address.?city").expect("occurs") + 14;
    let reply = lsp.request(
        "textDocument/rename",
        &json!({ "textDocument": doc, "position": position(&text, at), "newName": "town" }),
    );
    let mut edits: Vec<(u64, u64)> = reply["result"]["changes"][uri(&main)]
        .as_array()
        .unwrap_or_else(|| panic!("{reply}"))
        .iter()
        .map(|e| {
            let start = &e["range"]["start"];
            (
                start["line"].as_u64().expect("line"),
                start["character"].as_u64().expect("column"),
            )
        })
        .collect();
    edits.sort_unstable();
    let want: Vec<(u64, u64)> = text
        .match_indices("?city")
        .map(|(i, _)| {
            let p = position(&text, i + 1);
            (
                p["line"].as_u64().expect("line"),
                p["character"].as_u64().expect("column"),
            )
        })
        .collect();
    assert_eq!(edits, want, "{reply}");
    lsp.close();
}
