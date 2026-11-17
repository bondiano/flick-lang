use super::*;

fn assert_local_hovers(lsp: &mut Lsp, file: &Path, source: &str, prefix: &str) {
    for (start, _) in source.match_indices("total") {
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": { "uri": uri(file) }, "position": position(source, start) }),
        );
        assert_eq!(hover["result"]["contents"]["kind"], "markdown");
        assert_eq!(
            hover["result"]["contents"]["value"],
            format!("```flick\n{prefix}total : Int\n```"),
            "{hover}"
        );
        assert_eq!(
            hover["result"]["range"],
            json!({ "start": position(source, start), "end": position(source, start + 5) }),
            "{hover}"
        );
    }
}

#[test]
fn mutable_hover_follows_open_edit_and_close() {
    let saved = "module App.Main exposing (count)\n// 🦔\ncount : Int -> Int\nlet count n =\n  let mut total = n\n  total = total + 1\n  total\n";
    let edited = saved
        .replace("let mut total", "let total")
        .replace("  total = total + 1\n", "");
    let dir = project(
        "lsp_mutable_hover_edits",
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
    assert_local_hovers(&mut lsp, &file, saved, "mut ");
    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri(&file), "version": 2 },
            "contentChanges": [{ "text": edited }]
        }),
    );
    assert_local_hovers(&mut lsp, &file, &edited, "");
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&file) } }),
    );
    assert_local_hovers(&mut lsp, &file, saved, "mut ");
    lsp.close();
}
