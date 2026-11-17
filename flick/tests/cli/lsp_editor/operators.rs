use super::*;

/// Typing a composition or a `<|` passes through text with an operand
/// missing: the server reports it and lives on to check the next text.
#[test]
fn an_operand_missing_while_typing_is_reported_not_fatal() {
    let saved = "module App.Main exposing (twice, applied)\n\n\
                 inc : Int -> Int\nlet inc n = n + 1\n\n\
                 twice : Int -> Int\nlet twice = inc >> inc\n\n\
                 applied : Int\nlet applied = inc <| 1\n";
    let dir = project(
        "lsp_operator_missing",
        &[("flick.toml", LIB), ("src/App/Main.fl", saved)],
    );
    let file = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
            "uri": uri(&file), "languageId": "flick", "version": 1, "text": saved
        } }),
    );
    assert_eq!(lsp.diagnostics(&uri(&file)), Vec::<Value>::new());
    let typing = [
        saved.replace("inc >> inc", "(inc >> >> inc) << inc"),
        saved.replace("inc <| 1", "inc <| >> inc"),
        saved.to_owned(),
    ];
    for (version, text) in (2..).zip(&typing) {
        lsp.notify(
            "textDocument/didChange",
            &json!({
                "textDocument": { "uri": uri(&file), "version": version },
                "contentChanges": [{ "text": text }]
            }),
        );
        let codes: Vec<String> = (lsp.diagnostics(&uri(&file)).iter())
            .filter_map(|d| d["code"].as_str().map(str::to_owned))
            .collect();
        let missing = text != saved;
        assert_eq!(
            codes.contains(&"FLICK-S007".to_owned()),
            missing,
            "{text}: {codes:?}"
        );
    }
    lsp.close();
}
