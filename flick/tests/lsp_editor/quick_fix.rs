use super::*;

const UNSIGNED: &str = "module App.Unsigned exposing (..)

service Log =
  write : String ->{} Unit

let double n = n * 2

let say msg = Log.write msg

let pair a b = (a, b)

let describe x = show x

main : Unit -> Float
let main () =
  let inc x = x + 1
  inc 1
";

/// The byte offset of an LSP position in ASCII text.
fn offset(text: &str, position: &Value) -> usize {
    let line = usize::try_from(position["line"].as_u64().expect("line")).expect("small");
    let character =
        usize::try_from(position["character"].as_u64().expect("character")).expect("small");
    text.split_inclusive('\n')
        .take(line)
        .map(str::len)
        .sum::<usize>()
        + character
}

#[test]
fn signature_quick_fix_makes_the_module_check() {
    let dir = project(
        "lsp_signature_fix",
        &[("flick.toml", LIB), ("src/App/Unsigned.fl", UNSIGNED)],
    );
    let file = dir.join("src/App/Unsigned.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": UNSIGNED } }));
    let missing: Vec<Value> = lsp
        .diagnostics(&uri(&file))
        .into_iter()
        .filter(|d| d["code"] == "FLICK-T004")
        .collect();
    assert_eq!(missing.len(), 5, "{missing:?}");
    let mut edits = Vec::new();
    for d in &missing {
        let actions = lsp.request(
            "textDocument/codeAction",
            &json!({ "textDocument": doc, "range": d["range"], "context": { "diagnostics": [d] } }),
        );
        let fix = actions["result"]
            .as_array()
            .expect("actions")
            .iter()
            .find(|a| a["diagnostics"][0]["code"] == "FLICK-T004")
            .unwrap_or_else(|| panic!("no quick fix for {d}"));
        assert_eq!(fix["kind"], "quickfix");
        let edit = fix["edit"]["changes"][uri(&file)][0].clone();
        assert_eq!(edit["range"]["start"]["character"], 0, "{edit}");
        edits.push(edit);
    }
    let mut fixed = UNSIGNED.to_owned();
    for edit in edits.iter().rev() {
        let at = offset(&fixed, &edit["range"]["start"]);
        fixed.insert_str(at, edit["newText"].as_str().expect("text"));
    }
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 2 }, "contentChanges": [{ "text": fixed }] }));
    lsp.notify(
        "textDocument/didSave",
        &json!({ "textDocument": { "uri": uri(&file) } }),
    );
    assert_eq!(lsp.diagnostics(&uri(&file)), Vec::<Value>::new(), "{fixed}");

    // a binding the checker rejects gets no signature to insert
    let broken = format!("{fixed}\nlet broken x = x + \"a\"\n");
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 3 }, "contentChanges": [{ "text": broken }] }));
    lsp.notify(
        "textDocument/didSave",
        &json!({ "textDocument": { "uri": uri(&file) } }),
    );
    let reported = lsp.diagnostics(&uri(&file));
    let t004 = reported
        .iter()
        .find(|d| d["code"] == "FLICK-T004")
        .expect("T004");
    let actions = lsp.request(
        "textDocument/codeAction",
        &json!({ "textDocument": doc, "range": t004["range"], "context": { "diagnostics": [t004] } }),
    );
    assert_eq!(actions["result"], json!([]), "{actions}");
    lsp.close();
}

#[test]
fn rec_quick_fix_and_diagnostics_wait_for_save() {
    let text = "module App.Fib exposing (..)\n\nfib : Float -> Float\nlet fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)\n";
    let dir = project(
        "lsp_rec_fix",
        &[("flick.toml", LIB), ("src/App/Fib.fl", text)],
    );
    let file = dir.join("src/App/Fib.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": text } }));
    let reported = lsp.diagnostics(&uri(&file));
    let t017 = reported
        .iter()
        .find(|d| d["code"] == "FLICK-T017")
        .unwrap_or_else(|| panic!("{reported:?}"));
    let actions = lsp.request(
        "textDocument/codeAction",
        &json!({ "textDocument": doc, "range": t017["range"], "context": { "diagnostics": [t017] } }),
    );
    let edit = &actions["result"][0]["edit"]["changes"][uri(&file)][0];
    let mut fixed = text.to_owned();
    fixed.insert_str(
        offset(text, &edit["range"]["start"]),
        edit["newText"].as_str().expect("text"),
    );
    assert!(fixed.contains("let rec fib n ="), "{fixed}");

    // an edit alone publishes nothing: the first report is the saved, fixed text
    let typing = text.replace("fib (n - 2)", "fib (n -");
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 2 }, "contentChanges": [{ "text": typing }] }));
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 3 }, "contentChanges": [{ "text": fixed }] }));
    lsp.notify(
        "textDocument/didSave",
        &json!({ "textDocument": { "uri": uri(&file) } }),
    );
    assert_eq!(lsp.diagnostics(&uri(&file)), Vec::<Value>::new());
    lsp.close();
}

#[test]
fn misspelt_name_quick_fix_replaces_it() {
    let text = "module App.Typo exposing (..)\n\nlet total = 1\n\nlet more () = totl + 1\n";
    let dir = project(
        "lsp_typo_fix",
        &[("flick.toml", LIB), ("src/App/Typo.fl", text)],
    );
    let file = dir.join("src/App/Typo.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": text } }));
    let reported = lsp.diagnostics(&uri(&file));
    let t001 = reported
        .iter()
        .find(|d| d["code"] == "FLICK-T001")
        .unwrap_or_else(|| panic!("{reported:?}"));
    let actions = lsp.request(
        "textDocument/codeAction",
        &json!({ "textDocument": { "uri": uri(&file) }, "range": t001["range"], "context": { "diagnostics": [t001] } }),
    );
    let fix = &actions["result"][0];
    assert_eq!(fix["title"], "did you mean `total`?", "{actions}");
    let edit = &fix["edit"]["changes"][uri(&file)][0];
    assert_eq!(edit["newText"], "total");
    assert_eq!(edit["range"], t001["range"]);
    lsp.close();
}

#[test]
fn unused_import_is_greyed_out_and_removable() {
    let text = "module App.Tidy exposing (one)\n\nimport String\n\none : Float\nlet one = 1\n";
    let dir = project(
        "lsp_unused_import",
        &[("flick.toml", LIB), ("src/App/Tidy.fl", text)],
    );
    let file = dir.join("src/App/Tidy.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": text } }));
    let reported = lsp.diagnostics(&uri(&file));
    let r012 = reported
        .iter()
        .find(|d| d["code"] == "FLICK-R012")
        .unwrap_or_else(|| panic!("{reported:?}"));
    assert_eq!(r012["severity"], 2);
    assert_eq!(r012["tags"], json!([1]));
    let actions = lsp.request(
        "textDocument/codeAction",
        &json!({ "textDocument": { "uri": uri(&file) }, "range": r012["range"], "context": { "diagnostics": [r012] } }),
    );
    let fix = &actions["result"][0];
    assert_eq!(fix["title"], "remove the import", "{actions}");
    let edit = &fix["edit"]["changes"][uri(&file)][0];
    let (start, end) = (
        offset(text, &edit["range"]["start"]),
        offset(text, &edit["range"]["end"]),
    );
    let fixed = format!("{}{}", &text[..start], &text[end..]);
    assert_eq!(
        fixed,
        "module App.Tidy exposing (one)\n\n\none : Float\nlet one = 1\n"
    );
    lsp.close();
}

#[test]
fn redundant_annotation_quick_fix_removes_it() {
    let text =
        "module App.Kinds exposing (..)\n\ntype Kind = A | B\n\nlet kinds : List Kind = [A, B]\n";
    let dir = project(
        "lsp_redundant_annotation",
        &[("flick.toml", LIB), ("src/App/Kinds.fl", text)],
    );
    let file = dir.join("src/App/Kinds.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": text } }));
    let reported = lsp.diagnostics(&uri(&file));
    let t021 = reported
        .iter()
        .find(|d| d["code"] == "FLICK-T021")
        .unwrap_or_else(|| panic!("{reported:?}"));
    let actions = lsp.request(
        "textDocument/codeAction",
        &json!({ "textDocument": { "uri": uri(&file) }, "range": t021["range"], "context": { "diagnostics": [t021] } }),
    );
    let fix = &actions["result"][0];
    assert_eq!(fix["title"], "remove the annotation", "{actions}");
    let edit = &fix["edit"]["changes"][uri(&file)][0];
    let (start, end) = (
        offset(text, &edit["range"]["start"]),
        offset(text, &edit["range"]["end"]),
    );
    let fixed = format!(
        "{}{}{}",
        &text[..start],
        edit["newText"].as_str().expect("text"),
        &text[end..]
    );
    assert!(fixed.contains("\nlet kinds = [A, B]\n"), "{fixed}");
    lsp.close();
}
