#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::PathBuf;

use super::*;

fn isolation_projects() -> [(PathBuf, String, &'static str); 2] {
    [
        ("a", "lsp_isolation_a", "String", "\"Ada\"", "\"𝄞π\" ++ "),
        ("b", "lsp_isolation_a/src/nested", "Number", "42", ""),
    ]
    .map(|(name, app_name, kind, argument, prefix)| {
        let package_source = format!(
            "module Lib.Value exposing (value)\n\n/// Package {name}.\nvalue : {kind} -> {kind}\nlet value x = x\n"
        );
        let lib = project(
            &format!("lsp_isolation_package_{name}"),
            &[
                ("flick.toml", LIB),
                (
                    "package.json",
                    "{ \"name\": \"@acme/value\", \"version\": \"1.0.0\" }",
                ),
                ("src/Lib/Value.fl", &package_source),
            ],
        );
        let built = flick_in(&lib, &["build"]);
        assert_eq!(
            built.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let domain = format!(
            "module App.Domain exposing (message)\n\nmessage : {kind}\nlet message = {argument}\n"
        );
        let main = format!(
            "module App.Main exposing (run)\n\nimport App.Domain exposing (message)\nimport Lib.Value exposing (value)\n\nrun : {kind}\nlet run = {prefix}value message\n"
        );
        let app = project(
            app_name,
            &[
                ("flick.toml", LIB),
                (
                    "package.json",
                    "{ \"name\": \"app\", \"dependencies\": { \"@acme/value\": \"1.0.0\" } }",
                ),
                ("src/App/Main.fl", &main),
                ("src/App/Domain.fl", &domain),
                (
                    "src/App/Error.fl",
                    "module App.Error exposing (broken)\n\nbroken : Number\nlet broken = 0\n",
                ),
                (
                    "node_modules/@acme/value/package.json",
                    "{ \"name\": \"@acme/value\", \"version\": \"1.0.0\", \"flick\": { \"iface-dir\": \"dist\" } }",
                ),
            ],
        );
        let package = app.join("node_modules/@acme/value");
        copy_tree(&lib.join("dist"), &package.join("dist"));
        copy_tree(&lib.join("src"), &package.join("src"));
        (app, main, kind)
    })
}

#[test]
fn projects_own_their_buffers_packages_and_diagnostics() {
    let projects = isolation_projects();

    let (a, a_text, _) = &projects[0];
    let (b, b_text, _) = &projects[1];
    let a_main = a.join("src/App/Main.fl");
    let a_error = a.join("src/App/Error.fl");
    let b_main = b.join("src/App/Main.fl");
    let bad = "module App.Error exposing (broken)\n\nbroken : Number\nlet broken = \"𝄞π\"\n";
    let reports_a_error = |reports: &[Value]| {
        reports.iter().any(|p| {
            p["uri"] == uri(&a_error)
                && p["diagnostics"]
                    .as_array()
                    .is_some_and(|ds| ds.iter().any(|d| d["code"] == "FLICK-T003"))
        })
    };
    let mut lsp = Lsp::start(a);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&a_main), "languageId": "flick", "version": 1, "text": a_text } }));
    let reports = lsp.publications();
    assert!(
        reports.iter().any(|p| p["uri"] == uri(&a_main)),
        "{reports:?}"
    );
    assert!(
        reports.iter().all(|p| p["diagnostics"] == json!([])),
        "{reports:?}"
    );
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&a_error), "languageId": "flick", "version": 1, "text": bad } }));
    let reports = lsp.publications();
    assert!(reports_a_error(&reports), "{reports:?}");

    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&b_main), "languageId": "flick", "version": 1, "text": b_text } }));
    let reports = lsp.publications();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0]["uri"], uri(&b_main));
    assert_eq!(reports[0]["diagnostics"], json!([]));

    for index in [1, 0, 1, 0] {
        let (dir, text, kind) = &projects[index];
        let doc = json!({ "uri": uri(&dir.join("src/App/Main.fl")) });
        let point = at(text, "value message");
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": doc, "position": point }),
        );
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains(&format!("value : {kind} -> {kind}"))),
            "{hover}"
        );
        let definition = lsp.request(
            "textDocument/definition",
            &json!({ "textDocument": doc, "position": point }),
        );
        assert_eq!(
            definition["result"]["uri"],
            uri(&dir.join("node_modules/@acme/value/src/Lib/Value.fl")),
            "{definition}"
        );
        let point = position(text, text.rfind("message").expect("local import") + 1);
        let definition = lsp.request(
            "textDocument/definition",
            &json!({ "textDocument": doc, "position": point }),
        );
        assert_eq!(
            definition["result"]["uri"],
            uri(&dir.join("src/App/Domain.fl")),
            "{definition}"
        );
        let rename = lsp.request(
            "textDocument/rename",
            &json!({ "textDocument": doc, "position": point, "newName": "greeting" }),
        );
        let changes = rename["result"]["changes"].as_object().expect("rename");
        assert_eq!(changes.len(), 2, "{rename}");
        assert!(changes.contains_key(&uri(&dir.join("src/App/Main.fl"))));
        assert!(changes.contains_key(&uri(&dir.join("src/App/Domain.fl"))));
    }

    for method in ["textDocument/didSave", "textDocument/didClose"] {
        lsp.notify(method, &json!({ "textDocument": { "uri": uri(&b_main) } }));
        let reports = lsp.publications();
        assert_eq!(reports.len(), 1, "{method}: {reports:?}");
        assert_eq!(reports[0]["uri"], uri(&b_main));
        assert_eq!(reports[0]["diagnostics"], json!([]));
    }
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&a_main) } }),
    );
    let reports = lsp.publications();
    assert!(reports_a_error(&reports), "{reports:?}");
    assert!(reports.iter().all(|p| p["uri"] != uri(&b_main)));
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&a_error) } }),
    );
    let reports = lsp.publications();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0]["uri"], uri(&a_error));
    assert_eq!(reports[0]["diagnostics"], json!([]));
    lsp.close();
}

#[test]
fn standalone_documents_have_independent_contexts() {
    let first = "module Main exposing (answer)\n\nanswer : String\nlet answer = \"one\"\n";
    let second = "module Main exposing (answer)\n\nanswer : Number\nlet answer = 2\n";
    let dir = project(
        "lsp_standalone_contexts",
        &[("first/Main.fl", first), ("second/Main.fl", second)],
    );
    let mut lsp = Lsp::start(&dir);
    for (relative, text) in [("first/Main.fl", first), ("second/Main.fl", second)] {
        let file = dir.join(relative);
        lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": text } }));
        let reports = lsp.publications();
        assert_eq!(reports.len(), 1, "{reports:?}");
        assert_eq!(reports[0]["uri"], uri(&file));
        assert_eq!(reports[0]["diagnostics"], json!([]));
    }
    for (relative, text, kind) in [
        ("first/Main.fl", first, "String"),
        ("second/Main.fl", second, "Number"),
    ] {
        let file = dir.join(relative);
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": { "uri": uri(&file) }, "position": at(text, "answer :") }),
        );
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains(&format!("answer : {kind}"))),
            "{hover}"
        );
    }
    let closed = dir.join("second/Main.fl");
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&closed) } }),
    );
    let reports = lsp.publications();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0]["uri"], uri(&closed));
    assert_eq!(reports[0]["diagnostics"], json!([]));
    let unopened = dir.join("first/Main.fl");
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&unopened) } }),
    );
    let _ = lsp.publications();
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": { "uri": uri(&unopened) }, "position": at(first, "answer :") }),
    );
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|s| s.contains("answer : String")),
        "{hover}"
    );
    lsp.close();
}

#[test]
fn standalone_imports_follow_unsaved_dependency_edits_and_close() {
    let main = "module A exposing (answer)\nimport B exposing (value)\nlet answer = value\n";
    let saved = "module B exposing (value)\nvalue : Number\nlet value = 1\n";
    let unsaved = "module B exposing (value)\nvalue : String\nlet value = \"draft\"\n";
    let dir = project(
        "lsp_standalone_dependency",
        &[("A.fl", main), ("B.fl", saved)],
    );
    let dependency = dir.join("B.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
        "uri": uri(&dependency), "languageId": "flick", "version": 1, "text": saved
    } }),
    );
    let hover = |lsp: &mut Lsp| {
        let reply = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": { "uri": uri(&dir.join("A.fl")) }, "position": at(main, "value\n") }),
        );
        reply["result"]["contents"]["value"]
            .as_str()
            .expect("imported value hover")
            .to_owned()
    };
    let initial = hover(&mut lsp);
    assert!(initial.contains("value : Number"), "{initial}");
    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri(&dependency), "version": 2 },
            "contentChanges": [{ "text": unsaved }]
        }),
    );
    let changed = hover(&mut lsp);
    assert!(changed.contains("value : String"), "{changed}");
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&dependency) } }),
    );
    let closed = hover(&mut lsp);
    assert!(closed.contains("value : Number"), "{closed}");
    lsp.close();
}

#[cfg(unix)]
#[test]
fn project_aliases_share_identity_without_duplicate_modules() {
    let main = "module App.Main exposing (run)\n\nimport App.Domain exposing (message)\n\nrun : String\nlet run = message\n";
    let domain = "module App.Domain exposing (message)\n\n/// Unsaved greeting.\nmessage : String\nlet message = 0\n";
    let dir = project(
        "lsp_canonical_project",
        &[
            ("flick.toml", LIB),
            ("src/App/Main.fl", main),
            ("src/App/Domain.fl", &domain.replace("= 0", "= \"hello\"")),
        ],
    );
    let links = project("lsp_canonical_project_link", &[("keep", "")]);
    let alias = links.join("alias");
    symlink(&dir, &alias).expect("project alias");
    let main_file = dir.join("src/App/Main.fl");
    let domain_file = alias.join("src/App/Domain.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&main_file), "languageId": "flick", "version": 1, "text": main } }));
    let reports = lsp.publications();
    assert!(
        reports.iter().all(|p| p["diagnostics"] == json!([])),
        "{reports:?}"
    );
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&domain_file), "languageId": "flick", "version": 1, "text": domain } }));
    let reports = lsp.publications();
    assert!(
        reports.iter().any(|p| {
            p["uri"] == uri(&domain_file)
                && p["diagnostics"]
                    .as_array()
                    .is_some_and(|ds| ds.iter().any(|d| d["code"] == "FLICK-T003"))
        }),
        "{reports:?}"
    );
    assert!(
        reports.iter().all(|p| {
            p["diagnostics"]
                .as_array()
                .is_some_and(|ds| ds.iter().all(|d| d["code"] != "FLICK-R002"))
        }),
        "{reports:?}"
    );
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": { "uri": uri(&main_file) }, "position": at(main, "message\n") }),
    );
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|s| s.contains("Unsaved greeting.")),
        "{hover}"
    );
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&main_file) } }),
    );
    let reports = lsp.publications();
    assert!(
        reports.iter().any(|p| {
            p["uri"] == uri(&domain_file)
                && p["diagnostics"]
                    .as_array()
                    .is_some_and(|ds| ds.iter().any(|d| d["code"] == "FLICK-T003"))
        }),
        "{reports:?}"
    );
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&domain_file) } }),
    );
    let reports = lsp.publications();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert_eq!(reports[0]["uri"], uri(&domain_file));
    assert_eq!(reports[0]["diagnostics"], json!([]));
    lsp.close();
}
