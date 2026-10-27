use super::*;

const PLAN: &str = "module App.Plan exposing (plan, pair)

import App.Public exposing (Pair)

type Identity a
type ReaderT r (m : Type -> Type) a
type StateT s (m : Type -> Type) a
type MaybeT (m : Type -> Type) a

type alias Config = { budget : Number; overhead : Number }
type alias PlanState = { remaining : Number; completed : List String }

/// Optional plans share configuration and thread their remaining budget.
type alias Planner a = MaybeT (StateT PlanState (ReaderT Config Identity)) a

plan : Planner Number -> Planner Number
let plan value = value

pair : Pair String Number -> Pair String Number
let pair value = value
";

const PUBLIC: &str = "module App.Public exposing (Pair)

/// Keeps two independently typed values.
type alias Pair a b = (a, b)
";

fn assert_type_tokens(lsp: &mut Lsp, file: &Path, source: &str, name: &str) {
    let init = lsp.request("initialize", &json!({}));
    let type_token =
        init["result"]["capabilities"]["semanticTokensProvider"]["legend"]["tokenTypes"]
            .as_array()
            .expect("token legend")
            .iter()
            .position(|kind| kind == "type")
            .expect("type token");
    let tokens = lsp.request(
        "textDocument/semanticTokens/full",
        &json!({ "textDocument": { "uri": uri(file) } }),
    );
    let data = tokens["result"]["data"].as_array().expect("tokens");
    assert!(data.len().is_multiple_of(5), "{tokens}");
    let mut decoded = Vec::new();
    let (mut line, mut column) = (0, 0);
    for token in data.chunks_exact(5) {
        let delta_line = token[0].as_u64().expect("line delta");
        line += delta_line;
        column =
            if delta_line == 0 { column } else { 0 } + token[1].as_u64().expect("column delta");
        decoded.push((
            line,
            column,
            token[2].as_u64().expect("length"),
            token[3].as_u64().expect("kind"),
        ));
    }
    for (start, _) in source.match_indices(name) {
        let point = position(source, start);
        assert!(
            decoded.contains(&(
                point["line"].as_u64().expect("line"),
                point["character"].as_u64().expect("column"),
                u64::try_from(name.len()).expect("small"),
                u64::try_from(type_token).expect("small"),
            )),
            "{name} at {point}: {tokens}"
        );
    }
}

#[test]
fn private_generic_alias_hover_keeps_parameters_docs_ranges_and_type_tokens() {
    let dir = project(
        "lsp_private_generic_alias",
        &[
            ("flick.toml", LIB),
            ("src/App/Plan.fl", PLAN),
            ("src/App/Public.fl", PUBLIC),
        ],
    );
    let mut lsp = Lsp::start(&dir);

    for (source, module) in [(PLAN, "Plan"), (PUBLIC, "Public")] {
        let file = dir.join(format!("src/App/{module}.fl"));
        lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
        assert!(lsp.diagnostics(&uri(&file)).is_empty());
    }
    assert_type_tokens(&mut lsp, &dir.join("src/App/Plan.fl"), PLAN, "Planner");

    for (source, module, name, declaration, docs, from, skip_before) in [
        (
            PLAN,
            "Plan",
            "Planner",
            "type alias Planner a = MaybeT (StateT PlanState (ReaderT Config Identity)) a",
            "Optional plans share configuration and thread their remaining budget.",
            "App.Plan",
            0,
        ),
        (
            PUBLIC,
            "Public",
            "Pair",
            "type alias Pair a b = (a, b)",
            "Keeps two independently typed values.",
            "App.Public",
            PUBLIC.find("type alias").expect("declaration"),
        ),
        (
            PLAN,
            "Plan",
            "Pair",
            "type alias Pair a b = (a, b)",
            "Keeps two independently typed values.",
            "App.Public",
            PLAN.find("pair :").expect("imported alias use"),
        ),
    ] {
        let file = dir.join(format!("src/App/{module}.fl"));
        let doc = json!({ "uri": uri(&file) });

        for (start, _) in source
            .match_indices(name)
            .filter(|(start, _)| *start >= skip_before)
        {
            let hover = lsp.request(
                "textDocument/hover",
                &json!({ "textDocument": doc, "position": position(source, start + 1) }),
            );
            assert_eq!(hover["result"]["contents"]["kind"], "markdown", "{hover}");
            assert_eq!(
                hover["result"]["contents"]["value"],
                format!("```flick\n{declaration}\n```\n\n{docs}\n\nfrom `{from}`"),
                "{hover}"
            );
            let declared = source[..start].ends_with("type alias ");
            let width = if declared {
                declaration
                    .strip_prefix("type alias ")
                    .expect("alias")
                    .split(" = ")
                    .next()
                    .expect("header")
                    .len()
            } else {
                name.len()
            };
            assert_eq!(
                hover["result"]["range"],
                json!({ "start": position(source, start), "end": position(source, start + width) }),
                "{hover}"
            );
            let definition = lsp.request(
                "textDocument/definition",
                &json!({ "textDocument": doc, "position": position(source, start + 1) }),
            );
            let definition_source = if from == "App.Plan" { PLAN } else { PUBLIC };
            let definition_start =
                definition_source.find(declaration).expect("declaration") + "type alias ".len();
            assert_eq!(
                definition["result"]["range"],
                json!({ "start": position(definition_source, definition_start), "end": position(definition_source, definition_start + name.len()) }),
                "{definition}"
            );
        }
    }
    lsp.close();
}
