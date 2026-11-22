use super::*;

#[test]
fn orelse_hover_and_code_action_use_utf16_offsets_and_respect_did_change() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let dir = env::temp_dir().join(format!("flick-lsp-orelse-{}", process::id()));
    let path = dir.join("A.fl");
    // Line 3 has Cyrillic + supplementary emoji (2 UTF-16 units) before `orelse`.
    let source = "module A exposing (pick)\n\npick : Option String -> String\nlet pick x = \"Новосибирск 🚀 \" ++ (x orelse \"дом\")\n";
    server.notify("textDocument/didOpen", &opened(&path, source));
    settle(&mut server);
    let line3 = source.lines().nth(3).expect("line 3");
    let orelse_byte = line3.find("orelse").expect("orelse");
    let orelse_utf16 = u32::try_from(line3[..orelse_byte].encode_utf16().count()).expect("utf16");

    let hover = server
        .request(
            "textDocument/hover",
            &json!({
                "textDocument": { "uri": protocol::uri_of(&path) },
                "position": { "line": 3, "character": orelse_utf16 + 1 }
            }),
        )
        .expect("hover response");
    let hover_out = hover.to_string();
    assert!(
        hover_out.contains("option orelse fallback : String"),
        "{hover_out}"
    );

    let actions = server
        .request(
            "textDocument/codeAction",
            &json!({
                "textDocument": { "uri": protocol::uri_of(&path) },
                "range": {
                    "start": { "line": 3, "character": orelse_utf16 + 1 },
                    "end": { "line": 3, "character": orelse_utf16 + 1 }
                },
                "context": { "diagnostics": [] }
            }),
        )
        .expect("codeAction response");
    let action_out = actions.to_string();
    assert!(
        action_out.contains("expand `orelse` to `match`"),
        "{action_out}"
    );

    // After breaking the right-hand side via didChange, stale code action is not offered.
    let after_orelse_utf16 = orelse_utf16 + 6;
    let close_paren_byte = line3.rfind(')').expect("close paren");
    let close_paren_utf16 =
        u32::try_from(line3[..close_paren_byte].encode_utf16().count()).expect("close utf16");
    server.notify(
        "textDocument/didChange",
        &change(&path, (3, after_orelse_utf16), (3, close_paren_utf16), ""),
    );
    settle(&mut server);
    let stale = server
        .request(
            "textDocument/codeAction",
            &json!({
                "textDocument": { "uri": protocol::uri_of(&path) },
                "range": {
                    "start": { "line": 3, "character": orelse_utf16 + 1 },
                    "end": { "line": 3, "character": orelse_utf16 + 1 }
                },
                "context": { "diagnostics": [] }
            }),
        )
        .expect("stale codeAction response");
    let stale_out = stale.to_string();
    assert!(
        !stale_out.contains("expand `orelse` to `match`"),
        "{stale_out}"
    );
}
