use super::*;

/// The codes of the diagnostics published for `file`.
fn codes(lsp: &mut Lsp, file: &Path) -> Vec<String> {
    (lsp.diagnostics(&uri(file)).iter())
        .filter_map(|d| d["code"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn a_manifest_that_does_not_read_reports_why() {
    let dir = project(
        "lsp_manifest_unread",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"nope\"\n",
            ),
            (
                "src/App/M.fl",
                "module App.M exposing (..)\n\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let file = dir.join("src/App/M.fl");
    let text = fs::read_to_string(&file).expect("source");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": text } }));
    assert_eq!(codes(&mut lsp, &dir.join("flick.toml")), ["FLICK-P005"]);
    lsp.close();
}

#[test]
fn a_plugin_that_does_not_load_is_reported_on_its_import_and_its_block() {
    let app = "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[dependencies]\npairs = { path = \"../lib\" }\n";
    let lib = "schema-version = 1\n\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\n\n[project]\nkind = \"lib\"\n";
    let main = "module App.Main exposing (point)\n\nimport Pairs exposing (plugin pairs)\n\npoint : Float\nlet point = pairs { x = 1.0 }\n";
    let dir = project(
        "lsp_plugin_missing",
        &[
            ("app/flick.toml", app),
            ("app/src/App/Main.fl", main),
            ("lib/flick.toml", lib),
            (
                "lib/src/Pairs.fl",
                "module Pairs exposing (one)\n\none : Float\nlet one = 1.0\n",
            ),
        ],
    );
    let file = dir.join("app/src/App/Main.fl");
    let mut lsp = Lsp::start(&dir.join("app"));
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": main } }));
    let found = codes(&mut lsp, &file);
    assert!(found.contains(&"FLICK-P035".to_owned()), "{found:?}");
    assert!(found.contains(&"FLICK-P036".to_owned()), "{found:?}");
    lsp.close();
}

#[test]
fn the_manifest_of_the_workspace_around_a_member_reports_its_errors() {
    let root = "schema-version = 1\n\n[workspace]\nmembers = [\"app\"]\n\n[[workspace.plugins]]\npackage = \"pairs\"\n";
    let dir = project(
        "lsp_workspace_manifest",
        &[
            ("flick.toml", root),
            ("app/flick.toml", LIB),
            (
                "app/src/App/M.fl",
                "module App.M exposing (..)\n\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let file = dir.join("app/src/App/M.fl");
    let text = fs::read_to_string(&file).expect("source");
    let mut lsp = Lsp::start(&dir.join("app"));
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": text } }));
    assert_eq!(codes(&mut lsp, &dir.join("flick.toml")), ["FLICK-P005"]);
    lsp.close();
}
