//! The Conduit example as an editor opens it, nothing built: the server's
//! routes reach the contract's API through the alias `Api`.

use super::*;

#[test]
fn the_conduit_routes_see_the_contract_through_its_alias() {
    let conduit = common::example("conduit", "lsp_conduit");
    let server = conduit.join("server");
    let file = server.join("src/Conduit/Server/Routes.fl");
    let text = fs::read_to_string(&file).expect("routes");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&server);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": text } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": doc, "position": at(&text, "Public.api") }),
    );
    let markdown = hover["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_default();
    assert!(markdown.contains("Signing up and in"), "{hover}");
    // the editor sends the text with the dot and the cursor after it
    let dotted = text.replace("Api.Public.api", "Api.");
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 2 }, "contentChanges": [{ "text": dotted }] }));
    let after = dotted.find("Api.\n").expect("dot") + 4;
    let completion = lsp.request(
        "textDocument/completion",
        &json!({ "textDocument": doc, "position": position(&dotted, after), "context": { "triggerKind": 2, "triggerCharacter": "." } }),
    );
    let labels: Vec<&str> = completion["result"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    for wanted in ["Public", "Users", "Articles"] {
        assert!(labels.contains(&wanted), "{wanted} in {labels:?}");
    }
    let dotted = text.replace("Api.Public.api", "Api.Public.");
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 3 }, "contentChanges": [{ "text": dotted }] }));
    let after = dotted.find("Api.Public.\n").expect("dot") + 11;
    let completion = lsp.request(
        "textDocument/completion",
        &json!({ "textDocument": doc, "position": position(&dotted, after), "context": { "triggerKind": 2, "triggerCharacter": "." } }),
    );
    let labels: Vec<&str> = completion["result"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(
        labels.contains(&"api") && labels.contains(&"endpoints"),
        "{labels:?}"
    );
    lsp.close();
}
