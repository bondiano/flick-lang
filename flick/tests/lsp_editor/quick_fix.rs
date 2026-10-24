use super::*;

const UNSIGNED: &str = "module App.Unsigned exposing (..)

service Log =
  write : String ->{} Unit

let double n = n * 2

let say msg = Log.write msg

let pair a b = (a, b)

let describe x = show x

main : Unit -> Number
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
    let inserted: Vec<&str> = edits.iter().filter_map(|e| e["newText"].as_str()).collect();
    assert_eq!(
        inserted,
        [
            "double : Number -> Number\n",
            "say : String ->{Log; a} Unit\n",
            "pair : a -> b -> (a, b)\n",
            "describe : Show a => a -> String\n",
            "  inc : Number -> Number\n",
        ]
    );
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
    let text = "module App.Fib exposing (..)\n\nfib : Number -> Number\nlet fib n = if n < 2 then n else fib (n - 1) + fib (n - 2)\n";
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
