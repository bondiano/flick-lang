use super::*;

#[test]
fn fln_documents_publish_diagnostics_format_and_clear_without_worker() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let dir = env::temp_dir().join(format!("flick-lsp-fln-{}", process::id()));
    let path = dir.join("config.fln");
    let uri = protocol::uri_of(&path);

    let valid = "{ host = \"localhost\"; port = 8080u16 }\n";
    server.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": { "uri": uri, "languageId": "fln", "version": 1, "text": valid } }),
    );
    assert!(server.pending.is_empty(), "fln does not queue Flick worker");
    assert!(written_text(&written).contains("\"diagnostics\":[]"));

    // Formatting returns a canonical edit
    let fmt = server
        .request(
            "textDocument/formatting",
            &json!({ "textDocument": { "uri": uri }, "options": { "tabSize": 2, "insertSpaces": true } }),
        )
        .expect("formatting");
    assert!(fmt.is_array(), "expected text edits: {fmt:?}");

    // Non-FLN IDE queries return empty/null cleanly without error
    assert_eq!(
        server
            .request(
                "textDocument/hover",
                &json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 2 } }),
            )
            .expect("hover"),
        Value::Null
    );
    assert_eq!(
        server
            .request(
                "textDocument/completion",
                &json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 2 } }),
            )
            .expect("completion"),
        json!([])
    );
    assert_eq!(
        server
            .request(
                "textDocument/semanticTokens/full",
                &json!({ "textDocument": { "uri": uri } }),
            )
            .expect("semantic tokens"),
        json!({ "data": [] })
    );

    // Edit to malformed FLN publishes FLICK-S007 with span
    written.0.lock().expect("not poisoned").clear();
    server.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": "{\n  host = \"unterminated\n}" }]
        }),
    );
    let out = written_text(&written);
    assert!(
        out.contains("FLICK-S007"),
        "expected FLICK-S007 on malformed fln: {out}"
    );

    // Closing clears diagnostics
    written.0.lock().expect("not poisoned").clear();
    server.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri } }),
    );
    assert!(written_text(&written).contains("\"diagnostics\":[]"));
}
