use super::*;

const SOURCE: &str = "module App.Main exposing (..)

refined type Minutes = Number
  where fun n -> n >= 0
  deriving Cli.Value

let fallback = 3
let child = cli \"child\" { handle fun {} -> () }
group : Cli.Command none none
let group = cli \"tool\" { subcommand child }
let command =
  cli \"plan\" {
    description \"Планирование задач\"
    /// Extra whole minutes charged for every task.
    /// Uses **zero** when absent.
    flag overhead \"o\" : Option Minutes
    /// Task names from the catalog; repeated tasks count again.
    arg tasks : List String
    handle fun { overhead; tasks } ->
      let description = List.length tasks
      ignore (overhead, description)
  }
";

#[test]
fn cli_hover_explains_clauses_and_parameter_types_with_docs() {
    let dir = project(
        "lsp_cli_hover",
        &[("flick.toml", LIB), ("src/App/Main.fl", SOURCE)],
    );
    let file = dir.join("src/App/Main.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": SOURCE } }));
    let diagnostics = lsp.diagnostics(&uri(&file));
    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    for (needle, name, ty, docs) in [
        (
            "overhead \"o\"",
            "overhead",
            "Option Minutes",
            "Extra whole minutes charged for every task.\nUses **zero** when absent.",
        ),
        (
            "tasks :",
            "tasks",
            "List String",
            "Task names from the catalog; repeated tasks count again.",
        ),
    ] {
        let start = SOURCE.find(needle).expect("parameter");
        let reply = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": doc, "position": position(SOURCE, start + 1) }),
        );
        let markdown = reply["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("{reply}"));
        assert!(markdown.contains(&format!("{name} : {ty}")), "{reply}");
        assert!(markdown.contains(docs), "{reply}");
        assert_eq!(
            reply["result"]["range"],
            json!({ "start": position(SOURCE, start), "end": position(SOURCE, start + name.len()) })
        );
    }
    for (needle, syntax) in [
        ("description \"", "description"),
        ("flag overhead", "flag name"),
        ("arg tasks", "arg name"),
        ("subcommand child", "subcommand"),
        ("handle fun { overhead", "handle"),
    ] {
        let reply = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": doc, "position": at(SOURCE, needle) }),
        );
        let markdown = reply["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("{reply}"));
        assert!(markdown.contains(syntax), "{reply}");
        assert!(!markdown.contains("Record.Label"), "{reply}");
    }
    lsp.close();
}

#[test]
fn cli_completion_recovers_partial_clauses_without_hiding_flick_expressions() {
    let dir = project(
        "lsp_cli_completion",
        &[("flick.toml", LIB), ("src/App/Main.fl", SOURCE)],
    );
    let file = dir.join("src/App/Main.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": SOURCE } }));
    let cases = [
        ("", "flag", 14),
        ("fl", "flag", 14),
        ("ar", "arg", 14),
        ("desc", "description", 14),
        ("sub", "subcommand", 14),
        ("han", "handle", 14),
        ("flag cost : Num", "Number", 7),
        ("flag cost : ", "Number", 7),
        ("flag cost : Number = ", "fallback", 3),
        ("description ", "fallback", 3),
        ("flag cost : Number = fall", "fallback", 3),
        ("subcommand chi", "child", 3),
        ("handle fun opts -> List.len", "length", 3),
        ("flag cost : Number; fl", "flag", 14),
        ("subcommand cli \"nested\" { fl", "flag", 14),
        ("handle fun opts -> cli \"nested\" { fl", "flag", 14),
    ];
    for (version, (clause, wanted, kind)) in cases.into_iter().enumerate() {
        let prefix = format!("{SOURCE}\nlet editing = cli \"editing\" {{\n  ");
        let cursor = prefix.len() + clause.len();
        let text = format!("{prefix}{clause}\n}}\n");
        lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": version + 2 }, "contentChanges": [{ "text": text }] }));
        let reply = lsp.request(
            "textDocument/completion",
            &json!({ "textDocument": doc, "position": position(&text, cursor) }),
        );
        let items = reply["result"]
            .as_array()
            .unwrap_or_else(|| panic!("{reply}"));
        let item = items
            .iter()
            .find(|item| item["label"] == wanted)
            .unwrap_or_else(|| panic!("{clause:?}: missing {wanted}: {reply}"));
        assert_eq!(item["kind"], kind, "{clause:?}: {reply}");
        if kind == 14 {
            assert!(
                item["documentation"]["value"]
                    .as_str()
                    .is_some_and(|doc| doc.contains(wanted)),
                "{reply}"
            );
            assert!(
                item["insertText"]
                    .as_str()
                    .is_some_and(|snippet| snippet.starts_with(wanted)),
                "{reply}"
            );
        } else {
            assert!(
                !items
                    .iter()
                    .any(|item| item["label"] == "flag" && item["kind"] == 14),
                "expression position: {reply}"
            );
        }
    }
    lsp.close();
}

#[test]
fn cli_semantic_tokens_distinguish_clauses_parameters_and_handler_locals() {
    let dir = project(
        "lsp_cli_tokens",
        &[("flick.toml", LIB), ("src/App/Main.fl", SOURCE)],
    );
    let file = dir.join("src/App/Main.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": SOURCE } }));
    let init = lsp.request("initialize", &json!({}));
    let legend = init["result"]["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"]
        .as_array()
        .expect("legend");
    let reply = lsp.request(
        "textDocument/semanticTokens/full",
        &json!({ "textDocument": doc }),
    );
    let data = reply["result"]["data"].as_array().expect("tokens");
    let mut decoded = Vec::new();
    let (mut line, mut column) = (0, 0);
    for token in data.chunks_exact(5) {
        let delta = token[0].as_u64().expect("line delta");
        line += delta;
        column = if delta == 0 { column } else { 0 } + token[1].as_u64().expect("column delta");
        let kind = usize::try_from(token[3].as_u64().expect("kind")).expect("kind fits");
        decoded.push((
            line,
            column,
            token[2].as_u64().expect("length"),
            legend[kind].as_str().expect("kind name"),
        ));
    }
    for (needle, name, expected) in [
        ("description \"", "description", "keyword"),
        ("flag overhead", "flag", "keyword"),
        ("arg tasks", "arg", "keyword"),
        ("handle fun { overhead", "handle", "keyword"),
        ("overhead \"o\"", "overhead", "parameter"),
        ("tasks :", "tasks", "parameter"),
        ("Minutes\n    ///", "Minutes", "type"),
        ("description)", "description", "variable"),
    ] {
        let point = position(SOURCE, SOURCE.find(needle).expect("occurs"));
        assert!(
            decoded.contains(&(
                point["line"].as_u64().expect("line"),
                point["character"].as_u64().expect("column"),
                name.len() as u64,
                expected
            )),
            "{needle}: {decoded:?}"
        );
    }
    lsp.close();
}
