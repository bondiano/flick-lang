use super::*;

const STEPS: &str = "module App.Steps exposing (..)
rebuild : Option Bool -> Option Bool
let rebuild v =
  match v with
  | None -> None
  | Some x -> Some x
";

const LEMMA: &str = "module App.Base exposing (..)
import App.Steps exposing (rebuild)
proof rebuild_identity (v : Option Bool) where equal (rebuild v) v =
  match v with
  | None -> refl
  | Some _ -> refl
";

const OPEN: &str = "module App.Uses exposing (..)\n\
import App.Steps exposing (rebuild)\n\
import App.Base as B\n\
\n\
proof rebuilt (w : Option Bool) where equal (rebuild w) w =\n";

const NESTED: &str = "module App.Uses exposing (..)\n\
proof nested (x : Bool) (y : Bool) (h : equal y x) where equal y x = symm true\n\
proof check_value (x : Bool) where equal x x = trans x refl refl\n";

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
    cancel_search_and_request_hover(&mut lsp, &params);

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
    let after = lsp.request("flick/searchProof", &params);
    assert_eq!(after["result"]["status"], "no-candidate", "{after}");
    assert_eq!(after["result"]["suggestions"], json!([]));
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": dependency } }),
    );
    assert_nested_argument_edit_checks(&mut lsp, &document, &dir, path);
    lsp.close();
}

#[test]
fn proof_search_inserts_an_indented_body_from_the_next_line() {
    let text = "module App.Uses exposing (..)\nproof reflexive (x : Bool) where equal x x =\n\n";
    let path = "proofs/App/Uses.fl";
    let dir = project(
        "lsp_proof_search_blank_line",
        &[("flick.toml", LIB), (path, text)],
    );
    let document = uri(&dir.join(path));
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": document, "languageId": "flick", "version": 1, "text": text } }));
    let _ = lsp.diagnostics(&document);
    let caret = text.len() - 1;
    let position = position(text, caret);
    let reply = lsp.request(
        "flick/searchProof",
        &json!({ "textDocument": { "uri": document }, "position": position }),
    );
    assert_eq!(reply["result"]["status"], "found", "{reply}");
    let suggestion = &reply["result"]["suggestions"][0];
    assert_eq!(
        suggestion["range"],
        json!({ "start": position, "end": position })
    );
    let mut inserted = text.to_owned();
    inserted.insert_str(caret, suggestion["text"].as_str().expect("proof text"));
    fs::write(dir.join(path), inserted).expect("insert proof body");
    let proved = flick_in(&dir, &["prove"]);
    assert!(
        proved.status.success(),
        "{reply}\n{}",
        String::from_utf8_lossy(&proved.stderr)
    );
    lsp.close();
}

fn assert_nested_argument_edit_checks(lsp: &mut Lsp, document: &str, dir: &Path, path: &str) {
    let (start, end) = {
        let start = NESTED.rfind("true").expect("incomplete argument");
        (start, start + 4)
    };
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": document, "version": 2 }, "contentChanges": [{ "text": NESTED }] }));
    let ordinary_value = NESTED.rfind("trans x").expect("middle value") + "trans ".len();
    let reply = lsp.request(
        "flick/searchProof",
        &json!({
            "textDocument": { "uri": document }, "position": position(NESTED, ordinary_value),
        }),
    );
    assert_eq!(reply["result"]["status"], "no-candidate", "{reply}");
    let reply = lsp.request(
        "flick/searchProof",
        &json!({
            "textDocument": { "uri": document }, "position": position(NESTED, start + 1),
        }),
    );
    let result = &reply["result"];
    assert_eq!(result["version"], 2, "{reply}");
    assert_eq!(result["status"], "found", "{reply}");
    let suggestion = &result["suggestions"][0];
    assert_eq!(
        suggestion["range"],
        json!({
            "start": position(NESTED, start), "end": position(NESTED, end),
        })
    );
    let mut fixed = NESTED.to_owned();
    fixed.replace_range(start..end, suggestion["text"].as_str().expect("proof text"));
    fs::write(dir.join(path), fixed).expect("insert nested proof suggestion");
    let proved = flick_in(dir, &["prove"]);
    assert!(
        proved.status.success(),
        "{reply}\n{}",
        String::from_utf8_lossy(&proved.stderr)
    );
}

fn cancel_search_and_request_hover(lsp: &mut Lsp, params: &Value) {
    let goal_value = OPEN.rfind("(rebuild w)").expect("goal operand") + 2;
    let hover_params =
        json!({ "textDocument": params["textDocument"], "position": position(OPEN, goal_value) });
    for attempt in 0..3 {
        let id = format!("cancel-search-{attempt}");
        lsp.send(
            &json!({ "jsonrpc": "2.0", "id": id, "method": "flick/searchProof", "params": params }),
        );
        lsp.notify("$/cancelRequest", &json!({ "id": id }));
        let (hover, intervening) =
            lsp.request_with_notifications("textDocument/hover", &hover_params);
        assert!(hover.get("result").is_some(), "hover response: {hover}");
        let cancelled = intervening
            .into_iter()
            .find(|message| message["id"] == id)
            .unwrap_or_else(|| {
                loop {
                    let message = lsp.receive();
                    if message["id"] == id {
                        break message;
                    }
                }
            });
        if cancelled["error"]["code"] == -32800 {
            return;
        }
        // A terminal reply sent before cancellation is a valid race. Retry
        // so this exercise also sees a request cancelled while in flight.
        assert_eq!(cancelled["result"]["status"], "found", "{cancelled}");
    }
    panic!("all searches finished before cancellation");
}
