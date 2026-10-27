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
