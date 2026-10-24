use super::*;

const SOURCE: &str = "module App.Geo exposing (Shape(..), area, scaled)

type Shape = Circle Number | Square Number

add3 : Number -> Number -> Number -> Number
let add3 a b c = a + b + c

area : Shape -> Number
let area shape =
  let base = add3 1 2 3
  match shape with
  | Circle r -> r * base
  | Square s -> s * s

scaled : List Number -> List Number
let scaled xs =
  xs
    |> List.map (fun x -> x * 2)
";

#[test]
fn outline_tokens_folding_hints_and_signature_help() {
    let dir = project(
        "lsp_outline",
        &[("flick.toml", LIB), ("src/App/Geo.fl", SOURCE)],
    );
    let file = dir.join("src/App/Geo.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    let init = lsp.request("initialize", &json!({}));
    let caps = &init["result"]["capabilities"];
    for cap in [
        "documentSymbolProvider",
        "workspaceSymbolProvider",
        "typeDefinitionProvider",
        "foldingRangeProvider",
        "inlayHintProvider",
        "signatureHelpProvider",
        "semanticTokensProvider",
    ] {
        assert!(!caps[cap].is_null(), "{cap} in {caps}");
    }
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": SOURCE } }));
    let _ = lsp.diagnostics(&uri(&file));

    let symbols = lsp.request(
        "textDocument/documentSymbol",
        &json!({ "textDocument": doc }),
    );
    let names: Vec<&str> = symbols["result"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(names, ["Shape", "add3", "area", "scaled"]);
    assert_eq!(symbols["result"][0]["children"][1]["name"], "Square");

    let found = lsp.request("workspace/symbol", &json!({ "query": "sqr" }));
    assert_eq!(found["result"][0]["name"], "Square", "{found}");

    let tokens = lsp.request(
        "textDocument/semanticTokens/full",
        &json!({ "textDocument": doc }),
    );
    let data = tokens["result"]["data"].as_array().expect("data");
    assert!(!data.is_empty() && data.len().is_multiple_of(5), "{tokens}");

    let folds = lsp.request("textDocument/foldingRange", &json!({ "textDocument": doc }));
    assert!(
        folds["result"]
            .as_array()
            .expect("folds")
            .iter()
            .any(|f| f["startLine"] == 10),
        "{folds}"
    );

    let hints = lsp.request("textDocument/inlayHint", &json!({ "textDocument": doc, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 20, "character": 0 } } }));
    let labels: Vec<&str> = hints["result"]
        .as_array()
        .expect("hints")
        .iter()
        .filter_map(|h| h["label"].as_str())
        .collect();
    assert_eq!(labels, [": Number", " : List Number"], "{hints}");

    let help = lsp.request("textDocument/signatureHelp", &json!({ "textDocument": doc, "position": position(SOURCE, SOURCE.find("2 3").expect("call") + 1) }));
    assert_eq!(help["result"]["activeParameter"], 1, "{help}");
    assert_eq!(
        help["result"]["signatures"][0]["label"],
        "add3 : Number -> Number -> Number -> Number"
    );

    let typedef = lsp.request(
        "textDocument/typeDefinition",
        &json!({ "textDocument": doc, "position": at(SOURCE, "shape with") }),
    );
    assert_eq!(typedef["result"]["range"]["start"]["line"], 2, "{typedef}");
    lsp.close();
}
