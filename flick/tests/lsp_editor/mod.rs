mod aliases;
mod cli;
mod completion;
mod features;
mod ffi;
mod foreign;
mod manifests;
mod quick_fix;
mod sql_table;
mod workspace;

use super::*;

#[test]
fn builtin_facade_definition_opens_real_source() {
    let fixed = MAIN.replace("(count + \"x\")", "count");
    let dir = project(
        "lsp_builtin_facade",
        &[
            ("flick.toml", LIB),
            ("src/App/Domain.fl", DOMAIN),
            ("src/App/Main.fl", &fixed),
        ],
    );
    let main = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&main), "languageId": "flick", "version": 1, "text": fixed } }));
    assert!(lsp.diagnostics(&uri(&main)).is_empty());
    let builtin = lsp.request(
        "textDocument/definition",
        &json!({
            "textDocument": { "uri": uri(&main) }, "position": at(&fixed, "length"),
        }),
    );
    let path = builtin["result"]["uri"]
        .as_str()
        .and_then(|u| u.strip_prefix("file://"))
        .expect("real source");
    let content = fs::read_to_string(path).expect("source");
    assert!(fs::metadata(path).expect("source").permissions().readonly());
    let line = usize::try_from(
        builtin["result"]["range"]["start"]["line"]
            .as_u64()
            .expect("line"),
    )
    .expect("small");
    assert!(
        content
            .lines()
            .nth(line)
            .expect("line")
            .starts_with("length")
    );
    lsp.close();
}

#[test]
fn builtin_prelude_opens_without_lang_diagnostics() {
    let source = "module App.Main exposing (greet)\n\ngreet : String\nlet greet = \"hi\"\n";
    let dir = project(
        "lsp_builtin_prelude",
        &[("flick.toml", LIB), ("src/App/Main.fl", source)],
    );
    let main = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&main), "languageId": "flick", "version": 1, "text": source } }));
    assert!(lsp.diagnostics(&uri(&main)).is_empty());

    let definition = lsp.request(
        "textDocument/definition",
        &json!({ "textDocument": { "uri": uri(&main) }, "position": at(source, "String") }),
    );
    let builtin_uri = definition["result"]["uri"].as_str().expect("source URI");
    assert!(builtin_uri.ends_with("/Prelude.fl"), "{definition}");
    let path = builtin_uri.strip_prefix("file://").expect("file URI");
    let builtin_text = fs::read_to_string(path).expect("built-in source");
    assert!(builtin_text.contains("@lang prelude"));
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": builtin_uri, "languageId": "flick", "version": 1, "text": builtin_text } }));
    assert_eq!(lsp.diagnostics(builtin_uri), Vec::<Value>::new());
    lsp.close();
}

#[test]
fn sql_completion_and_hover() {
    let source = include_str!("../../../tests/check/pass/sql/balances.fl");
    let dir = project(
        "lsp_sql",
        &[("flick.toml", LIB), ("src/App/Balances.fl", source)],
    );
    let file = dir.join("src/App/Balances.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    for (needle, offset, expected) in [
        ("account.balance", 9, "account.balance : Option BigDecimal"),
        ("user.name", 6, "user.name : String"),
        ("params.user_id", 8, "params.user_id : UserId"),
        ("user_id : bigint not null", 1, "user_id : UserId"),
        ("select {", 1, "The result row"),
    ] {
        let hover = lsp.request(
            "textDocument/hover",
            &json!({
                "textDocument": doc,
                "position": position(source, source.find(needle).expect("occurs") + offset),
            }),
        );
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains(expected)),
            "{needle}: {hover}"
        );
    }
    for (old, replacement, wanted, detail) in [
        (
            "balance = account.balance",
            "balance = account.",
            "balance",
            Some("Option BigDecimal"),
        ),
        ("params.user_id", "params.", "user_id", Some("UserId")),
        ("from user in Users", "from user in ", "Accounts", None),
        ("name = user.name", "name = ", "user", None),
        ("select", "sel", "select", None),
        (
            "id : bigint primary",
            "id : big primary",
            "bigint",
            Some("BigInt"),
        ),
    ] {
        let text = source.replacen(old, replacement, 1);
        lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 2 }, "contentChanges": [{ "text": text }] }));
        let completion = lsp.request("textDocument/completion", &json!({
            "textDocument": doc,
            "position": position(&text, text.find(replacement).expect("occurs") + replacement.len()),
        }));
        let item = completion["result"]
            .as_array()
            .expect("items")
            .iter()
            .find(|i| i["label"] == wanted)
            .unwrap_or_else(|| panic!("{replacement}: {completion}"));
        if let Some(detail) = detail {
            assert_eq!(item["detail"], detail, "{replacement}");
        }
        assert!(
            !completion["result"]
                .as_array()
                .expect("items")
                .iter()
                .any(|i| i["label"] == "load_balances"),
            "{replacement}: SQL does not offer Flick functions"
        );
    }
    lsp.close();
}

#[test]
fn builtin_module_and_member_navigation() {
    let source = include_str!("../../../tests/check/pass/sql/balances.fl");
    let dir = project(
        "lsp_builtin_navigation",
        &[("flick.toml", LIB), ("src/App/Balances.fl", source)],
    );
    let file = dir.join("src/App/Balances.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    let mut definitions = Vec::new();
    for (offset, expected) in [(1, "Typed SQL for PostgreSQL"), (5, "Binds the parameters")] {
        let point = position(source, source.find("Sql.bind").expect("call") + offset);
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": doc, "position": point }),
        );
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains(expected)),
            "{hover}"
        );
        let definition = lsp.request(
            "textDocument/definition",
            &json!({ "textDocument": doc, "position": point }),
        );
        let path = definition["result"]["uri"]
            .as_str()
            .and_then(|u| u.strip_prefix("file://"))
            .expect("real file");
        let text = fs::read_to_string(path).expect("readable module");
        let line = usize::try_from(
            definition["result"]["range"]["start"]["line"]
                .as_u64()
                .expect("line"),
        )
        .expect("small");
        assert!(
            text.lines()
                .nth(line)
                .expect("definition line")
                .starts_with(if offset == 1 { "module Sql " } else { "bind :" })
        );
        definitions.push(definition["result"].clone());
    }
    assert_eq!(definitions[0]["uri"], definitions[1]["uri"]);
    assert_ne!(definitions[0]["range"], definitions[1]["range"]);
    let builtin_uri = definitions[0]["uri"].as_str().expect("source URI");
    let builtin_text =
        fs::read_to_string(builtin_uri.strip_prefix("file://").expect("file")).expect("source");
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": builtin_uri, "languageId": "flick", "version": 1, "text": builtin_text } }));
    assert!(
        lsp.diagnostics(builtin_uri).is_empty(),
        "a built-in must not import its own instances"
    );
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": { "uri": builtin_uri }, "position": at(&builtin_text, "bind :") }),
    );
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|s| s.contains("Binds the parameters")),
        "{hover}"
    );
    for method in ["textDocument/prepareRename", "textDocument/rename"] {
        let refused = lsp.request(
            method,
            &json!({
                "textDocument": { "uri": builtin_uri },
                "position": at(&builtin_text, "bind :"),
                "newName": "renamed_bind",
            }),
        );
        assert_eq!(refused["error"]["code"], -32803, "{refused}");
        assert!(refused.get("result").is_none(), "{refused}");
    }
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": doc, "position": at(source, "account.balance") }),
    );
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|s| s.contains("account : {")),
        "{hover}"
    );
    lsp.close();
}

#[test]
fn hover_covers_private_functions_and_ignore() {
    let source = r#"module App.Main exposing (main)
/// Reports a background failure.
report : Failure e ->{} Unit
let report failure = Platform.stderr_line (RawCause.describe (Failure.inspect failure))
view : Atom.Writable Float -> Float -> Ui
let view model m = Ui.text (show m)
let seed = 0.0
main : Unit ->{; AppError} Unit
let main () =
  match Ui.Dom.container "playground" with
  | None -> App.fail "missing root"
  | Some root ->
    let model = Atom.make seed
    provide Atom.layer {
      ignore (Ui.Dom.mount root report (Ui.render (Ui.view (fun () -> view model (Atom.value model))) ()))
    }
"#;
    let dir = project(
        "lsp_private_hover",
        &[("flick.toml", LIB), ("src/App/Main.fl", source)],
    );
    let file = dir.join("src/App/Main.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    // only unused-code warnings, which carry the `Unnecessary` tag
    let diagnostics = lsp.diagnostics(&uri(&file));
    assert!(
        diagnostics.iter().all(|d| d["tags"] == json!([1])),
        "{diagnostics:?}"
    );
    for (needle, expected) in [
        ("report (Ui", "report : Failure e ->{} Unit"),
        (
            "view model (Atom",
            "view : Atom.Writable Float -> Float -> Ui",
        ),
        ("seed\n", "seed : Float"),
        ("seed =", "seed : Float"),
        ("ignore (", "ignore : Never -> Unit"),
    ] {
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": doc, "position": at(source, needle) }),
        );
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains(expected)),
            "{needle}: {hover}"
        );
        let start = source.find(needle).expect("occurs");
        let name = needle.split_whitespace().next().expect("name");
        assert_eq!(
            hover["result"]["range"],
            json!({ "start": position(source, start), "end": position(source, start + name.len()) })
        );
    }
    let report = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": doc, "position": at(source, "report (Ui") }),
    );
    assert!(
        report["result"]["contents"]["value"]
            .as_str()
            .is_some_and(|s| s.contains("Reports a background failure.")),
        "{report}"
    );
    for name in ["report (Ui", "view model (Atom"] {
        let definition = lsp.request(
            "textDocument/definition",
            &json!({ "textDocument": doc, "position": at(source, name) }),
        );
        assert_eq!(definition["result"]["uri"], uri(&file));
        let target = name.split_whitespace().next().expect("name");
        assert_eq!(
            definition["result"]["range"]["start"],
            position(
                source,
                source.find(&format!("{target} :")).expect("signature")
            )
        );
    }
    lsp.close();
}

#[test]
fn hover_distinguishes_errors_services_types_and_constructors() {
    let source = "module App.Main exposing (..)\n\
        describe : File.OpenError ->{File.FileSystem; none} String\n\
        let describe err = show err\n\
        make : String -> File.OpenError\n\
        let make reason = File.OpenError reason\n";
    let dir = project(
        "lsp_declaration_kinds",
        &[("flick.toml", LIB), ("src/App/Main.fl", source)],
    );
    let file = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    for (needle, expected) in [
        ("OpenError", "type error OpenError"),
        ("FileSystem", "service FileSystem"),
        ("String", "type String : Type"),
        ("OpenError reason", "OpenError : String -> OpenError"),
    ] {
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": { "uri": uri(&file) }, "position": at(source, needle) }),
        );
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains(expected)),
            "{hover}"
        );
    }
    lsp.close();
}
