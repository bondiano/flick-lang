use super::*;

const SOURCE: &str = "module App.Notes exposing (..)

/// Doubles a number; see dou
double : Number -> Number
let double n = n * 2 // calls dou
/* block about dou */
label : String
let label = \"dou\"

value : Number
let value = dou
";

#[test]
fn comments_doc_comments_and_strings_offer_no_completion() {
    let dir = project(
        "lsp_completion_prose",
        &[("flick.toml", LIB), ("src/App/Notes.fl", SOURCE)],
    );
    let file = dir.join("src/App/Notes.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": SOURCE } }));
    let labels = |lsp: &mut Lsp, at: usize| -> Vec<String> {
        let completion = lsp.request(
            "textDocument/completion",
            &json!({ "textDocument": doc, "position": position(SOURCE, at) }),
        );
        completion["result"]
            .as_array()
            .expect("items")
            .iter()
            .filter_map(|i| i["label"].as_str().map(str::to_owned))
            .collect()
    };
    for needle in ["see dou", "calls dou", "about dou", "\"dou"] {
        let at = SOURCE.find(needle).expect("occurs") + needle.len();
        assert!(labels(&mut lsp, at).is_empty(), "{needle}");
    }
    let at = SOURCE.rfind("dou").expect("occurs") + 3;
    assert!(
        labels(&mut lsp, at).iter().any(|l| l == "double"),
        "code still completes"
    );
    lsp.close();
}

#[test]
fn completions_carry_docs_snippets_order_and_imports() {
    let helpers = "module App.Helpers exposing (shout)\n\nshout : String -> String\nlet shout s = s ++ \"!\"\n";
    let main = "module App.Main exposing (go)\n\nimport String\n\ngo : Number -> String\nlet go count =\n  sho\n";
    let dir = project(
        "lsp_completion_rich",
        &[
            ("flick.toml", LIB),
            ("src/App/Notes.fl", SOURCE),
            ("src/App/Helpers.fl", helpers),
            ("src/App/Main.fl", main),
        ],
    );
    let notes = dir.join("src/App/Notes.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&notes), "languageId": "flick", "version": 1, "text": SOURCE } }));
    let at = SOURCE.rfind("dou").expect("occurs") + 3;
    let items = lsp.request(
        "textDocument/completion",
        &json!({ "textDocument": { "uri": uri(&notes) }, "position": position(SOURCE, at) }),
    );
    let items = items["result"].as_array().expect("items").clone();
    let double = items
        .iter()
        .find(|i| i["label"] == "double")
        .expect("double");
    assert_eq!(
        double["documentation"]["value"],
        "Doubles a number; see dou"
    );
    assert_eq!(double["insertText"], "double ${1:n}");
    assert_eq!(double["insertTextFormat"], 2);
    assert!(double["sortText"].as_str().expect("sort").starts_with('1'));

    let file = dir.join("src/App/Main.fl");
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": main } }));
    let at = main.rfind("sho").expect("occurs") + 3;
    let items = lsp.request(
        "textDocument/completion",
        &json!({ "textDocument": { "uri": uri(&file) }, "position": position(main, at) }),
    );
    let shout = items["result"]
        .as_array()
        .expect("items")
        .iter()
        .find(|i| i["label"] == "shout")
        .unwrap_or_else(|| panic!("{items}"))
        .clone();
    assert_eq!(shout["insertText"], "App.Helpers.shout ${1:s}");
    assert_eq!(
        shout["additionalTextEdits"][0]["newText"],
        "\nimport App.Helpers"
    );
    assert!(shout["sortText"].as_str().expect("sort").starts_with('4'));
    lsp.close();
}
