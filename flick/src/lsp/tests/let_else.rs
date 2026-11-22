use super::*;

#[test]
fn let_else_hover_highlights_code_action_and_utf16_edit_requery() {
    let mut server = server(Box::new(io::sink()));
    let init = server
        .request("initialize", &json!({ "capabilities": {} }))
        .expect("initialized");
    assert_eq!(init["capabilities"]["documentHighlightProvider"], true);

    let dir = env::temp_dir().join(format!("flick-lsp-let-else-{}", process::id()));
    let path = dir.join("A.fl");
    // Non-ASCII `é` (1 UTF-16 unit, 2 UTF-8 bytes) and `𝄞` (2 UTF-16 units, 4 UTF-8 bytes)
    // precede `let ... else` to exercise UTF-16 conversion across every query.
    let source = "module A exposing (run)\n\nrun : Option String -> String\nlet run m =\n  let prefix = \"é𝄞:\"\n  let Some s = m else \"é𝄞\"\n  prefix ++ s\n";
    let uri = protocol::uri_of(&path);
    server.notify("textDocument/didOpen", &opened(&path, source));
    settle(&mut server);

    // Line 5: `  let Some s = m else "é𝄞"` -> `else` starts at UTF-16 col 17.
    let hover = server
        .request(
            "textDocument/hover",
            &json!({ "textDocument": { "uri": uri }, "position": { "line": 5, "character": 17 } }),
        )
        .expect("hover");
    let hover_val = hover["contents"]["value"].as_str().unwrap_or_default();
    assert!(
        hover_val.contains("String") && hover_val.contains("does not match"),
        "{hover}"
    );

    let highlights = server
        .request(
            "textDocument/documentHighlight",
            &json!({ "textDocument": { "uri": uri }, "position": { "line": 5, "character": 17 } }),
        )
        .expect("documentHighlight");
    let arr = highlights.as_array().expect("highlight array");
    assert_eq!(arr.len(), 4, "let, else, fallback, remainder: {highlights}");
    // Fallback `"é𝄞"` is on line 5 from col 22 to col 27 in UTF-16 (`"` + `é`(1) + `𝄞`(2) + `"` = 5 units).
    assert_eq!(
        arr[2]["range"]["start"],
        json!({ "line": 5, "character": 22 })
    );
    assert_eq!(
        arr[2]["range"]["end"],
        json!({ "line": 5, "character": 27 })
    );
    // Skipped remainder `prefix ++ s` is on line 6.
    assert_eq!(
        arr[3]["range"]["start"],
        json!({ "line": 6, "character": 2 })
    );
    assert_eq!(
        arr[3]["range"]["end"],
        json!({ "line": 6, "character": 13 })
    );

    let actions = server
        .request(
            "textDocument/codeAction",
            &json!({
                "textDocument": { "uri": uri },
                "range": {
                    "start": { "line": 5, "character": 2 },
                    "end": { "line": 5, "character": 5 }
                },
                "context": { "diagnostics": [] }
            }),
        )
        .expect("codeAction");
    assert!(
        actions
            .as_array()
            .expect("actions")
            .iter()
            .any(|a| a["title"] == "expand `let ... else` to `match`"),
        "{actions}"
    );

    // Edit the remainder line and re-query immediately after settling: no stale highlight end column!
    server.notify(
        "textDocument/didChange",
        &change(&path, (6, 2), (6, 13), "s"),
    );
    settle(&mut server);
    let updated = server
        .request(
            "textDocument/documentHighlight",
            &json!({ "textDocument": { "uri": uri }, "position": { "line": 5, "character": 17 } }),
        )
        .expect("updated documentHighlight");
    let updated_arr = updated.as_array().expect("updated array");
    assert_eq!(
        updated_arr[3]["range"]["end"],
        json!({ "line": 6, "character": 3 }),
        "stale highlight range after edit: {updated}"
    );
}
