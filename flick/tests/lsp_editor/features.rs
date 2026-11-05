use super::*;

#[test]
fn private_recursive_types_and_constructors_have_hover() {
    let source = "module App.Main exposing (make, example)

type Tree a = Leaf a | Branch (List (Tree a))
  deriving Schema

type Expr = Num Float | Neg Expr | Block Statements
  deriving Schema

type Statements = Statements (List Expr)
  deriving Schema

make : Tree Float
let make = Branch [Leaf 1]

example : Expr
let example = Block (Statements [Neg (Num 1)])
";
    let dir = project(
        "lsp_private_recursive_hover",
        &[("flick.toml", LIB), ("src/App/Main.fl", source)],
    );
    let file = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    for (needle, expected) in [
        ("Tree a =", "type Tree : Type -> Type"),
        ("Expr =", "type Expr : Type"),
        ("Statements =", "type Statements : Type"),
        ("Leaf a", "Leaf :"),
        ("Branch (", "Branch :"),
        ("Num Float", "Num :"),
        ("Neg Expr", "Neg :"),
        ("Block Statements", "Block :"),
        ("Statements (", "Statements :"),
        ("Branch [", "Branch :"),
        ("Leaf 1", "Leaf :"),
    ] {
        let start = source.find(needle).expect("hover target");
        let name = needle.split([' ', '=']).next().expect("name");
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": { "uri": uri(&file) }, "position": position(source, start + 1) }),
        );
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains(expected)),
            "{needle}: {hover}"
        );
        assert_eq!(hover["result"]["range"]["start"], position(source, start));
        assert_eq!(
            hover["result"]["range"]["end"],
            position(source, start + name.len())
        );
    }
    lsp.close();
}

const SOURCE: &str = "module App.Geo exposing (Shape(..), area, scaled)

type Shape = Circle Float | Square Float

add3 : Float -> Float -> Float -> Float
let add3 a b c = a + b + c

area : Shape -> Float
let area shape =
  let base = add3 1 2 3
  match shape with
  | Circle r -> r * base
  | Square s -> s * s

scaled : List Float -> List Float
let scaled xs =
  xs
    |> List.map (fun x -> x * 2)
";

#[test]
fn outline_tokens_folding_hints_and_signature_help() {
    let dir = project(
        "lsp_outline",
        &[("flick.toml", LIB), ("src/App/Geo.fl", SOURCE)],
    );
    let file = dir.join("src/App/Geo.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    let init = lsp.request("initialize", &json!({}));
    let caps = &init["result"]["capabilities"];
    for cap in [
        "documentSymbolProvider",
        "workspaceSymbolProvider",
        "typeDefinitionProvider",
        "foldingRangeProvider",
        "inlayHintProvider",
        "signatureHelpProvider",
        "semanticTokensProvider",
    ] {
        assert!(!caps[cap].is_null(), "{cap} in {caps}");
    }
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": SOURCE } }));
    let _ = lsp.diagnostics(&uri(&file));

    let symbols = lsp.request(
        "textDocument/documentSymbol",
        &json!({ "textDocument": doc }),
    );
    let names: Vec<&str> = symbols["result"]
        .as_array()
        .expect("symbols")
        .iter()
        .filter_map(|s| s["name"].as_str())
        .collect();
    assert_eq!(names, ["Shape", "add3", "area", "scaled"]);
    assert_eq!(symbols["result"][0]["children"][1]["name"], "Square");

    let found = lsp.request("workspace/symbol", &json!({ "query": "sqr" }));
    assert_eq!(found["result"][0]["name"], "Square", "{found}");

    let tokens = lsp.request(
        "textDocument/semanticTokens/full",
        &json!({ "textDocument": doc }),
    );
    let data = tokens["result"]["data"].as_array().expect("data");
    assert!(!data.is_empty() && data.len().is_multiple_of(5), "{tokens}");

    let folds = lsp.request("textDocument/foldingRange", &json!({ "textDocument": doc }));
    assert!(
        folds["result"]
            .as_array()
            .expect("folds")
            .iter()
            .any(|f| f["startLine"] == 10),
        "{folds}"
    );

    let hints = lsp.request("textDocument/inlayHint", &json!({ "textDocument": doc, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 20, "character": 0 } } }));
    let labels: Vec<&str> = hints["result"]
        .as_array()
        .expect("hints")
        .iter()
        .filter_map(|h| h["label"].as_str())
        .collect();
    assert_eq!(labels, [": Float", " : List Float"], "{hints}");

    let help = lsp.request("textDocument/signatureHelp", &json!({ "textDocument": doc, "position": position(SOURCE, SOURCE.find("2 3").expect("call") + 1) }));
    assert_eq!(help["result"]["activeParameter"], 1, "{help}");
    assert_eq!(
        help["result"]["signatures"][0]["label"],
        "add3 : Float -> Float -> Float -> Float"
    );

    let typedef = lsp.request(
        "textDocument/typeDefinition",
        &json!({ "textDocument": doc, "position": at(SOURCE, "shape with") }),
    );
    assert_eq!(typedef["result"]["range"]["start"]["line"], 2, "{typedef}");
    lsp.close();
}

#[test]
fn private_route_patterns_and_url_companion_have_hover_documentation() {
    let source = "module App.Pages exposing (profile)

type error NotFound = NotFound
  deriving Schema

/// Pages of this application.
type Page =
  /// The landing page.
  | Home
  /// A user profile by numeric identifier.
  | User Float
  deriving Schema, Url

page : Atom.Writable Page
let page = Atom.route Page.url Home

name : Float ->{; NotFound} String
let name id =
  if id == 404 then from_result (Err NotFound) else show id

profile : Atom.Atom (Atom.Async String {NotFound})
let profile =
  Atom.serializable_result
    (Codec.json Schema.string)
    (Codec.json NotFound.schema)
    (Atom.loader page (fun p -> task {
      match p with
      | Home -> \"home\"
      | User 404 ->
        let _ = name 404
        from_result (Err NotFound)
      | User id -> name id
    }))
";
    let dir = project(
        "lsp_route_hover",
        &[("flick.toml", LIB), ("src/App/Pages.fl", source)],
    );
    let file = dir.join("src/App/Pages.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    for (needle, name, ty, docs) in [
        ("Home ->", "Home", "Page", "The landing page."),
        (
            "User 404",
            "User",
            "Float -> Page",
            "A user profile by numeric identifier.",
        ),
        (
            "User id ->",
            "User",
            "Float -> Page",
            "A user profile by numeric identifier.",
        ),
        ("url Home", "url", "Url Page", "deriving Url"),
    ] {
        let start = source.find(needle).expect("hover target");
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": doc, "position": position(source, start + 1) }),
        );
        let markdown = hover["result"]["contents"]["value"]
            .as_str()
            .expect("hover markdown");
        assert!(markdown.contains(&format!("{name} : {ty}")), "{hover}");
        assert!(markdown.contains(docs), "{hover}");
        assert_eq!(
            hover["result"]["range"],
            json!({
                "start": position(source, start),
                "end": position(source, start + name.len()),
            }),
            "{hover}"
        );
        if name == "url" {
            for expected in ["Page.url :", "Route.print", "Route.parse"] {
                assert!(markdown.contains(expected), "{hover}");
            }
            let definition = lsp.request(
                "textDocument/definition",
                &json!({ "textDocument": doc, "position": position(source, start + 1) }),
            );
            let declared = source.find("Page =").expect("type declaration");
            assert_eq!(definition["result"]["uri"], doc["uri"]);
            assert_eq!(
                definition["result"]["range"],
                json!({
                    "start": position(source, declared),
                    "end": position(source, declared + 4),
                }),
                "{definition}"
            );
        }
    }
    lsp.close();
}
