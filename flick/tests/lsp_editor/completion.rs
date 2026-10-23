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
