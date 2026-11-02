use super::*;

/// The text at the start of the location a definition request answers.
fn definition_text(lsp: &mut Lsp, file: &Path, text: &str, offset: usize) -> Option<String> {
    let response = lsp.request(
        "textDocument/definition",
        &json!({ "textDocument": { "uri": uri(file) }, "position": position(text, offset) }),
    );
    let result = &response["result"];
    let path = result["uri"].as_str()?.strip_prefix("file://")?.to_owned();
    let line = usize::try_from(result["range"]["start"]["line"].as_u64()?).ok()?;
    let content = fs::read_to_string(&path).ok()?;
    Some(format!("{path}:{}", content.lines().nth(line)?.trim()))
}

#[test]
fn conduit_endpoint_names_go_to_their_definitions() {
    let mut failed = Vec::new();
    for (name, edit) in [
        ("clean", None),
        (
            "type_error",
            Some((
                "|> HttpApi.error_case 422 Problem.unprocessable)\n\n/// The errors of a request about",
                "|> HttpApi.error_case 422 Problem.unprocessabl)\n\n/// The errors of a request about",
            )),
        ),
        (
            "syntax_error",
            Some((
                "/// The errors of a request about",
                "let broken = HttpApi.\n\n/// The errors of a request about",
            )),
        ),
    ] {
        failed.extend(definitions_in(name, edit, ENDPOINT));
    }
    let qualified = (
        "|> HttpApi.with_success 200 (Codec.json (schema UserBody))\n  |> HttpApi.with_errors\n    (HttpApi.errors\n      |> HttpApi.error_case 401 Problem.unauthorized\n      |> HttpApi.error_case 409",
        "|> HttpApi.with_success 200 (Codec.json (schema Conduit.Model.UserBody))\n  |> HttpApi.with_errors\n    (HttpApi.errors\n      |> HttpApi.error_case 401 Problem.unauthorized\n      |> HttpApi.error_case 409",
    );
    let schema: &[(&str, usize, &str)] = &[
        ("schema Conduit", 0, "type alias UserBody"),
        ("schema Conduit", 7, "module Conduit.Model"),
        ("Model.UserBody", 0, "module Conduit.Model"),
        ("Model.UserBody", 6, "type alias UserBody"),
    ];
    failed.extend(definitions_in("qualified_schema", Some(qualified), schema));
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Names in the `update_user` endpoint, the offset of the cursor in each and
/// the start of the line their definition is on.
const ENDPOINT: &[(&str, usize, &str)] = &[
    ("at HttpApi.Put", 0, "at :"),
    ("at HttpApi.Put", 1, "at :"),
    ("HttpApi.Put", 0, "module HttpApi"),
    ("HttpApi.Put", 6, "module HttpApi"),
    ("HttpApi.Put", 7, "module HttpApi"),
    ("HttpApi.Put", 8, "type Method"),
    ("HttpApi.Put", 10, "type Method"),
    ("Record.label :", 0, "module Record"),
    ("Record.label :", 7, "label :"),
    ("Record.label :", 11, "label :"),
    ("Record.Label \"update_user\"", 7, "type Label"),
    ("HttpApi.with_payload", 8, "with_payload :"),
    ("HttpApi.with_payload", 19, "with_payload :"),
    ("HttpApi.with_payload", 20, "with_payload :"),
    ("Input.update_user_body", 0, "module Conduit.Input"),
    ("Input.update_user_body", 6, "update_user_body :"),
    ("Input.update_user_body", 21, "update_user_body :"),
    ("Codec.json", 6, "json :"),
    ("schema UserBody", 0, "type alias UserBody"),
    ("schema UserBody", 7, "type alias UserBody"),
    ("HttpApi.errors", 8, "errors :"),
    ("HttpApi.error_case 401", 8, "error_case :"),
    ("Problem.unauthorized", 0, "module Conduit.Problem"),
    ("Problem.unauthorized", 8, "unauthorized :"),
    ("Problem.unauthorized", 19, "unauthorized :"),
    ("Problem.conflict", 8, "conflict :"),
];

/// The names of the endpoint whose definition is not where it should be.
fn definitions_in(
    name: &str,
    edit: Option<(&str, &str)>,
    names: &[(&str, usize, &str)],
) -> Vec<String> {
    let dir = common::example("conduit", &format!("lsp_conduit_{name}"));
    let api = dir.join("contract/src/Conduit/Api.fl");
    let mut text = fs::read_to_string(&api).expect("Api.fl");
    if let Some((from, to)) = edit {
        assert!(text.contains(from));
        text = text.replacen(from, to, 1);
    }
    let block = text.find("let update_user =").expect("the endpoint");
    let mut lsp = Lsp::start(&dir.join("contract"));
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&api), "languageId": "flick", "version": 1, "text": text } }));
    let _ = lsp.diagnostics(&uri(&api));
    let mut failed = Vec::new();
    for &(needle, offset, expected) in names {
        let at = block + text[block..].find(needle).expect("in the block") + offset;
        let found = definition_text(&mut lsp, &api, &text, at);
        if !found.as_deref().is_some_and(|f| f.contains(expected)) {
            failed.push(format!("{name}: {needle} +{offset}: {found:?}"));
        }
    }
    lsp.close();
    failed
}

#[test]
fn server_names_go_to_the_contract_s_own_files() {
    let dir = common::example("conduit", "lsp_conduit_server");
    let routes = dir.join("server/src/Conduit/Server/Routes.fl");
    let text = fs::read_to_string(&routes).expect("Routes.fl");
    let mut lsp = Lsp::start(&dir.join("server"));
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&routes), "languageId": "flick", "version": 1, "text": text } }));
    assert!(lsp.diagnostics(&uri(&routes)).is_empty());
    let contract = dir.join("contract/src/Conduit/Api.fl");
    let mut failed = Vec::new();
    for (needle, offset, expected) in [
        (
            "Api.update_user)",
            0,
            format!("{}:module Conduit.Api", contract.display()),
        ),
        (
            "Api.update_user)",
            4,
            format!("{}:update_user :", contract.display()),
        ),
        (
            "HttpApi.with_auth token Session Auth.check Api.update_user",
            8,
            "with_auth :".to_owned(),
        ),
        ("Auth.check Api.update_user", 5, "check :".to_owned()),
        ("Accounts.update_user", 9, "update_user :".to_owned()),
    ] {
        let at = text.find(needle).expect("occurs") + offset;
        let found = definition_text(&mut lsp, &routes, &text, at);
        if !found.as_deref().is_some_and(|f| f.contains(&expected)) {
            failed.push(format!("{needle} +{offset}: {found:?}"));
        }
    }
    lsp.close();
    assert!(failed.is_empty(), "{failed:#?}");
}

#[test]
fn references_renames_and_symbols_reach_the_members_that_compile_a_library() {
    let dir = common::example("conduit", "lsp_conduit_members");
    let api = dir.join("contract/src/Conduit/Api.fl");
    let routes = dir.join("server/src/Conduit/Server/Routes.fl");
    let text = fs::read_to_string(&api).expect("Api.fl");
    let mut lsp = Lsp::start(&dir.join("contract"));
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&api), "languageId": "flick", "version": 1, "text": text } }));
    let _ = lsp.diagnostics(&uri(&api));
    let at = position(&text, text.find("let update_user").expect("declared") + 4);
    let refs = lsp.request(
        "textDocument/references",
        &json!({ "textDocument": { "uri": uri(&api) }, "position": at, "context": { "includeDeclaration": true } }),
    );
    let uris: Vec<&str> = refs["result"]
        .as_array()
        .expect("locations")
        .iter()
        .filter_map(|l| l["uri"].as_str())
        .collect();
    assert!(uris.contains(&uri(&routes).as_str()), "{uris:?}");
    assert!(uris.iter().all(|u| !u.contains("node_modules")), "{uris:?}");
    let renamed = lsp.request(
        "textDocument/rename",
        &json!({ "textDocument": { "uri": uri(&api) }, "position": at, "newName": "change_user" }),
    );
    let changes = &renamed["result"]["changes"];
    assert!(changes[uri(&api)].is_array(), "{renamed}");
    assert_eq!(
        changes[uri(&routes)].as_array().map(Vec::len),
        Some(1),
        "{renamed}"
    );
    let symbols = lsp.request("workspace/symbol", &json!({ "query": "settings" }));
    let main = dir.join("server/src/Conduit/Server/Main.fl");
    assert!(
        symbols["result"]
            .as_array()
            .expect("symbols")
            .iter()
            .any(|s| s["location"]["uri"] == uri(&main)),
        "{symbols}"
    );
    lsp.close();
}

#[test]
fn a_library_s_diagnostics_come_from_it_and_a_save_rechecks_its_members() {
    let dir = common::example("conduit", "lsp_conduit_publish");
    let api = dir.join("contract/src/Conduit/Api.fl");
    let routes = dir.join("server/src/Conduit/Server/Routes.fl");
    let open = |lsp: &mut Lsp, path: &Path, text: &str| {
        lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(path), "languageId": "flick", "version": 1, "text": text } }));
    };
    let mut lsp = Lsp::start(&dir.join("server"));
    open(
        &mut lsp,
        &routes,
        &fs::read_to_string(&routes).expect("Routes.fl"),
    );
    let _ = lsp.publications();
    let text = fs::read_to_string(&api).expect("Api.fl");
    let broken = text.replacen("Problem.conflict", "Problem.conflic", 1);
    open(&mut lsp, &api, &broken);
    let published = lsp.publications();
    let of_api: Vec<_> = published.iter().filter(|p| p["uri"] == uri(&api)).collect();
    // once, by the contract, though the server compiles the module too
    assert_eq!(of_api.len(), 1, "{published:#?}");
    assert!(
        !of_api[0]["diagnostics"]
            .as_array()
            .expect("list")
            .is_empty()
    );
    assert!(
        published.iter().any(|p| p["uri"] == uri(&routes)),
        "{published:#?}"
    );
    lsp.close();
}
