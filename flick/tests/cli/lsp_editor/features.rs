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

#[test]
fn a_gadt_branch_hover_shows_the_refined_type() {
    let source = "module App.Main exposing (Ty(..), describe)

type Ty a =
  | TInt : Ty Int
  | TStr : Ty String

describe : forall a. Ty a -> a -> String
let describe t x =
  match t with
  | TInt -> show (x + 1)
  | TStr -> x ++ \"!\"
";
    let dir = project(
        "lsp_gadt_hover",
        &[("flick.toml", LIB), ("src/App/Main.fl", source)],
    );
    let file = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": source } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    for (needle, expected) in [("x + 1", "x : Int (= a)"), ("x ++", "x : String (= a)")] {
        let start = source.find(needle).expect("hover target");
        let hover = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": { "uri": uri(&file) }, "position": position(source, start) }),
        );
        assert!(
            hover["result"]["contents"]["value"]
                .as_str()
                .is_some_and(|s| s.contains(expected)),
            "{needle}: {hover}"
        );
    }
    lsp.close();
}

const SOURCE: &str = "module App.Geo exposing (Shape(..), area, scaled, counted)

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

counted : List Float -> Int
let counted xs =
  List.length
    <| xs
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
    assert_eq!(names, ["Shape", "add3", "area", "scaled", "counted"]);
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

    let hints = lsp.request("textDocument/inlayHint", &json!({ "textDocument": doc, "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 30, "character": 0 } } }));
    let labels: Vec<&str> = hints["result"]
        .as_array()
        .expect("hints")
        .iter()
        .filter_map(|h| h["label"].as_str())
        .collect();
    assert_eq!(labels, [": Float", " : List Float", " : Int"], "{hints}");

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
  if id == 404 then Fail.fail NotFound else show id

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
        Fail.fail NotFound
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

#[test]
fn lsp_hover_shows_data_variance_after_edit_and_for_installed_package() {
    let dep_manifest = "schema-version = 1\n[package]\nname = \"geo\"\nversion = \"1.0.0\"\n[project]\nkind = \"lib\"\n";
    let dep_src = "module Geo exposing (Box, Sink)\nopaque type Box a = Box { value : a }\nopaque type Sink a = Sink (a -> Unit)\n";
    let app_manifest = format!("{LIB}\n[dependencies]\ngeo = {{ path = \"../dep\" }}\n");
    let main_v1 = "module App.Main exposing (LocalBox, use_pkg, use_local)\nimport Geo exposing (Box, Sink)\nopaque type LocalBox a = LocalBox a\nuse_pkg : Box Int -> Sink Int -> Unit\nlet use_pkg _ _ = ()\nuse_local : LocalBox Int -> LocalBox Int\nlet use_local x = x\n";
    let main_v2 = main_v1.replace(
        "opaque type LocalBox a = LocalBox a",
        "opaque type LocalBox a = LocalBox (Ref a)",
    );
    let dir = project(
        "lsp_variance_hover",
        &[
            ("dep/flick.toml", dep_manifest),
            ("dep/src/Geo.fl", dep_src),
            ("app/flick.toml", &app_manifest),
            ("app/src/App/Main.fl", main_v1),
        ],
    );
    let app_dir = dir.join("app");
    let file = app_dir.join("src/App/Main.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start(&app_dir);
    let hover_at = |lsp: &mut Lsp, text: &str, needle: &str, shift: usize| {
        let pos = position(text, text.find(needle).expect(needle) + shift);
        let res = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": doc, "position": pos }),
        );
        res["result"]["contents"]["value"]
            .as_str()
            .expect("hover text")
            .to_owned()
    };
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": main_v1 } }),
    );
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    for (needle, shift, expected) in [
        ("Box Int", 1, "Variance: `a` (`+`, covariant)"),
        ("Sink Int", 1, "Variance: `a` (`-`, contravariant)"),
        ("LocalBox Int", 1, "Variance: `a` (`+`, covariant)"),
        ("a = LocalBox", 0, "Variance: `+` (covariant)"),
    ] {
        let got = hover_at(&mut lsp, main_v1, needle, shift);
        assert!(got.contains(expected), "{needle}: {got}");
    }
    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri(&file), "version": 2 },
            "contentChanges": [{ "text": main_v2 }],
        }),
    );
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    for (needle, shift, expected) in [
        ("LocalBox Int", 1, "Variance: `a` (`inv`, invariant)"),
        ("a = LocalBox", 0, "Variance: `inv` (invariant)"),
    ] {
        let got = hover_at(&mut lsp, &main_v2, needle, shift);
        assert!(got.contains(expected), "{needle}: {got}");
    }
    lsp.close();
}

#[test]
fn lsp_hover_shows_variance_for_fetched_locked_package_with_private_module_wrapper() {
    let hash = "4f0e0bd5b9b8b1f2a0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a3b2c1d0e9f8a7";
    let dep_manifest = "schema-version = 1\n[package]\nname = \"geo\"\nversion = \"1.0.0\"\n[project]\nkind = \"lib\"\n";
    let secret_src = "private module Geo.Secret exposing (Inner, Key(..))\ntype Inner a = Inner a\ntype Key = Key Int\n";
    let dep_src = "\
module Geo exposing (Box, Sink, Wrapper(..), Tag(..), BoxErr(..))
import Geo.Secret exposing (Inner, Key)
opaque type Box a = Box { value : a }
opaque type Sink a = Sink (a -> Unit)
type Wrapper a = Wrapper (Inner a)
type Tag a =
  | TagKey : Tag Key
  | TagVal a : Tag a
type error BoxErr a = BoxErr (Inner a)
";
    let app_manifest = format!(
        "{LIB}\n[dependencies]\ngeo = {{ git = \"https://example.com/geo\", tag = \"v1.0.0\", subdir = \"lib\" }}\n"
    );
    let lock = format!(
        "version = 1\n\n[[package]]\nname = \"geo\"\nversion = \"1.0.0\"\nsource = \"git+https://example.com/geo?tag=v1.0.0&subdir=lib#0123abcd\"\nchecksum = \"sha256:{hash}\"\n"
    );
    let main_src = "\
module App.Main exposing (widen_box, narrow_sink, widen_wrap, widen_tag, widen_err)
import Geo exposing (Box, Sink, Wrapper(..), Tag(..), BoxErr(..))
widen_box : Box (Float -> Float) -> Box (Float ->{Async} Float)
let widen_box b = b
narrow_sink : Sink (Float ->{Async} Float) -> Sink (Float -> Float)
let narrow_sink s = s
widen_wrap : Wrapper (Float -> Float) -> Wrapper (Float ->{Async} Float)
let widen_wrap w = w
widen_tag : Tag (Float -> Float) -> Tag (Float ->{Async} Float)
let widen_tag t = t
widen_err : BoxErr (Float -> Float) -> BoxErr (Float ->{Async} Float)
let widen_err e = e
";
    let dir = project(
        "lsp_fetched_variance_hover",
        &[
            ("app/flick.toml", &app_manifest),
            ("app/flick.lock", &lock),
            ("app/src/App/Main.fl", main_src),
            (&format!("store/{hash}/lib/flick.toml"), dep_manifest),
            (&format!("store/{hash}/lib/src/Geo/Secret.fl"), secret_src),
            (&format!("store/{hash}/lib/src/Geo.fl"), dep_src),
        ],
    );
    let app_dir = dir.join("app");
    let store = dir.join("store");
    let cache = dir.join("cache");
    let file = app_dir.join("src/App/Main.fl");
    let doc = json!({ "uri": uri(&file) });
    let mut lsp = Lsp::start_with_env(
        &app_dir,
        &[("FLICK_STORE", &store), ("XDG_CACHE_HOME", &cache)],
    );
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": main_src } }),
    );
    let diags = lsp.diagnostics(&uri(&file));
    assert!(
        diags.is_empty(),
        "fetched package widening failed: {diags:?}"
    );
    for (needle, expected) in [
        (": Box (", "Variance: `a` (`+`, covariant)"),
        (": Sink (", "Variance: `a` (`-`, contravariant)"),
        (": Wrapper (", "Variance: `a` (`+`, covariant)"),
        (": Tag (", "Variance: `a` (`+`, covariant)"),
        (": BoxErr (", "Variance: `a` (`+`, covariant)"),
    ] {
        let pos = position(main_src, main_src.find(needle).expect(needle) + 3);
        let res = lsp.request(
            "textDocument/hover",
            &json!({ "textDocument": doc, "position": pos }),
        );
        let markdown = res["result"]["contents"]["value"]
            .as_str()
            .expect("hover text");
        assert!(markdown.contains(expected), "{needle}: {markdown}");
    }
    lsp.close();
}
