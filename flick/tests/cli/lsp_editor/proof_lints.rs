use super::*;
use crate::prove_lints::{PROOF_LINT_CONFIG, PROOF_LINTS, by_code};

#[test]
fn proof_lints_and_quick_fixes_follow_checked_proofs() {
    let path = "proofs/App/Laws.fl";
    let dir = project(
        "lsp_proof_lints",
        &[("flick.toml", PROOF_LINT_CONFIG), (path, PROOF_LINTS)],
    );
    let document = uri(&dir.join(path));
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": document, "languageId": "flick", "version": 1, "text": PROOF_LINTS } }));
    let found = lsp.diagnostics(&document);
    assert_eq!(found.len(), 2, "{found:?}");
    let swaps = by_code(&found, "FLICK-T091");
    let unused = by_code(&found, "FLICK-T092");
    let premise = PROOF_LINTS.find("extra :").expect("premise binder");
    assert_eq!(
        unused["range"],
        json!({ "start": position(PROOF_LINTS, premise), "end": position(PROOF_LINTS, premise + 5) })
    );
    assert_eq!(unused["severity"], 2);

    let action = |lsp: &mut Lsp, d: &Value| {
        let reply = lsp.request("textDocument/codeAction", &json!({
            "textDocument": { "uri": document }, "range": d["range"], "context": { "diagnostics": [d] },
        }));
        reply["result"]
            .as_array()
            .expect("actions")
            .iter()
            .find(|a| a["diagnostics"][0]["code"] == d["code"])
            .cloned()
    };
    assert!(action(&mut lsp, unused).is_none());
    let fix = action(&mut lsp, swaps).expect("double symmetry quick fix");
    let edit = &fix["edit"]["changes"][&document][0];
    assert_eq!(fix["kind"], "quickfix", "{fix}");
    assert_eq!(fix["isPreferred"], true, "{fix}");
    let source = "symm (symm h)";
    let start = PROOF_LINTS.find(source).expect("redundant proof");
    let end = start + source.len();
    let range = json!({ "start": position(PROOF_LINTS, start), "end": position(PROOF_LINTS, end) });
    assert_eq!(swaps["range"], range);
    assert_eq!(edit["range"], range);
    assert_eq!(edit["newText"], "h");
    let mut fixed = PROOF_LINTS.to_owned();
    fixed.replace_range(start..end, edit["newText"].as_str().expect("replacement"));

    let invalid = PROOF_LINTS.replace("equal x y = symm (symm h)", "equal y x = symm (symm h)");
    let incomplete = PROOF_LINTS.replace(source, "");
    for (version, text, expected) in [
        (2, &fixed, None),
        (3, &invalid, Some("FLICK-T081")),
        (4, &incomplete, None),
    ] {
        lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": document, "version": version }, "contentChanges": [{ "text": text }] }));
        let found = lsp.diagnostics(&document);
        assert!(found.iter().all(|d| d["code"] != "FLICK-T091"), "{found:?}");
        if let Some(expected) = expected {
            assert_eq!(by_code(&found, expected)["severity"], 1);
        } else if version == 2 {
            assert_eq!(found.len(), 1, "{found:?}");
            assert_eq!(found[0]["code"], "FLICK-T092");
        }
    }

    let commented = PROOF_LINTS.replace(source, "symm (symm /* Keep this reason. */ h)");
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": document, "version": 5 }, "contentChanges": [{ "text": commented }] }));
    let found = lsp.diagnostics(&document);
    let swaps = by_code(&found, "FLICK-T091");
    assert!(action(&mut lsp, swaps).is_none());
    lsp.close();
}
