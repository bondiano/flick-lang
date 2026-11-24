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

fn goal_at(lsp: &mut Lsp, uri: &str, text: &str, at: usize) -> Value {
    let response = lsp.request(
        "flick/proofAt",
        &json!({
            "textDocument": { "uri": uri }, "position": position(text, at),
        }),
    );
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

#[test]
fn proof_goals_follow_the_cursor_through_drafts_and_steps() {
    let draft = "module App.Main exposing (..)\n\nproof back (x : Bool) (y : Bool) (h : equal x y) where equal y x = ";
    let proved = format!(
        "{draft}symm h\n\
proof both (x : Bool) (y : Bool) (z : Bool) (p : equal x y) (q : equal y z) where equal x z = trans y p q\n\
proof eta (f : a -> b) where equal (fun x -> f x) f = funext (fun y -> refl)\n\
proof rebuilt (v : Option a) where equal (match v with | None -> None | Some x -> Some x) v =\n  match v with\n  | None -> refl\n  | Some y -> refl\n"
    );
    let dir = project(
        "lsp_proof_cursor",
        &[("flick.toml", LIB), ("src/App/Main.fl", draft)],
    );
    let main = uri(&dir.join("src/App/Main.fl"));
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
        "uri": main, "languageId": "flick", "version": 1, "text": draft,
    } }),
    );
    let _ = lsp.diagnostics(&main);
    let open = goal_at(&mut lsp, &main, draft, draft.len());
    assert_eq!(open["version"], 1, "{open}");
    assert_eq!(open["proof"]["status"], "open", "{open}");
    assert_eq!(open["proof"]["goal"], "y = x", "{open}");
    assert_eq!(
        open["proof"]["premises"],
        json!([{ "name": "h", "statement": "x = y" }]),
        "{open}"
    );

    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": main, "version": 2 },
            "contentChanges": [{ "text": proved }],
        }),
    );
    assert!(lsp.diagnostics(&main).is_empty());
    let read = |lsp: &mut Lsp, needle: &str, inside: usize| {
        goal_at(
            lsp,
            &main,
            &proved,
            proved.find(needle).expect("proof step") + inside,
        )
    };
    let symmetry = read(&mut lsp, "symm h", 5);
    assert_eq!(symmetry["version"], 2, "{symmetry}");
    assert_eq!(symmetry["proof"]["status"], "checked", "{symmetry}");
    assert_eq!(symmetry["proof"]["goal"], "x = y", "{symmetry}");
    assert_eq!(read(&mut lsp, "trans y p q", 8)["proof"]["goal"], "x = y");
    assert_eq!(read(&mut lsp, "trans y p q", 10)["proof"]["goal"], "y = z");
    let eta = read(&mut lsp, "y -> refl", 5);
    let binds = eta["proof"]["binders"].as_array().expect("binders");
    assert!(
        binds.contains(&json!({ "name": "y", "type": "a" })),
        "{eta}"
    );
    for (arm, value) in [("| None -> refl", "None"), ("| Some y -> refl", "Some y")] {
        let at = proved.rfind(arm).expect("arm") + arm.len() - 3;
        let found = goal_at(&mut lsp, &main, &proved, at);
        assert_eq!(
            found["proof"]["substitutions"],
            json!([{ "name": "v", "value": value }]),
            "{found}"
        );
    }

    let broken = proved.replace("| Some y -> refl", "| Some y -> missing");
    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": main, "version": 3 },
            "contentChanges": [{ "text": broken }],
        }),
    );
    let _ = lsp.diagnostics(&main);
    let found = goal_at(
        &mut lsp,
        &main,
        &broken,
        broken.find("missing").expect("arm"),
    );
    assert_eq!(found["proof"]["status"], "open", "{found}");
    assert_eq!(
        found["proof"]["substitutions"],
        json!([{ "name": "v", "value": "Some y" }]),
        "{found}"
    );
    lsp.close();
}

#[test]
fn proof_cost_and_exhaustion_point_to_the_failing_step() {
    use std::fmt::Write as _;

    let mut text = String::from(
        "module App.Main exposing (..)\n\ndouble0 : (a -> a) -> a -> a\nlet double0 f x = f (f x)\n",
    );
    for i in 1..16 {
        let previous = i - 1;
        writeln!(text, "double{i} : (a -> a) -> a -> a").unwrap();
        writeln!(
            text,
            "let double{i} f x = double{previous} f (double{previous} f x)"
        )
        .unwrap();
    }
    text.push_str("\nproof costly where equal true (double15 (fun x -> x) true) = symm refl\n");
    let dir = project(
        "lsp_proof_cost",
        &[("flick.toml", LIB), ("proofs/App/Main.fl", &text)],
    );
    let main = uri(&dir.join("proofs/App/Main.fl"));
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
            "uri": main, "languageId": "flick", "version": 1, "text": text,
        } }),
    );
    let diagnostics = lsp.diagnostics(&main);
    assert_eq!(codes(&diagnostics), ["FLICK-T084"]);
    let step = text.rfind("refl").expect("inner proof step");
    let range = json!({ "start": position(&text, step), "end": position(&text, step + 4) });
    assert_eq!(diagnostics[0]["range"], range);
    let at_header = text.find("proof costly").expect("proof declaration");
    let header = goal_at(&mut lsp, &main, &text, at_header)["proof"].clone();
    let inner = goal_at(&mut lsp, &main, &text, step)["proof"].clone();
    assert_eq!(header["status"], "budget-exhausted", "{header}");
    assert_eq!(header["failureRange"], range);
    assert_eq!(header["verificationSteps"], 200_000);
    assert_eq!(header["normalizationSteps"], Value::Null);
    assert_eq!(inner["verificationSteps"], header["verificationSteps"]);
    lsp.close();
}

#[test]
fn proof_goal_revisions_cover_dependencies_and_closed_documents() {
    let source =
        "module App.Steps exposing (..)\ntwice : (a -> a) -> a -> a\nlet twice f x = f (f x)\n";
    let text = concat!(
        "module App.Laws exposing (..)\n",
        "import App.Steps exposing (twice)\n",
        "proof chosen (f : a -> a) (x : a) where equal (twice f x) (f (f x)) = refl\n",
        "proof draft where equal /* 🥝 */ true true =\n",
    );
    let dir = project(
        "lsp_proof_revisions",
        &[
            ("flick.toml", LIB),
            ("src/App/Steps.fl", source),
            ("proofs/App/Laws.fl", text),
        ],
    );
    let laws = uri(&dir.join("proofs/App/Laws.fl"));
    let dep = uri(&dir.join("src/App/Steps.fl"));
    let mut lsp = Lsp::start(&dir);
    let open = |lsp: &mut Lsp, uri: &str, text: &str| {
        lsp.notify(
            "textDocument/didOpen",
            &json!({ "textDocument": {
            "uri": uri, "languageId": "flick", "version": 1, "text": text,
        } }),
        );
    };
    let proved = text.replace("true true =\n", "true true = refl\n");
    open(&mut lsp, &laws, &proved);
    let diagnostics = lsp.diagnostics(&laws);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let at = proved.find("refl").expect("certificate");
    let first = goal_at(&mut lsp, &laws, &proved, at);
    assert_eq!(first["proof"]["status"], "checked", "{first}");

    open(&mut lsp, &dep, &source.replace("f (f x)", "f x"));
    let after_edit = goal_at(&mut lsp, &laws, &proved, at);
    assert!(after_edit["generation"].as_u64() > first["generation"].as_u64());
    let changed = if after_edit["pending"] == true {
        assert_eq!(after_edit["proof"], Value::Null);
        assert_eq!(after_edit["snapshotGeneration"], Value::Null);
        let _ = lsp.diagnostics(&laws);
        goal_at(&mut lsp, &laws, &proved, at)
    } else {
        after_edit
    };
    assert_eq!(changed["proof"]["status"], "open", "{changed}");
    assert!(changed["snapshotGeneration"].as_u64() > first["snapshotGeneration"].as_u64());

    lsp.notify(
        "textDocument/didChange",
        &json!({ "textDocument": { "uri": laws, "version": 2 },
        "contentChanges": [{ "text": text }] }),
    );
    let _ = lsp.diagnostics(&laws);
    let at = text.rfind('=').expect("draft") + 1;
    let utf16 = goal_at(&mut lsp, &laws, text, at);
    assert_eq!(utf16["position"], position(text, at));
    assert_eq!(utf16["proof"]["name"], "draft", "{utf16}");
    assert_eq!(utf16["proof"]["goal"], "true = true", "{utf16}");
    assert_eq!(utf16["proof"]["lemmas"], json!([]), "{utf16}");
    lsp.notify(
        "textDocument/didChange",
        &json!({ "textDocument": { "uri": laws, "version": 3 },
        "contentChanges": [{ "text": proved }] }),
    );
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": laws } }),
    );
    let closed = goal_at(&mut lsp, &laws, &proved, at);
    assert_eq!(closed["version"], Value::Null, "{closed}");
    assert_eq!(closed["proof"], Value::Null, "{closed}");
    lsp.close();
}

#[test]
fn proof_goals_include_missing_laws_of_the_resolved_instance() {
    let text = "module App.Main exposing (..)\nproof opt where lawful Monad Option = {}\n";
    let dir = project(
        "lsp_proof_bundle_goals",
        &[("flick.toml", LIB), ("proofs/App/Main.fl", text)],
    );
    let main = uri(&dir.join("proofs/App/Main.fl"));
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
        "uri": main, "languageId": "flick", "version": 1, "text": text,
    } }),
    );
    let _ = lsp.diagnostics(&main);
    let view = goal_at(&mut lsp, &main, text, text.find("{}").expect("bundle"));
    let proof = &view["proof"];
    assert_eq!(proof["status"], "open", "{view}");
    assert!(
        proof["instance"]
            .as_str()
            .is_some_and(|instance| instance.contains("Monad Option")),
        "{view}"
    );
    let obligations = proof["obligations"].as_array().expect("obligations");
    assert!(
        obligations.iter().any(|o| {
            o["path"] == json!(["Applicative", "Functor"])
                && o["law"] == "identity"
                && o["status"] == "open"
                && o["statement"].is_string()
        }),
        "{view}"
    );
    lsp.close();
}
