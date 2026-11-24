use std::{iter, mem};

use super::*;

fn project_dir(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("flick-lsp-{name}-{}", process::id()));
    fs::create_dir_all(&dir).expect("project created");
    fs::write(
        dir.join(project::CONFIG_FILE),
        "schema-version = 1\n[project]\nkind = \"lib\"\nsource-dir = \".\"\n",
    )
    .expect("manifest written");
    dir
}

fn next(server: &Server) -> worker::Done {
    server
        .worker
        .recv_timeout(Duration::from_mins(1))
        .expect("analysis completes")
}

fn take_messages(written: &Written) -> Vec<Value> {
    let bytes = mem::take(&mut *written.0.lock().expect("not poisoned"));
    let mut input = io::Cursor::new(bytes);
    iter::from_fn(|| protocol::read(&mut input).expect("valid protocol")).collect()
}

#[test]
fn an_older_project_snapshot_cannot_republish_a_fixed_sibling_error() {
    let dir = project_dir("sibling-freshness");
    let (a, b) = (dir.join("A.fl"), dir.join("B.fl"));
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    server.notify(
        "textDocument/didOpen",
        &opened(&a, "module A exposing (x)\n\nx : Int\nlet x = 1\n"),
    );
    server.notify(
        "textDocument/didOpen",
        &opened(
            &b,
            "module B exposing (v)\n\nv : Int\nlet v = missingValue\n",
        ),
    );
    settle(&mut server);
    assert!(written_text(&written).contains("FLICK-T001"));
    take_messages(&written);

    server.notify("textDocument/didChange", &change(&a, (3, 8), (3, 9), "2"));
    let older = next(&server);
    server.notify("textDocument/didChange", &change(&b, (3, 8), (3, 20), "2"));
    let hover = json!({ "textDocument": { "uri": protocol::uri_of(&b) },
        "position": { "line": 3, "character": 4 } });
    assert!(server.should_defer("textDocument/hover", &hover));
    server.defer(json!(7), "textDocument/hover", hover);

    // Hold A's real completion until B's new text has been accepted.
    server.publish(older);
    server.resume();
    let stale = take_messages(&written);
    assert!(stale.is_empty(), "obsolete publication or reply: {stale:?}");
    assert!(server.snapshots.is_empty());

    settle(&mut server);
    server.resume();
    let messages = take_messages(&written);
    let reports: Vec<_> = messages
        .iter()
        .filter(|m| m["params"]["uri"] == protocol::uri_of(&b))
        .map(|m| &m["params"]["diagnostics"])
        .collect();
    assert_eq!(reports, [&json!([])]);
    assert!(messages.iter().any(|m| {
        m["id"] == 7
            && m["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains("v : Int"))
    }));
    fs::remove_dir_all(dir).expect("removed");
}

#[test]
fn a_shared_input_invalidates_its_in_flight_reader_but_an_unrelated_job_publishes() {
    let dir = project_dir("dependency-freshness");
    let lib = dir.join("dep");
    fs::create_dir_all(lib.join("src")).expect("library created");
    fs::write(
        lib.join(project::CONFIG_FILE),
        "schema-version = 1\n[package]\nname = \"dep\"\nversion = \"1.0.0\"\n[project]\nkind = \"lib\"\n",
    )
    .expect("manifest written");
    let dependency = lib.join("src/Dep.fl");
    let old = "module Dep exposing (v)\n\nv : Int\nlet v = 1\n";
    fs::write(&dependency, old).expect("source written");

    let app = dir.join("app");
    fs::create_dir_all(app.join("src")).expect("app created");
    fs::write(
        app.join(project::CONFIG_FILE),
        "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\ndep = { path = \"../dep\" }\n",
    )
    .expect("manifest written");
    let reader = app.join("src/App.fl");
    let independent = project_dir("independent-freshness").join("One.fl");
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    server.notify(
        "textDocument/didOpen",
        &opened(
            &reader,
            "module App exposing (x)\nimport Dep exposing (v)\nx : Int\nlet x = v\n",
        ),
    );
    let reader_job = next(&server);
    assert!(reader_job.snapshot.file(&dependency).is_some());
    server.notify(
        "textDocument/didOpen",
        &opened(
            &independent,
            "module One exposing (x)\n\nx : Int\nlet x = 1\n",
        ),
    );
    let independent_job = next(&server);

    let new = "module Dep exposing (v)\n\nv : String\nlet v = \"now a string\"\n";
    server.notify("textDocument/didOpen", &opened(&dependency, new));
    server.publish(reader_job);
    assert!(
        take_messages(&written).is_empty(),
        "the old reader cannot publish"
    );
    server.publish(independent_job);
    let unrelated = take_messages(&written);
    assert!(
        unrelated
            .iter()
            .any(|m| m["params"]["uri"] == protocol::uri_of(&independent))
    );

    settle(&mut server);
    let current = take_messages(&written);
    assert!(
        current
            .iter()
            .any(|m| m["params"]["uri"] == protocol::uri_of(&reader)
                && m["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|ds| ds.iter().any(|d| d["code"] == "FLICK-T003"))),
        "the retried reader sees the dependency's new type: {current:?}"
    );
    fs::remove_dir_all(dir).expect("removed");
    fs::remove_dir_all(independent.parent().expect("project")).expect("removed");
}

#[test]
fn closing_the_last_document_or_changing_a_manifest_settles_deferred_requests() {
    let dir = project_dir("context-settlement");
    let a = dir.join("A.fl");
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    server.notify(
        "textDocument/didOpen",
        &opened(&a, "module A exposing (x)\n\nx : Int\nlet x = 1\n"),
    );
    let before_manifest = next(&server);
    server.defer(json!(1), "workspace/symbol", json!({ "query": "x" }));
    let manifest = dir.join(project::CONFIG_FILE);
    fs::write(
        &manifest,
        "schema-version = 1\n[project]\nkind = \"nope\"\n",
    )
    .expect("manifest changed on disk");
    server.notify(
        "workspace/didChangeWatchedFiles",
        &json!({
            "changes": [{ "uri": protocol::uri_of(&manifest), "type": 2 }]
        }),
    );
    server.publish(before_manifest);
    server.resume();
    assert!(take_messages(&written).is_empty());
    settle(&mut server);
    server.resume();
    let current = take_messages(&written);
    assert!(
        current
            .iter()
            .any(|m| m["id"] == 1 && m["result"].is_array())
    );
    assert!(current.iter().any(|m| {
        m["params"]["uri"] == protocol::uri_of(&manifest)
            && m["params"]["diagnostics"]
                .as_array()
                .is_some_and(|ds| ds.iter().any(|d| d["code"] == "FLICK-P005"))
    }));

    server.notify("textDocument/didChange", &change(&a, (3, 8), (3, 9), "2"));
    let before_close = next(&server);
    server.defer(json!(2), "workspace/symbol", json!({ "query": "x" }));
    server.notify("textDocument/didClose", &document(&a));
    take_messages(&written); // closing the project clears its published diagnostics
    server.publish(before_close);
    server.resume();
    let replies = take_messages(&written);
    assert_eq!(replies.len(), 1, "{replies:?}");
    assert_eq!(replies[0]["id"], 2);
    assert_eq!(replies[0]["result"], json!([]));
    assert!(server.pending.is_empty() && server.snapshots.is_empty());
    fs::remove_dir_all(dir).expect("removed");
}

#[test]
fn opening_a_nested_manifest_while_a_file_moves_refreshes_both_live_projects() {
    let dir = project_dir("moving-context");
    let nested = dir.join("child");
    fs::create_dir_all(&nested).expect("nested directory created");
    let (a, b) = (dir.join("A.fl"), nested.join("B.fl"));
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    server.notify(
        "textDocument/didOpen",
        &opened(&a, "module A exposing (x)\n\nx : Int\nlet x = 1\n"),
    );
    settle(&mut server);
    take_messages(&written);
    server.notify(
        "textDocument/didOpen",
        &opened(&b, "module B exposing (x)\n\nx : Int\nlet x = 1\n"),
    );
    let old = next(&server);
    assert!(old.path == b && old.snapshot.context == Context::Project(canonical(&dir)));
    server.notify(
        "textDocument/didOpen",
        &opened(
            &nested.join(project::CONFIG_FILE),
            "schema-version = 1\n[project]\nkind = \"lib\"\nsource-dir = \".\"\n",
        ),
    );
    server.publish(old);
    assert!(take_messages(&written).is_empty());
    settle(&mut server);
    assert!(
        server
            .snapshots
            .contains_key(&Context::Project(canonical(&dir)))
    );
    assert!(
        server
            .snapshots
            .contains_key(&Context::Project(canonical(&nested)))
    );
    fs::remove_dir_all(dir).expect("removed");
}

#[test]
fn a_dead_worker_settles_held_requests_and_a_new_one_analyzes_the_next_edit() {
    let dir = project_dir("dead-worker");
    let a = dir.join("A.fl");
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    server.notify(
        "textDocument/didOpen",
        &opened(&a, "module A exposing (x)\n\nx : Int\nlet x = 1\n"),
    );
    settle(&mut server);
    take_messages(&written);
    // an idle healthy worker is kept, with its cache
    server.poll();
    assert!(take_messages(&written).is_empty());
    assert_eq!(server.worker.cache_len(), 1);

    server.worker.crash();
    server.notify("textDocument/didChange", &change(&a, (3, 8), (3, 9), "2"));
    assert!(
        !server.pending.is_empty(),
        "the change waits for the worker"
    );
    let hover = json!({ "textDocument": { "uri": protocol::uri_of(&a) },
        "position": { "line": 3, "character": 4 } });
    for id in [7, 8] {
        assert!(server.should_defer("textDocument/hover", &hover));
        server.defer(json!(id), "textDocument/hover", hover.clone());
    }
    server.cancel_request(&json!({ "id": 8 }));
    assert_eq!(
        server.worker.recv_timeout(Duration::from_mins(1)).err(),
        Some(mpsc::RecvTimeoutError::Disconnected)
    );
    server.poll();
    let replies: Vec<_> = take_messages(&written)
        .into_iter()
        .filter(|m| m.get("id").is_some())
        .collect();
    let ids: Vec<_> = replies.iter().map(|m| &m["id"]).collect();
    assert_eq!(ids, [&json!(8), &json!(7)], "{replies:?}");
    assert_eq!(replies[0]["error"]["code"], -32800);
    assert!(
        replies[1]["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|s| s.contains("x : Int")),
        "{replies:?}"
    );
    assert!(server.pending.is_empty() && server.deferred.is_empty());

    let text = "module A exposing (y)\n\ny : String\nlet y = missingValue\n";
    let whole = json!({ "textDocument": { "uri": protocol::uri_of(&a) },
        "contentChanges": [{ "text": text }] });
    server.notify("textDocument/didChange", &whole);
    settle(&mut server);
    let published = take_messages(&written);
    assert!(
        published
            .iter()
            .any(|m| m["params"]["uri"] == protocol::uri_of(&a)
                && m["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|ds| ds.iter().any(|d| d["code"] == "FLICK-T001"))),
        "the new worker publishes the current text: {published:?}"
    );
    server.answer(&json!(9), "textDocument/hover", &hover);
    let reply = take_messages(&written);
    assert!(
        reply[0]["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|s| s.contains("y : String")),
        "{reply:?}"
    );
    fs::remove_dir_all(dir).expect("removed");
}
