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

#[test]
fn browser_db_preflight_is_the_same_for_the_editor_and_check() {
    let config = "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[build]\nenvironment = \"js/browser\"\n";
    let source = "module App.Main exposing (run)\n\nservice Db =\n  query : String ->{} String\n\nrun : String ->{Db} String\nlet run key = Db.query key\n";
    let dir = project(
        "lsp_target_preflight_db",
        &[("flick.toml", config), ("src/App/Main.fl", source)],
    );
    let file = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }),
    );
    let published = lsp.diagnostics(&uri(&file));
    let lsp_hit = published
        .iter()
        .find(|d| d["code"] == "FLICK-E020")
        .unwrap_or_else(|| panic!("LSP E020 in {published:?}"));
    assert_eq!(
        lsp_hit["message"].as_str(),
        Some("unhandled effect Db on js/browser")
    );
    lsp.close();
    let out = flick_in(&dir, &["check", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let check_hit = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|d| d["code"] == "FLICK-E020")
        .expect("check E020");
    assert_eq!(check_hit["message"], lsp_hit["message"]);
    let start = check_hit["range"]["start"].as_u64().expect("start") as usize;
    let end = check_hit["range"]["end"].as_u64().expect("end") as usize;
    assert_eq!(&source[start..end], "Db.query");
    assert_eq!(
        lsp_hit["range"],
        json!({ "start": position(source, start), "end": position(source, end) })
    );
}

#[test]
fn a_workspace_git_plugin_is_the_same_chain_for_the_editor_and_the_command_line() {
    let root = "schema-version = 1\n\n[workspace]\nmembers = [\"app\"]\n\n[[workspace.plugins]]\npackage = \"pairs\"\nsource = { git = \"https://example.com/pairs\", tag = \"v1.0.0\", subdir = \"lib\" }\n";
    let app = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";
    let main = "module App.Main exposing (point)\n\nimport Pairs exposing (plugin pairs)\n\npoint : Float\nlet point = pairs { x = 1.0 }\n";
    let dir = project(
        "lsp_workspace_git_plugin",
        &[
            ("flick.toml", root),
            ("app/flick.toml", app),
            ("app/src/App/Main.fl", main),
        ],
    );
    let file = dir.join("app/src/App/Main.fl");
    let mut lsp = Lsp::start(&dir.join("app"));
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": main } }));
    let message = "dependency `pairs` is not in `flick.lock`: run `flick install`";
    // the dependency is a place in the workspace's manifest, as for check
    let _ = lsp.diagnostics(&uri(&file));
    let diagnostics = lsp.diagnostics(&uri(&dir.join("flick.toml")));
    assert!(
        diagnostics
            .iter()
            .any(|d| d["message"].as_str().is_some_and(|m| m.contains(message))),
        "{diagnostics:?}"
    );
    lsp.close();
    let out = flick_in(dir.join("app"), &["plugins"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FLICK-P034") && stderr.contains(message),
        "{stderr}"
    );
}
