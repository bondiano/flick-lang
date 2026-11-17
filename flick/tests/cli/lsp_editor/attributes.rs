use super::*;

fn assert_hover(lsp: &mut Lsp, file: &Path, source: &str, word: &str, expected: &str) {
    let start = source.find(word).expect("hover target");
    assert_hover_at(lsp, file, source, start, word, expected);
}

fn assert_hover_at(
    lsp: &mut Lsp,
    file: &Path,
    source: &str,
    start: usize,
    word: &str,
    expected: &str,
) {
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": { "uri": uri(file) }, "position": position(source, start) }),
    );
    assert_eq!(hover["result"]["contents"]["kind"], "markdown", "{hover}");
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|s| s.contains(expected)),
        "{word}: {hover}"
    );
    assert_eq!(
        hover["result"]["range"],
        json!({
            "start": position(source, start), "end": position(source, start + word.len())
        }),
        "{hover}"
    );
}

#[test]
fn attribute_hover_follows_open_edit_and_close() {
    let saved = "module App.Main exposing (..)\n// ёж 🦔\n@maker\nlet make = 1\nuntagged type Choice = Previous = \"ёж 🦔\" | @lang none Empty = \"empty\"\n";
    let edited = "module App.Main exposing (..)\n// ёж 🦔🦔\n@derive std\nclass Custom a\n";
    let dir = project(
        "lsp_attribute_hover_edits",
        &[("flick.toml", LIB), ("src/App/Main.fl", saved)],
    );
    let file = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
        "uri": uri(&file), "languageId": "flick", "version": 1, "text": saved
    } }),
    );
    assert_hover(&mut lsp, &file, saved, "@maker", "atom, action or view");
    assert_hover(&mut lsp, &file, saved, "@lang", "built-in modules only");
    assert_hover(&mut lsp, &file, saved, "none", "Prelude.None");
    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri(&file), "version": 2 },
            "contentChanges": [{ "text": edited }]
        }),
    );
    assert_hover(&mut lsp, &file, edited, "@derive", "`std`");
    assert_hover(&mut lsp, &file, edited, "std", "deriving");
    lsp.notify(
        "textDocument/didClose",
        &json!({ "textDocument": { "uri": uri(&file) } }),
    );
    assert_hover(&mut lsp, &file, saved, "@maker", "atom, action or view");
    lsp.close();
}

#[test]
fn lang_attribute_hover_works_in_opened_standard_library_source() {
    let source = "module App.Main exposing (same)\nimport Action exposing (Action)\nsame : Action Unit -> Action Unit\nlet same value = value\n";
    let dir = project(
        "lsp_lang_attribute_hover",
        &[("flick.toml", LIB), ("src/App/Main.fl", source)],
    );
    let main = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
        "uri": uri(&main), "languageId": "flick", "version": 1, "text": source
    } }),
    );
    let definition = lsp.request(
        "textDocument/definition",
        &json!({
            "textDocument": { "uri": uri(&main) }, "position": at(source, "Action Unit")
        }),
    );
    let builtin_uri = definition["result"]["uri"]
        .as_str()
        .expect("builtin source URI");
    let file = Path::new(builtin_uri.strip_prefix("file://").expect("file URI"));
    let text = fs::read_to_string(file).expect("Action source");
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": {
        "uri": builtin_uri, "languageId": "flick", "version": 1, "text": text
    } }),
    );
    assert!(lsp.diagnostics(builtin_uri).is_empty());
    for (attribute, word, expected) in [
        ("@lang action\n", "@lang", "built-in modules only"),
        ("@lang action\n", "action", "Action.Action"),
        (
            "@lang action_queue",
            "action_queue",
            "`capacity` of them waiting",
        ),
        (
            "@lang action_concurrent",
            "action_concurrent",
            "`limit` tasks at once",
        ),
        ("@maker", "@maker", "atom, action or view"),
    ] {
        let start = text.find(attribute).expect("attribute") + attribute.find(word).expect("word");
        assert_hover_at(&mut lsp, file, &text, start, word, expected);
    }
    lsp.close();
}
