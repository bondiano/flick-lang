use super::*;
use crate::common::flick_in;

fn completion(lsp: &mut Lsp, uri: &str, text: &str, offset: usize) -> Vec<Value> {
    let response = lsp.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": uri }, "position": position(text, offset),
        }),
    );
    response["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .clone()
}

fn replace(lsp: &mut Lsp, uri: &str, version: u32, text: &str) {
    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri, "version": version },
            "contentChanges": [{ "text": text }],
        }),
    );
}

fn open(lsp: &mut Lsp, uri: &str, text: &str) {
    lsp.notify(
        "textDocument/didOpen",
        &json!({
            "textDocument": { "uri": uri, "languageId": "flick", "version": 1, "text": text },
        }),
    );
}

#[test]
fn completion_inserts_checked_proofs_and_tracks_scope() {
    let draft = "module App.Main exposing (..)\n\
proof bad where equal true false = refl\n\
proof back (x : Bool) (y : Bool) (h : equal x y) where equal y x = ";
    let dir = project(
        "lsp_proof_completion",
        &[("flick.toml", LIB), ("proofs/App/Main.fl", draft)],
    );
    let main = uri(&dir.join("proofs/App/Main.fl"));
    let mut lsp = Lsp::start(&dir);
    open(&mut lsp, &main, draft);
    let items = completion(&mut lsp, &main, draft, draft.len());
    let suggestion = items
        .iter()
        .find(|item| item["label"] == "symm h")
        .unwrap_or_else(|| panic!("{items:?}"));
    assert_eq!(suggestion["detail"], "Closes the goal");
    assert!(!items.iter().any(|item| item["label"] == "bad"
        || item["label"] == "refl" && item["detail"] == "Closes the goal"));
    assert!(
        items
            .iter()
            .any(|item| item["label"] == "funext" && item["detail"] == "Leaves proof obligations")
    );
    let inserted = suggestion["insertText"]
        .as_str()
        .or_else(|| suggestion["label"].as_str())
        .expect("insertable text");
    let fixed = format!("{draft}{inserted}\n");
    replace(&mut lsp, &main, 2, &fixed);
    let view = lsp.request("textDocument/hover", &json!({
        "textDocument": { "uri": main }, "position": position(&fixed, fixed.find("proof back").expect("proof")),
    }));
    assert!(view["error"].is_null(), "{view}");
    let only_back = fixed.replace("proof bad where equal true false = refl\n", "");
    fs::write(dir.join("proofs/App/Main.fl"), &only_back).expect("save inserted proof");
    let proved = flick_in(&dir, &["prove"]);
    assert!(
        proved.status.success(),
        "{}",
        String::from_utf8_lossy(&proved.stderr)
    );

    let false_goal = "module App.Main exposing (..)\nproof never where equal true false = ";
    replace(&mut lsp, &main, 3, false_goal);
    let items = completion(&mut lsp, &main, false_goal, false_goal.len());
    assert!(
        !items
            .iter()
            .any(|item| item["label"] == "refl" && item["detail"] == "Closes the goal")
    );
    lsp.close();
}

#[test]
fn lawful_completion_and_scaffolding_preserve_written_fields() {
    const OPTION_LAWS: &str = include_str!("../../../../packages/core/proofs/Laws/Option.fl");
    let (lemmas, _) = OPTION_LAWS
        .rsplit_once("proof option\n")
        .expect("Option bundle");
    let draft = format!(
        "{lemmas}proof option\nwhere lawful Monad Option = {{ Applicative = {{ Functor = {{ identity = map_identity }} }} }}\n"
    );
    let dir = project(
        "lsp_law_completion",
        &[("flick.toml", LIB), ("proofs/Laws/Option.fl", &draft)],
    );
    let main = uri(&dir.join("proofs/Laws/Option.fl"));
    let mut lsp = Lsp::start(&dir);
    open(&mut lsp, &main, &draft);
    let field = draft.rfind("identity =").expect("Functor law");
    let value = draft.rfind("map_identity").expect("Functor lemma");
    let items = completion(&mut lsp, &main, &draft, value);
    assert!(
        items
            .iter()
            .any(|item| item["label"] == "map_identity" && item["detail"] == "Proves this law"),
        "{items:?}"
    );
    let items = completion(&mut lsp, &main, &draft, value + "map_identity ".len());
    assert!(
        items.iter().any(|item| item["label"] == "composition"),
        "{items:?}"
    );
    let point = position(&draft, field);
    let actions = lsp.request(
        "textDocument/codeAction",
        &json!({
            "textDocument": { "uri": main }, "range": {"start": point, "end": point},
            "context": {"diagnostics": []},
        }),
    );
    let edit = actions["result"]
        .as_array()
        .unwrap_or_else(|| panic!("{actions}"))
        .iter()
        .find(|action| action["title"] == "Add missing law fields")
        .unwrap_or_else(|| panic!("{actions}"));
    let edits = edit["edit"]["changes"][&main]
        .as_array()
        .expect("law edits");
    let mut insertions: Vec<_> = edits
        .iter()
        .map(|edit| {
            let range = &edit["range"];
            assert_eq!(range["start"], range["end"], "{edit}");
            let line = usize::try_from(range["start"]["line"].as_u64().expect("line"))
                .expect("small line");
            let col = usize::try_from(range["start"]["character"].as_u64().expect("column"))
                .expect("small column");
            let offset = draft
                .split_inclusive('\n')
                .take(line)
                .map(str::len)
                .sum::<usize>()
                + col;
            (offset, edit["newText"].as_str().expect("scaffold"))
        })
        .collect();
    insertions.sort_unstable_by_key(|(at, _)| *at);
    let mut filled = draft.clone();
    for (at, addition) in insertions.into_iter().rev() {
        filled.insert_str(at, addition);
    }
    assert!(filled.contains("identity = map_identity"), "{filled}");
    assert!(filled.contains("composition = _"), "{filled}");
    assert!(filled.contains("left_identity = _"), "{filled}");
    replace(&mut lsp, &main, 2, &filled);
    lsp.close();
}

#[test]
fn aliased_proof_completion_rechecks_imported_implementation_bodies() {
    let steps = "module App.Steps exposing (..)\n\
twice : (a -> a) -> a -> a\nlet twice f x = f (f x)\n\
other : (a -> a) -> a -> a\nlet other f x = f (f x)\n";
    let facts = "module App.Facts exposing (..)\nimport App.Steps exposing (twice, other)\n\
proof unfolding (f : a -> a) (x : a) where equal (twice f x) (f (f x)) = refl\n\
proof wrong_subject (f : a -> a) (x : a) where equal (other f x) (f (f x)) = refl\n";
    let text = "module App.Main exposing (..)\nimport App.Facts as F\nimport App.Steps exposing (twice)\n\
proof draft (f : a -> a) (x : a) where equal (twice f x) (f (f x)) = \n\
proof later (x : Bool) where equal x x = refl\n";
    let dir = project(
        "lsp_proof_completion_alias",
        &[
            ("flick.toml", LIB),
            ("src/App/Steps.fl", steps),
            ("proofs/App/Facts.fl", facts),
            ("proofs/App/Main.fl", text),
        ],
    );
    let main = uri(&dir.join("proofs/App/Main.fl"));
    let dep = uri(&dir.join("src/App/Steps.fl"));
    let mut lsp = Lsp::start(&dir);
    open(&mut lsp, &main, text);
    let at = text.find("= \n").expect("empty proof") + 2;
    let items = completion(&mut lsp, &main, text, at);
    assert!(
        items
            .iter()
            .any(|i| i["label"] == "F.unfolding f x" && i["detail"] == "Closes the goal"),
        "{items:?}"
    );
    assert!(
        !items.iter().any(|i| {
            i["label"] == "later"
                || i["label"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("F.wrong_subject "))
                    && i["detail"] == "Closes the goal"
        }),
        "{items:?}"
    );
    let broken = steps.replacen("f (f x)", "f x", 1);
    open(&mut lsp, &dep, &broken);
    let items = completion(&mut lsp, &main, text, at);
    assert!(
        !items.iter().any(|i| i["label"]
            .as_str()
            .is_some_and(|name| name.starts_with("F.unfolding"))),
        "{items:?}"
    );
    lsp.close();
}
