use super::*;

#[test]
fn logs_respect_filter_without_leaking_rpc_payloads_into_either_stream() {
    const PRIVATE_SOURCE: &str = "private-source-sentinel-4821";
    const PRIVATE_RPC: &str = "private-rpc-sentinel-9183";
    let source = format!(
        "module App.Main exposing (value)\n\nvalue : String\nlet value = \"{PRIVATE_SOURCE}\"\n"
    );
    let dir = project(
        "lsp_logging",
        &[("flick.toml", LIB), ("src/App/Main.fl", &source)],
    );
    let file = dir.join("src/App/Main.fl");
    for filter in ["flick::lsp=debug", "off"] {
        let log = dir.join("stderr.log");
        let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["lsp", "--stdio"])
            .current_dir(&dir)
            .env("RUST_LOG", filter)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(&log).expect("log file"))
            .spawn()
            .expect("flick lsp");
        let mut lsp = Lsp {
            input: child.stdin.take().expect("stdin"),
            output: BufReader::new(child.stdout.take().expect("stdout")),
            next: 0,
            child,
        };
        lsp.request(
            "initialize",
            &json!({ "capabilities": {}, "initializationOptions": { "private": PRIVATE_RPC } }),
        );
        lsp.notify(
            "textDocument/didOpen",
            &json!({ "textDocument": {
            "uri": uri(&file), "languageId": "flick", "version": 1, "text": source,
        } }),
        );
        let reply = lsp.request(
            "flick/unsupported",
            &json!({ "textDocument": { "uri": uri(&file) }, "position": { "line": 3, "character": 5 } }),
        );
        assert_eq!(reply["error"]["code"], -32601);
        lsp.close();
        let logs = fs::read_to_string(log).expect("logs");
        for private in [PRIVATE_SOURCE, PRIVATE_RPC] {
            assert!(!logs.contains(private), "{filter}: private payload logged");
        }
        if filter == "off" {
            assert!(logs.is_empty(), "{logs}");
        } else {
            for context in [
                "DEBUG",
                "flick/unsupported",
                &uri(&file),
                "elapsed=",
                "code=-32601",
            ] {
                assert!(logs.contains(context), "missing {context}: {logs}");
            }
            assert!(!logs.contains('\u{1b}'), "ANSI escapes in stderr: {logs}");
        }
    }
}

/// A request reads the analysis of its own project, whichever project the
/// request before it was of; a change of a document is analyzed in what
/// reads the document, and leaves the analysis of every other project.
#[test]
fn each_project_keeps_its_analysis_until_a_document_changes() {
    let source = "module App.Main exposing (value)\n\nvalue : Float\nlet value = 1\n";
    let dir = project(
        "lsp_snapshots",
        &[
            ("a/flick.toml", LIB),
            ("a/src/App/Main.fl", source),
            ("b/flick.toml", LIB),
            ("b/src/App/Main.fl", source),
        ],
    );
    let [a, b] = ["a", "b"].map(|member| dir.join(member).join("src/App/Main.fl"));
    let log = dir.join("stderr.log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["lsp", "--stdio"])
        .current_dir(&dir)
        .env("RUST_LOG", "flick::lsp=debug")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(fs::File::create(&log).expect("log file"))
        .spawn()
        .expect("flick lsp");
    let mut lsp = Lsp {
        input: child.stdin.take().expect("stdin"),
        output: BufReader::new(child.stdout.take().expect("stdout")),
        next: 0,
        child,
    };
    lsp.request("initialize", &json!({ "capabilities": {} }));
    for file in [&a, &b] {
        lsp.notify(
            "textDocument/didOpen",
            &json!({ "textDocument": { "uri": uri(file), "languageId": "flick", "version": 1, "text": source } }),
        );
    }
    let hover = |lsp: &mut Lsp, file: &Path| {
        let at = json!({ "textDocument": { "uri": uri(file) }, "position": { "line": 3, "character": 5 } });
        lsp.request("textDocument/hover", &at)["result"]["contents"]["value"].to_string()
    };
    for file in [&a, &b, &a] {
        assert!(hover(&mut lsp, file).contains("Float"));
    }
    let changed = source
        .replace("Float", "String")
        .replace("= 1", "= \"one\"");
    lsp.notify(
        "textDocument/didChange",
        &json!({ "textDocument": { "uri": uri(&a), "version": 2 }, "contentChanges": [{ "text": changed }] }),
    );
    assert!(hover(&mut lsp, &a).contains("String"), "the change is read");
    assert!(hover(&mut lsp, &b).contains("Float"));
    lsp.close();
    // no request analyzed anew: opening and changing a document did
    let logs = fs::read_to_string(&log).expect("logs");
    let selected = |cached: bool| logs.matches(&format!("cached={cached}")).count();
    assert_eq!((selected(true), selected(false)), (5, 0), "{logs}");
}
