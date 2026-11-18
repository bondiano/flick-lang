use super::*;

const PORTS: &str = "module App.Ports exposing (Port)\n\
refined type Port = Float where fun n -> n > 0 && n < 65536\n";

fn codes(diagnostics: &[Value]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| d["code"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn a_proof_of_an_imported_type_follows_editor_changes() {
    let proved = "module App.Main exposing (as_port)\nimport App.Ports exposing (Port)\n\
as_port : Float -> Option Port\n\
let as_port n = if n > 0 && n < 65536 then Some (Port.prove n) else None\n";
    let incomplete = proved.replace("n > 0 && n < 65536", "n > 0");
    let dir = project(
        "lsp_imported_proofs",
        &[
            ("flick.toml", LIB),
            ("src/App/Ports.fl", PORTS),
            ("src/App/Main.fl", proved),
        ],
    );
    let main = uri(&dir.join("src/App/Main.fl"));
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
            "uri": main, "languageId": "flick", "version": 1, "text": proved,
        } }),
    );
    assert_eq!(codes(&lsp.diagnostics(&main)), Vec::<String>::new());
    for (version, (text, expected)) in [(incomplete.as_str(), vec!["FLICK-T077"]), (proved, vec![])]
        .into_iter()
        .enumerate()
    {
        lsp.notify(
            "textDocument/didChange",
            &json!({ "textDocument": { "uri": main, "version": version + 2 },
                "contentChanges": [{ "text": text }] }),
        );
        let diagnostics = lsp.diagnostics(&main);
        assert_eq!(codes(&diagnostics), expected, "{diagnostics:?}");
        if let Some(d) = diagnostics.first() {
            let message = d["message"].as_str().unwrap_or_default();
            assert!(message.contains("`n < 65536`"), "{message}");
            let start = text.find("Port.prove n").expect("proof");
            assert_eq!(d["range"]["start"], position(text, start), "{d}");
        }
    }
    // a stricter check in the open module of the type breaks the proof
    let ports = uri(&dir.join("src/App/Ports.fl"));
    let stricter = PORTS.replace("n < 65536", "n < 1024");
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
            "uri": ports, "languageId": "flick", "version": 1, "text": stricter,
        } }),
    );
    let diagnostics = lsp.diagnostics(&main);
    assert_eq!(codes(&diagnostics), ["FLICK-T077"], "{diagnostics:?}");
    lsp.close();
}
