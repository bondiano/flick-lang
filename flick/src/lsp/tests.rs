use std::fs;
use std::sync::{Arc, Mutex};

use super::*;
use serde_json::json;

/// A server under test: it records what it writes and owns no worker of its
/// own start.
fn server(out: Box<dyn Write>) -> Server {
    Server {
        docs: HashMap::new(),
        snapshots: HashMap::new(),
        latest: None,
        caches: Caches::new(),
        published: HashMap::new(),
        out,
        registered: None,
        requests: 0,
        worker: Worker::new(),
        pending: HashMap::new(),
        generation: 0,
        deferred: Vec::new(),
    }
}

/// Waits for the analyses the server queued and publishes them, as the loop does.
fn settle(server: &mut Server) {
    while !server.pending.is_empty() {
        let done = server
            .worker
            .recv_timeout(Duration::from_mins(1))
            .expect("the analysis finishes");
        server.publish(&done.path, done.generation, done.snapshot);
    }
    server.worker.synchronize();
}

fn document(path: &Path) -> Value {
    json!({ "textDocument": { "uri": protocol::uri_of(path), "languageId": "flick", "version": 1,
        "text": "module A exposing (x)\n\nx : Number\nlet x = 1\n" } })
}

fn opened(path: &Path, text: &str) -> Value {
    json!({ "textDocument": { "uri": protocol::uri_of(path), "languageId": "flick", "version": 1, "text": text } })
}

/// One incremental replacement of `range` in `path`'s open text.
fn change(path: &Path, start: (u32, u32), end: (u32, u32), text: &str) -> Value {
    json!({ "textDocument": { "uri": protocol::uri_of(path) }, "contentChanges": [
        { "range": { "start": { "line": start.0, "character": start.1 },
                     "end": { "line": end.0, "character": end.1 } },
          "text": text } ] })
}

fn written_text(written: &Written) -> String {
    String::from_utf8(written.0.lock().expect("not poisoned").clone()).expect("UTF-8")
}

#[test]
fn a_closed_standalone_file_leaves_no_cache_behind() {
    let mut server = server(Box::new(io::sink()));
    let dir = env::temp_dir().join(format!("flick-lsp-caches-{}", process::id()));
    for name in ["A.fl", "B.fl", "C.fl"] {
        let params = document(&dir.join(name));
        server.notify("textDocument/didOpen", &params);
        settle(&mut server);
        assert_eq!(server.worker.cache_len(), 1, "{name} opened");
        server.notify("textDocument/didClose", &params);
        settle(&mut server);
        assert_eq!(server.worker.cache_len(), 0, "{name} closed");
    }
}

#[test]
fn only_the_project_closed_last_keeps_its_cache() {
    let mut server = server(Box::new(io::sink()));
    let dir = env::temp_dir().join(format!("flick-lsp-projects-{}", process::id()));
    let project = |name: &str| {
        let root = dir.join(name);
        fs::create_dir_all(&root).expect("created");
        fs::write(
            root.join(project::CONFIG_FILE),
            "schema-version = 1\n\n[project]\nkind = \"lib\"\nsource-dir = \".\"\n",
        )
        .expect("written");
        document(&root.join("A.fl"))
    };
    let (kept, first, second) = (project("kept"), project("first"), project("second"));
    server.notify("textDocument/didOpen", &kept);
    settle(&mut server);
    for params in [&first, &second] {
        server.notify("textDocument/didOpen", params);
        settle(&mut server);
        server.notify("textDocument/didClose", params);
        settle(&mut server);
        // the open project and the one closed now: the next document opened
        // in it finds its modules
        assert_eq!(server.worker.cache_len(), 2, "closed");
    }
    server.notify("textDocument/didClose", &kept);
    settle(&mut server);
    assert_eq!(server.worker.cache_len(), 1, "every project closed");
    fs::remove_dir_all(&dir).expect("removed");
}

/// What a server writes, kept for the test to read.
#[derive(Clone, Default)]
struct Written(Arc<Mutex<Vec<u8>>>);

impl Write for Written {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("not poisoned")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn the_extensions_of_loaded_files_are_registered_once_where_the_client_lets_it() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let loaded = [
        Path::new("/p/src/App/Main.fl"),
        Path::new("/p/src/App/Words.pairs"),
    ];
    // a client that registers nothing later gets no request
    server.register_loaded(loaded.into_iter());
    assert!(written.0.lock().expect("not poisoned").is_empty());
    let initialize = json!({ "capabilities": { "textDocument": { "synchronization": { "dynamicRegistration": true } } } });
    server
        .request("initialize", &initialize)
        .expect("initialized");
    server.register_loaded(loaded.into_iter());
    server.register_loaded(loaded.into_iter());
    let out = written_text(&written);
    assert_eq!(out.matches("client/registerCapability").count(), 1, "{out}");
    assert!(
        out.contains("**/*.pairs") && !out.contains("**/*.fl"),
        "{out}"
    );
}

#[test]
fn a_panic_in_a_message_leaves_the_server_running_with_fresh_caches() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let dir = env::temp_dir().join(format!("flick-lsp-panic-{}", process::id()));
    let params = document(&dir.join("A.fl"));
    server.notify("textDocument/didOpen", &params);
    settle(&mut server);
    assert_eq!(server.worker.cache_len(), 1, "analyzed");
    let caught = server.guarded::<()>(|_| panic!("a defect in the checker"));
    assert!(caught.is_none(), "the panic is caught");
    assert!(
        server.snapshots.is_empty(),
        "a half-built analysis is dropped"
    );
    written.0.lock().expect("not poisoned").clear();
    server.guarded(|s| s.notify("textDocument/didSave", &params));
    settle(&mut server);
    let out = written_text(&written);
    assert!(out.contains("textDocument/publishDiagnostics"), "{out}");
}

#[test]
fn the_server_advertises_incremental_synchronization() {
    let mut server = server(Box::new(io::sink()));
    let reply = server
        .request("initialize", &json!({ "capabilities": {} }))
        .expect("initialized");
    assert_eq!(reply["capabilities"]["textDocumentSync"]["change"], 2);
}

#[test]
fn a_change_replaces_only_the_range_it_names() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let dir = env::temp_dir().join(format!("flick-lsp-changes-{}", process::id()));
    let path = dir.join("A.fl");
    // `x = 1` is on line 3; only the `1` is replaced, the rest stays
    server.notify("textDocument/didOpen", &document(&path));
    settle(&mut server);
    // two changes in one notification: the second reads the text of the first
    let notification = json!({ "textDocument": { "uri": protocol::uri_of(&path) },
        "contentChanges": [
            { "range": { "start": { "line": 3, "character": 8 }, "end": { "line": 3, "character": 9 } }, "text": "41" },
            { "range": { "start": { "line": 3, "character": 9 }, "end": { "line": 3, "character": 10 } }, "text": "2" } ] });
    server.notify("textDocument/didChange", &notification);
    settle(&mut server);
    assert_eq!(
        server.docs[&path],
        "module A exposing (x)\n\nx : Number\nlet x = 42\n"
    );
}

#[test]
fn a_change_counts_utf16_units_not_bytes() {
    let mut server = server(Box::new(io::sink()));
    let dir = env::temp_dir().join(format!("flick-lsp-utf16-{}", process::id()));
    let path = dir.join("A.fl");
    // `é` is one UTF-16 unit and two UTF-8 bytes; `𝄞` is two units and four bytes
    server.notify("textDocument/didOpen", &opened(&path, "x = \"é𝄞\"\n"));
    settle(&mut server);
    // the `𝄞` starts at UTF-16 unit 6, after `x = "é`
    let notification = change(&path, (0, 6), (0, 8), "!");
    server.notify("textDocument/didChange", &notification);
    settle(&mut server);
    assert_eq!(server.docs[&path], "x = \"é!\"\n");
}

#[test]
fn a_change_without_a_range_replaces_the_whole_document() {
    let mut server = server(Box::new(io::sink()));
    let dir = env::temp_dir().join(format!("flick-lsp-full-{}", process::id()));
    let path = dir.join("A.fl");
    server.notify("textDocument/didOpen", &document(&path));
    settle(&mut server);
    let notification = json!({ "textDocument": { "uri": protocol::uri_of(&path) },
        "contentChanges": [ { "text": "module B exposing (y)\n" } ] });
    server.notify("textDocument/didChange", &notification);
    settle(&mut server);
    assert_eq!(server.docs[&path], "module B exposing (y)\n");
}

#[test]
fn a_change_analysis_holds_requests_until_it_is_published() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let dir = env::temp_dir().join(format!("flick-lsp-defer-{}", process::id()));
    let path = dir.join("A.fl");
    server.notify("textDocument/didOpen", &document(&path));
    settle(&mut server);
    // the analysis of the change runs in the background: the request that
    // reads the document waits for it instead of blocking the message loop
    server.notify(
        "textDocument/didChange",
        &change(&path, (3, 8), (3, 9), "2"),
    );
    assert!(!server.pending.is_empty(), "the change is being analyzed");
    let hover = json!({ "textDocument": { "uri": protocol::uri_of(&path) },
        "position": { "line": 3, "character": 8 } });
    let id = json!(7);
    assert!(server.should_defer("textDocument/hover", &hover));
    server.defer(id.clone(), "textDocument/hover", hover.clone());
    // the client cancels it: the held request is answered with the protocol's code
    server.cancel_request(&json!({ "id": 7 }));
    assert!(server.deferred.is_empty());
    let out = written_text(&written);
    assert!(out.contains("-32800"), "{out}");
}

#[test]
fn a_watched_file_change_analyzes_the_documents_that_read_it() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let dir = env::temp_dir().join(format!("flick-lsp-watched-{}", process::id()));
    fs::create_dir_all(&dir).expect("created");
    fs::write(
        dir.join(project::CONFIG_FILE),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\nsource-dir = \".\"\n",
    )
    .expect("written");
    let path = dir.join("A.fl");
    fs::write(&path, "module A exposing (x)\n\nx : Number\nlet x = 1\n").expect("written");
    server.notify("textDocument/didOpen", &document(&path));
    settle(&mut server);
    written.0.lock().expect("not poisoned").clear();
    fs::write(&path, "module A exposing (x)\n\nx : Number\nlet x = 2\n").expect("written");
    let notification = json!({ "changes": [ { "uri": protocol::uri_of(&path), "type": 2 } ] });
    server.notify("workspace/didChangeWatchedFiles", &notification);
    settle(&mut server);
    let out = written_text(&written);
    assert!(out.contains("textDocument/publishDiagnostics"), "{out}");
    assert!(
        server
            .snapshots
            .contains_key(&Context::Project(canonical(&dir))),
        "the project was analyzed again"
    );
}

#[test]
fn a_dotted_completion_waits_for_the_background_analysis_and_reads_its_snapshot() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let dir = env::temp_dir().join(format!("flick-lsp-dot-defer-{}", process::id()));
    let path = dir.join("A.fl");
    let source = "module A exposing (User, run)\n\ntype alias User = { name : String; age : Float }\n\nrun : User -> String\nlet run user = user.name\n";
    server.notify("textDocument/didOpen", &opened(&path, source));
    settle(&mut server);
    server.notify(
        "textDocument/didChange",
        &change(&path, (5, 20), (5, 24), ""),
    );
    assert!(!server.pending.is_empty(), "the change is being analyzed");
    let completion = json!({
        "textDocument": { "uri": protocol::uri_of(&path) },
        "position": { "line": 5, "character": 20 }
    });
    assert!(server.should_defer("textDocument/completion", &completion));
    server.defer(json!(9), "textDocument/completion", completion);
    written.0.lock().expect("not poisoned").clear();
    settle(&mut server);
    server.resume();
    assert!(server.deferred.is_empty());
    assert!(
        server.caches.is_empty(),
        "dotted completion must not run synchronous analyze on the server thread"
    );
    let out = written_text(&written);
    assert!(
        out.contains("\"id\":9") && out.contains("\"name\"") && out.contains("\"age\""),
        "{out}"
    );
}

#[test]
fn result_block_lsp_hover_actions_and_edits_handle_utf16_without_stale_results() {
    let written = Written::default();
    let mut server = server(Box::new(written.clone()));
    let dir = env::temp_dir().join(format!("flick-lsp-result-{}", process::id()));
    let path = dir.join("A.fl");
    // "é𝄞" on line 5 before `result { ... }`: 6 UTF-8 bytes, 3 UTF-16 units
    let source = "module A exposing (run)\n\ntype error Boom = Boom\n\nrun : Bool ->{} (String, Result Boom String)\nlet run flag = (\"é𝄞\", result { if flag then Fail.fail Boom else \"ok\" })\n";
    server.notify("textDocument/didOpen", &opened(&path, source));
    settle(&mut server);

    // Hover on `result` keyword on line 5 using UTF-16 character column
    let line5 = source.lines().nth(5).expect("line 5");
    let byte_col = line5.find("result {").expect("result");
    let utf16_col: usize = line5[..byte_col].chars().map(char::len_utf16).sum();
    let hover = json!({
        "textDocument": { "uri": protocol::uri_of(&path) },
        "position": { "line": 5, "character": utf16_col + 1 }
    });
    let reply = server
        .request("textDocument/hover", &hover)
        .expect("hover reply");
    let out = reply.to_string();
    assert!(
        out.contains("result { … } : Result Boom String")
            && out.contains("Remaining services: `none`"),
        "{out}"
    );

    // Code action on `result` offers expansion to `try … with`
    let code_action = json!({
        "textDocument": { "uri": protocol::uri_of(&path) },
        "range": {
            "start": { "line": 5, "character": utf16_col },
            "end": { "line": 5, "character": utf16_col + 6 }
        },
        "context": { "diagnostics": [] }
    });
    let reply = server
        .request("textDocument/codeAction", &code_action)
        .expect("codeAction reply");
    let out = reply.to_string();
    assert!(out.contains("expand `result"), "{out}");

    // Edit `result { ... }` into an ordinary variable `result` and verify no stale keyword hover
    let edited = "module A exposing (run)\n\ntype error Boom = Boom\n\nrun : Bool ->{} (String, String)\nlet run _ =\n  let result = \"ok\"\n  (\"é𝄞\", result)\n";
    let full_change = json!({
        "textDocument": { "uri": protocol::uri_of(&path), "version": 2 },
        "contentChanges": [{ "text": edited }]
    });
    server.notify("textDocument/didChange", &full_change);
    settle(&mut server);

    let edited_line7 = edited.lines().nth(7).expect("edited line 7");
    let edited_col: usize = edited_line7[..edited_line7.rfind("result").expect("result var")]
        .chars()
        .map(char::len_utf16)
        .sum();
    let hover_after = json!({
        "textDocument": { "uri": protocol::uri_of(&path) },
        "position": { "line": 7, "character": edited_col + 1 }
    });
    let reply = server
        .request("textDocument/hover", &hover_after)
        .expect("hover reply after edit");
    let out = reply.to_string();
    assert!(
        out.contains("result : String") && !out.contains("result { … }"),
        "{out}"
    );
}
