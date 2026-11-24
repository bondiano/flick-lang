//! Record fields and accessors over the protocol: completion on the text
//! of the latest edit, hover and tokens at UTF-16 positions after
//! non-ASCII text, renaming a field across modules in one workspace edit
//! or refusing it, definitions and the accessor rewrites.

use std::path::PathBuf;

use super::*;

const DOMAIN: &str = "module App.Domain exposing (..)

type alias Address = { city : String; zip : String }

type alias User = { name : String; address : Address }

type alias Product = { name : String; price : Float }

make : String -> Address -> User
let make n a = { name = n; address = a }

label : Product -> String
let label p = p.name
";

const MAIN: &str = "module App.Main exposing (..)

import App.Domain exposing (User)

greet : User -> String
let greet user = \"Привет, 🎉 \" ++ user.name

cities : List User -> List String
let cities users = users |> List.map .address.city

shout : User -> String
let shout u = u |> .name
";

struct Open {
    lsp: Lsp,
    main: PathBuf,
    domain: PathBuf,
}

fn open(name: &str, main: &str) -> Open {
    let dir = project(
        name,
        &[
            ("flick.toml", LIB),
            ("src/App/Domain.fl", DOMAIN),
            ("src/App/Main.fl", main),
        ],
    );
    let (main_path, domain) = (dir.join("src/App/Main.fl"), dir.join("src/App/Domain.fl"));
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&main_path), "languageId": "flick", "version": 1, "text": main } }));
    assert_eq!(lsp.diagnostics(&uri(&main_path)), Vec::<Value>::new());
    Open {
        lsp,
        main: main_path,
        domain,
    }
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

/// Each edit completes the fields of the record its own text reads.
#[test]
fn completion_follows_the_latest_edit() {
    let Open { mut lsp, main, .. } = open("lsp_fields_completion", MAIN);
    let doc = json!({ "uri": uri(&main) });
    for (version, (written, needle, want)) in (2..).zip([
        (".address.", ".address.", ["city", "zip"]),
        (".addr", ".addr", ["address", "name"]),
        (".address.z", ".address.z", ["city", "zip"]),
    ]) {
        let text = MAIN.replace(".address.city", written);
        lsp.notify(
            "textDocument/didChange",
            &json!({ "textDocument": { "uri": uri(&main), "version": version }, "contentChanges": [{ "text": text }] }),
        );
        let at = text.find(needle).expect("occurs") + needle.len();
        let reply = lsp.request(
            "textDocument/completion",
            &json!({ "textDocument": doc, "position": position(&text, at) }),
        );
        assert_eq!(labels(&reply), want, "{written}");
    }
    lsp.close();
}

#[test]
fn hover_and_tokens_count_utf16_units() {
    let Open { mut lsp, main, .. } = open("lsp_fields_utf16", MAIN);
    let doc = json!({ "uri": uri(&main) });
    let name = MAIN.find("user.name").expect("occurs") + 5;
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": doc, "position": position(MAIN, name + 1) }),
    );
    assert_eq!(
        hover["result"]["range"],
        json!({ "start": position(MAIN, name), "end": position(MAIN, name + 4) })
    );
    let shown = hover["result"]["contents"]["value"]
        .as_str()
        .expect("hover");
    assert_eq!(
        shown,
        "```flick\nname : String\n```\n\nfield of `User` from `App.Domain`"
    );
    let init = lsp.request("initialize", &json!({}));
    let legend =
        init["result"]["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"].clone();
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
        ("user.name", 5, 4, "property"),
        (".address.city", 1, 7, "property"),
        (".address.city", 9, 4, "property"),
        ("List.map .address", 0, 4, "namespace"),
        ("u |> .name", 6, 4, "property"),
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
    // nothing of an accessor is a module path
    let accessor = position(MAIN, MAIN.find(".address.city").expect("occurs"));
    let namespaces = (decoded.iter())
        .filter(|(l, c, _, k)| {
            *l == accessor["line"].as_u64().expect("line")
                && *c >= accessor["character"].as_u64().expect("column")
                && k == "namespace"
        })
        .count();
    assert_eq!(namespaces, 0, "{decoded:?}");
    lsp.close();
}

/// The edits of `reply` for the document at `path`, in order.
fn edited(reply: &Value, path: &Path) -> Vec<Value> {
    let mut edits: Vec<Value> = reply["result"]["changes"][uri(path)]
        .as_array()
        .cloned()
        .unwrap_or_default();
    edits.sort_by_key(|e| {
        (
            e["range"]["start"]["line"].as_u64(),
            e["range"]["start"]["character"].as_u64(),
        )
    });
    edits
}

fn edit(text: &str, at: usize, old: &str, new: &str) -> Value {
    json!({ "range": { "start": position(text, at), "end": position(text, at + old.len()) }, "newText": new })
}

#[test]
fn a_field_is_renamed_across_modules_or_not_at_all() {
    let Open {
        mut lsp,
        main,
        domain,
    } = open("lsp_fields_rename", MAIN);
    let doc = json!({ "uri": uri(&main) });
    let shout = MAIN.find("u |> .name").expect("occurs") + 6;
    let reply = lsp.request(
        "textDocument/rename",
        &json!({ "textDocument": doc, "position": position(MAIN, shout), "newName": "full_name" }),
    );
    assert_eq!(
        edited(&reply, &main),
        [
            edit(
                MAIN,
                MAIN.find("user.name").expect("occurs") + 5,
                "name",
                "full_name"
            ),
            edit(MAIN, shout, "name", "full_name"),
        ],
        "{reply}"
    );
    // `Product`'s field of the same spelling is no part of it
    assert_eq!(
        edited(&reply, &domain),
        [
            edit(
                DOMAIN,
                DOMAIN.find("{ name : String; address").expect("occurs") + 2,
                "name",
                "full_name"
            ),
            edit(
                DOMAIN,
                DOMAIN.find("{ name = n").expect("occurs") + 2,
                "name",
                "full_name"
            ),
        ],
        "{reply}"
    );
    // a label of a record no alias is shown to be: refused, with why
    let address = MAIN.find(".address.city").expect("occurs") + 1;
    let refused = lsp.request(
        "textDocument/prepareRename",
        &json!({ "textDocument": doc, "position": position(MAIN, address) }),
    );
    assert_eq!(
        refused["error"]["message"],
        "cannot rename `address`: its record is written out, and records of other declarations with these fields are of its type; it may not be one declaration's field",
        "{refused}"
    );
    lsp.close();
}

/// A rename the workspace cannot account for sends no edit at all.
#[test]
fn a_refused_rename_edits_nothing() {
    let main =
        format!("{MAIN}\nnames : List User -> List String\nlet names us = List.map .name us\n");
    let Open {
        mut lsp,
        main: path,
        ..
    } = open("lsp_fields_refused", &main);
    let at = main.find("user.name").expect("occurs") + 5;
    let reply = lsp.request(
        "textDocument/rename",
        &json!({ "textDocument": { "uri": uri(&path) }, "position": position(&main, at), "newName": "full_name" }),
    );
    assert!(reply["result"].is_null(), "{reply}");
    let message = reply["error"]["message"].as_str().expect("why");
    assert!(
        message.starts_with("cannot rename the field `name` of `User`: `name` at ")
            && message.contains("Main.fl:15 may read it"),
        "{message}"
    );
    lsp.close();
}

#[test]
fn definitions_and_rewrites_of_an_accessor() {
    let main = format!("{MAIN}\ntype alias Place = {{ city : String; zip : String }}\n");
    let Open {
        mut lsp,
        main: path,
        domain,
    } = open("lsp_fields_definition", &main);
    let doc = json!({ "uri": uri(&path) });
    // read of a record written out: each record type with its fields
    let city = main.find(".address.city").expect("occurs") + 10;
    let reply = lsp.request(
        "textDocument/definition",
        &json!({ "textDocument": doc, "position": position(&main, city) }),
    );
    let mut targets: Vec<(String, Value)> = (reply["result"].as_array())
        .unwrap_or_else(|| panic!("{reply}"))
        .iter()
        .map(|l| {
            (
                l["uri"].as_str().expect("uri").to_owned(),
                l["range"]["start"].clone(),
            )
        })
        .collect();
    targets.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        targets,
        [
            (
                uri(&domain),
                position(DOMAIN, DOMAIN.find("city").expect("occurs"))
            ),
            (
                uri(&path),
                position(&main, main.find("city : String; zip").expect("occurs"))
            ),
        ]
    );
    let at = position(&main, main.find("u |> .name").expect("occurs") + 5);
    let actions = lsp.request(
        "textDocument/codeAction",
        &json!({ "textDocument": doc, "range": { "start": at, "end": at }, "context": { "diagnostics": [] } }),
    );
    let rewrite = (actions["result"].as_array().expect("actions").iter())
        .find(|a| a["title"] == "write `.name` as a function")
        .unwrap_or_else(|| panic!("{actions}"));
    assert_eq!(rewrite["kind"], "refactor.rewrite");
    assert!(rewrite.get("isPreferred").is_none(), "{rewrite}");
    let edits = &rewrite["edit"]["changes"][uri(&path)];
    assert_eq!(edits[0]["newText"], "(fun x -> x.name)", "{rewrite}");
    lsp.close();
}
