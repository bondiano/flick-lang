use super::*;

const SOURCE: &str = "module App.Count exposing (count)\n\ncount : Unit -> Int\nforeign pure count from \"./count.mjs\" \"count\"\n";
const JS: &str = "// export const count = wrong\nexport const count = () => 1;\n";
const GO: &str =
    "package main\n// func count() is not the binding\nfunc count() int { return 1 }\n";

#[test]
fn local_foreign_definition_follows_build_target() {
    for (target, expected_file, expected_line) in [("js", "count.mjs", 1), ("go", "count.go", 2)] {
        let config = format!("{LIB}\n[build]\nemit = \"{target}\"\n");
        let dir = project(
            &format!("lsp_foreign_{target}"),
            &[
                ("flick.toml", &config),
                ("src/App/Count.fl", SOURCE),
                ("src/App/count.mjs", JS),
                ("src/App/count.go", GO),
            ],
        );
        let file = dir.join("src/App/Count.fl");
        let target_file = dir.join("src/App").join(expected_file);
        let mut lsp = Lsp::start(&dir);
        lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": SOURCE } }));
        let _ = lsp.diagnostics(&uri(&file));
        for (needle, line, character) in [
            ("./count.mjs", 0, 0),
            (
                "\"count\"",
                expected_line,
                if target == "go" { 5 } else { 13 },
            ),
        ] {
            let found = lsp.request(
                "textDocument/definition",
                &json!({
                    "textDocument": { "uri": uri(&file) }, "position": at(SOURCE, needle)
                }),
            );
            assert_eq!(found["result"]["uri"], uri(&target_file), "{found}");
            assert_eq!(
                found["result"]["range"]["start"],
                json!({"line": line, "character": character}),
                "{found}"
            );
        }
        lsp.close();
    }
}

#[test]
fn foreign_alias_reaches_local_declaration() {
    let source = SOURCE.replace("\"count\"", "\"alias\"");
    let js = "// export const alias = wrong\nconst count = () => 1;\nexport { count as alias };\n";
    let dir = project(
        "lsp_foreign_alias",
        &[
            ("flick.toml", LIB),
            ("src/App/Count.fl", &source),
            ("src/App/count.mjs", js),
        ],
    );
    let file = dir.join("src/App/Count.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    let _ = lsp.diagnostics(&uri(&file));
    let found = lsp.request(
        "textDocument/definition",
        &json!({ "textDocument": { "uri": uri(&file) }, "position": at(&source, "\"alias\"") }),
    );
    assert_eq!(
        found["result"]["range"]["start"],
        json!({"line": 1, "character": 6}),
        "{found}"
    );
    lsp.close();
}

#[test]
fn foreign_missing_file_or_export_has_no_definition() {
    let source = SOURCE.replace("\"count\"", "\"absent\"");
    let dir = project(
        "lsp_foreign_missing",
        &[
            ("flick.toml", LIB),
            ("src/App/Count.fl", &source),
            ("src/App/count.mjs", JS),
        ],
    );
    let file = dir.join("src/App/Count.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    let _ = lsp.diagnostics(&uri(&file));
    let missing_export = lsp.request(
        "textDocument/definition",
        &json!({ "textDocument": { "uri": uri(&file) }, "position": at(&source, "\"absent\"") }),
    );
    assert!(missing_export["result"].is_null(), "{missing_export}");
    let absent_source = source.replace("./count.mjs", "./gone.mjs");
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 2 }, "contentChanges": [{ "text": absent_source }] }));
    let missing_file = lsp.request("textDocument/definition", &json!({ "textDocument": { "uri": uri(&file) }, "position": at(&absent_source, "./gone.mjs") }));
    assert!(missing_file["result"].is_null(), "{missing_file}");
    lsp.close();
}
