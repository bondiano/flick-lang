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
        ("type_error", Some(("TagsBody {}", "TagsBod {}"))),
        (
            "syntax_error",
            Some((
                "/// The articles, the feed",
                "let broken = HttpApi.\n\n/// The articles, the feed",
            )),
        ),
    ] {
        failed.extend(definitions_in(name, edit, ENDPOINT));
    }
    let qualified = (
        "/// The articles, the feed",
        "probe : Schema Conduit.Model.UserBody\nlet probe = schema Conduit.Model.UserBody\n\n/// The articles, the feed",
    );
    let schema: &[(&str, usize, &str)] = &[
        ("schema Conduit", 0, "type alias UserBody"),
        ("schema Conduit", 7, "module Conduit.Model"),
        ("Model.UserBody\n\n", 0, "module Conduit.Model"),
        ("Model.UserBody\n\n", 6, "type alias UserBody"),
    ];
    failed.extend(definitions_in("qualified_schema", Some(qualified), schema));
    assert!(failed.is_empty(), "{failed:#?}");
}

/// Names in the `update_user` endpoint, the offset of the cursor in each and
/// the start of the line their definition is on.
const ENDPOINT: &[(&str, usize, &str)] = &[
    ("Authed", 0, "type Authed"),
    ("Authed", 3, "type Authed"),
    ("Put", 0, "type Put"),
    ("Credentials;", 0, "type alias Credentials"),
    ("Credentials;", 11, "type alias Credentials"),
    ("UpdateUserBody", 6, "type alias UpdateUserBody"),
    ("\n      UserBody", 9, "type alias UserBody"),
    ("Unauthorized,", 0, "type error Unauthorized"),
    ("Conflict,", 3, "type error Conflict"),
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
    let block = text.find("update_user : Authed").expect("the endpoint");
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
            "Api.Users.api",
            0,
            format!("{}:module Conduit.Api", contract.display()),
        ),
        (
            "Api.Users.api",
            10,
            format!("{}:type alias Users", contract.display()),
        ),
        ("HttpApi.add_group (Api.Users", 8, "add_group :".to_owned()),
        ("Auth.check", 5, "check :".to_owned()),
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
    let accounts = dir.join("server/src/Conduit/Server/Accounts.fl");
    let text = fs::read_to_string(&api).expect("Api.fl");
    let mut lsp = Lsp::start(&dir.join("contract"));
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&api), "languageId": "flick", "version": 1, "text": text } }));
    let _ = lsp.diagnostics(&uri(&api));
    let at = position(&text, text.find("alias Request").expect("declared") + 6);
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
    assert!(uris.contains(&uri(&accounts).as_str()), "{uris:?}");
    assert!(uris.iter().all(|u| !u.contains("node_modules")), "{uris:?}");
    let renamed = lsp.request(
        "textDocument/rename",
        &json!({ "textDocument": { "uri": uri(&api) }, "position": at, "newName": "Call" }),
    );
    let changes = &renamed["result"]["changes"];
    assert!(changes[uri(&api)].is_array(), "{renamed}");
    assert!(
        changes[uri(&accounts)]
            .as_array()
            .is_some_and(|edits| !edits.is_empty()),
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
    let broken = text.replacen("TagsBody {}", "TagsBod {}", 1);
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
