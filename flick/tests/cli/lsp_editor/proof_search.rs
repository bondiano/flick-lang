use super::*;

const STEPS: &str = "module App.Steps exposing (..)\n\
rebuild : Option Bool -> Option Bool\n\
let rebuild v =\n\
  match v with\n\
  | None -> None\n\
  | Some x -> Some x\n";

const LEMMA: &str = "module App.Base exposing (..)\n\
import App.Steps exposing (rebuild)\n\
proof rebuild_identity (v : Option Bool) where equal (rebuild v) v =\n\
  match v with\n\
  | None -> refl\n\
  | Some _ -> refl\n";

const OPEN: &str = "module App.Uses exposing (..)\n\
import App.Steps exposing (rebuild)\n\
import App.Base as B\n\
\n\
proof rebuilt (w : Option Bool) where equal (rebuild w) w =\n";

#[test]
fn proof_search_inserts_a_checked_imported_term_and_invalidates_it_on_a_dependency_edit() {
    let path = "proofs/App/Uses.fl";
    let steps = "src/App/Steps.fl";
    let dir = project(
        "lsp_explicit_proof_search",
        &[
            ("flick.toml", LIB),
            (steps, STEPS),
            ("proofs/App/Base.fl", LEMMA),
            (path, OPEN),
        ],
    );
    let document = uri(&dir.join(path));
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": document, "languageId": "flick", "version": 1, "text": OPEN } }));
    let _ = lsp.diagnostics(&document);
    let at = OPEN.len() - 1;
    let text_document = json!({ "uri": document });
    let cursor = position(OPEN, at);
    let params = json!({ "textDocument": text_document, "position": cursor });
    let found = lsp.request("flick/searchProof", &params);
    let result = &found["result"];
    assert_eq!(result["version"], 1, "{found}");
    assert_eq!(result["position"], cursor, "{found}");
    assert_eq!(result["status"], "found", "{found}");
    let suggestions = result["suggestions"].as_array().expect("suggestions");
    let suggestion = suggestions
        .iter()
        .find(|s| {
            s["text"]
                .as_str()
                .is_some_and(|text| text.trim() == "B.rebuild_identity w")
        })
        .unwrap_or_else(|| panic!("missing checked lemma: {found}"));
    assert_eq!(
        suggestion["range"],
        json!({ "start": cursor, "end": cursor })
    );
    let repeated = lsp.request("flick/searchProof", &params);
    assert_eq!(repeated["result"]["suggestions"], result["suggestions"]);
    let validate =
        json!({ "textDocument": text_document, "version": 1, "generation": result["generation"] });
    assert_eq!(
        lsp.request("flick/validateProofSearch", &validate)["result"],
        true
    );

    let mut inserted = OPEN.to_owned();
    inserted.insert_str(at, suggestion["text"].as_str().expect("plain proof text"));
    fs::write(dir.join(path), inserted).expect("insert proof suggestion");
    let proved = flick_in(&dir, &["prove"]);
    assert!(
        proved.status.success(),
        "{}",
        String::from_utf8_lossy(&proved.stderr)
    );

    let dependency = uri(&dir.join(steps));
    let changed = STEPS.replace("Some x -> Some x", "Some _ -> None");
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": dependency, "languageId": "flick", "version": 1, "text": changed } }));
    assert_eq!(
        lsp.request("flick/validateProofSearch", &validate)["result"],
        false
    );
    let _ = lsp.diagnostics(&document);
    let after = lsp.request("flick/searchProof", &params);
    assert_eq!(after["result"]["status"], "no-candidate", "{after}");
    assert_eq!(after["result"]["suggestions"], json!([]));
    lsp.close();
}
