use super::*;

#[test]
fn private_module_completion_survives_editor_changes() {
    let header = "module App.Main exposing (..)\nimport Lib.Facade\nimport App.Hidden\n";
    let initial = format!("{header}let answer = Lib.Facade.secret\n");
    let dir = project(
        "lsp_private_modules",
        &[
            (
                "app/flick.toml",
                &format!("{LIB}\n[dependencies]\nlib = {{ path = \"../lib\" }}\n"),
            ),
            ("app/src/App/Main.fl", &initial),
            (
                "app/src/App/Hidden.fl",
                "private module App.Hidden exposing (secret)\nlet secret = 7\n",
            ),
            ("lib/flick.toml", LIB),
            (
                "lib/src/Lib/Hidden.fl",
                "private module Lib.Hidden exposing (secret)\nlet secret = 42\n",
            ),
            (
                "lib/src/Lib/Facade.fl",
                "module Lib.Facade exposing (secret)\nimport Lib.Hidden exposing (secret)\n",
            ),
        ],
    );
    let file = dir.join("app/src/App/Main.fl");
    let file_uri = uri(&file);
    let mut lsp = Lsp::start(&dir.join("app"));
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
            "uri": file_uri, "languageId": "flick", "version": 1, "text": initial,
        } }),
    );
    lsp.diagnostics(&file_uri);
    for (version, (path, visible)) in [
        ("Lib.Hidden", false),
        ("App.Hidden", true),
        ("Lib.Facade", true),
        ("Lib.Hidden", false),
    ]
    .into_iter()
    .enumerate()
    {
        // The non-ASCII prefix also exercises UTF-16 position conversion.
        let text = format!("{header}let answer = (\"🦀\", {path}.secret)\n");
        lsp.notify(
            "textDocument/didChange",
            &json!({ "textDocument": { "uri": file_uri, "version": version + 2 },
                "contentChanges": [{ "text": text }] }),
        );
        lsp.diagnostics(&file_uri);
        let offset = text.rfind(".secret").expect("member") + 1;
        let reply = lsp.request(
            "textDocument/completion",
            &json!({ "textDocument": { "uri": file_uri },
                "position": position(&text, offset),
                "context": { "triggerKind": 2, "triggerCharacter": "." } }),
        );
        let items = reply["result"].as_array().expect("completion items");
        assert_eq!(
            items.iter().any(|item| item["label"] == "secret"),
            visible,
            "{path}: {reply}"
        );
    }
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": file_uri } }),
    );
    lsp.close();
}
