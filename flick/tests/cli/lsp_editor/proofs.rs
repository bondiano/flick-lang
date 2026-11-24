use super::*;
use crate::prove_command::{STEP_PROOFS, proof_step_diagnostic};

const PORTS: &str = "module App.Ports exposing (Port)\n\
refined type Port = Float where fun n -> n > 0 && n < 65536\n";

fn codes(diagnostics: &[Value]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| d["code"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn a_failed_proof_step_reports_its_obligation_and_related_ranges() {
    let dir = project(
        "lsp_failed_obligation",
        &[("flick.toml", LIB), ("proofs/App/Laws.fl", STEP_PROOFS)],
    );
    let uri = uri(&dir.join("proofs/App/Laws.fl"));
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
            "uri": uri, "languageId": "flick", "version": 1, "text": STEP_PROOFS,
        } }),
    );
    let diagnostics = lsp.diagnostics(&uri);
    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
    let offset = |needle: &str| STEP_PROOFS.find(needle).expect("fixture text");
    for (name, obligation, argument) in [
        ("wrong_step", "true = false", "trans false "),
        ("wrong_premise", "false = true", "given false "),
    ] {
        let d = proof_step_diagnostic(&diagnostics, name);
        let start = offset(argument) + argument.len();
        assert_eq!(d["code"], "FLICK-T090", "{d}");
        assert_eq!(
            d["range"],
            json!({ "start": position(STEP_PROOFS, start), "end": position(STEP_PROOFS, start + 4) })
        );
        assert!(
            d["message"].as_str().expect("message").contains(obligation),
            "{d}"
        );
        let related = d["relatedInformation"]
            .as_array()
            .expect("related locations");
        let points_to = |declaration: &str| {
            let start = position(STEP_PROOFS, offset(declaration));
            related
                .iter()
                .any(|r| r["location"]["uri"] == uri && r["location"]["range"]["start"] == start)
        };
        assert!(points_to(&format!("proof {name}")), "{d}");
        if name == "wrong_premise" {
            assert!(points_to("proof given"), "{d}");
        }
    }
    assert_eq!(
        proof_step_diagnostic(&diagnostics, "refuted")["code"],
        "FLICK-T080"
    );

    let fixed = STEP_PROOFS
        .replace("trans false refl refl", "refl")
        .replace("given false refl", "refl");
    lsp.notify(
        "textDocument/didChange",
        &json!({ "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": fixed }] }),
    );
    assert_eq!(codes(&lsp.diagnostics(&uri)), ["FLICK-T080"]);
    lsp.close();
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

#[test]
fn a_proof_declaration_follows_editor_changes() {
    let proved = concat!(
        "module App.Main exposing (..)\n",
        "proof rebuilt (v : Option a)\n",
        "where equal (match v with | None -> None | Some x -> Some x) v =\n",
        "  match v with\n",
        "  | None -> refl\n",
        "  | Some y -> refl\n",
    );
    let unsplit = proved.replace(
        "  match v with\n  | None -> refl\n  | Some y -> refl\n",
        "  refl\n",
    );
    let dir = project(
        "lsp_proof_declarations",
        &[("flick.toml", LIB), ("src/App/Main.fl", proved)],
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
    // without the split the match on `v` does not compute: the goal is left open
    for (version, (text, expected)) in [(unsplit.as_str(), vec!["FLICK-T081"]), (proved, vec![])]
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
            let start = text.rfind("refl").expect("the proof");
            assert_eq!(d["range"]["start"], position(text, start), "{d}");
        }
    }
    lsp.close();
}

/// A proof module follows an edit of the body of a definition it names in
/// another module, though the signature stays the same.
#[test]
fn a_proof_follows_a_body_edit_of_another_module() {
    let steps =
        "module App.Steps exposing (..)\n\ntwice : (a -> a) -> a -> a\nlet twice f x = f (f x)\n";
    let proof = "module App.Laws exposing (..)\n\nimport App.Steps exposing (twice)\n\nproof unfolded (f : a -> a) (x : a) where equal (twice f x) (f (f x)) = refl\n";
    let dir = project(
        "lsp_proof_across_modules",
        &[
            ("flick.toml", LIB),
            ("src/App/Steps.fl", steps),
            ("proofs/App/Laws.fl", proof),
        ],
    );
    let laws = uri(&dir.join("proofs/App/Laws.fl"));
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
            "uri": laws, "languageId": "flick", "version": 1, "text": proof,
        } }),
    );
    assert_eq!(codes(&lsp.diagnostics(&laws)), Vec::<String>::new());
    let source = uri(&dir.join("src/App/Steps.fl"));
    let broken = steps.replace("f (f x)", "f x");
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
            "uri": source, "languageId": "flick", "version": 1, "text": broken,
        } }),
    );
    assert_eq!(codes(&lsp.diagnostics(&laws)), ["FLICK-T081"]);
    lsp.notify(
        "textDocument/didChange",
        &json!({ "textDocument": { "uri": source, "version": 2 },
            "contentChanges": [{ "text": steps }] }),
    );
    assert_eq!(codes(&lsp.diagnostics(&laws)), Vec::<String>::new());
    lsp.close();
}
