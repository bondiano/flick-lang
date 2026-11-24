//! A fixture backend component reads full IR 0.15.0 types (#741):
//! constructor field types (`Shape`, parameterized `Tree a`, record-over
//! constructor, GADT existentials/constraints), every binder of a polymorphic
//! function, and instantiation type arguments of `List.map` and GADT calls.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::common::{flick_in, project};
use super::pinned;

const MODULE_SOURCE: &str = "\
module Fixture.TypedIr exposing (main, poly, use_all, rewritten)

type Shape =
  | Circle Float
  | Rect Float Float

type Tree a =
  | Leaf a
  | Node (Tree a) (Tree a)

type Expr a =
  | IntLit Int : Expr Int
  | Lift a : Expr a
  | Apply (Expr (b -> a)) (Expr b) : Expr a

type ShowBox =
  | Show b => ShowBox b : ShowBox

poly : (a -> b) -> List a -> b -> (List b, Int)
let poly f xs fallback =
  let mapped = List.map f xs
  let mut count = 0
  let picked =
    match mapped with
    | [head, ..._] ->
      count = count + 1
      head
    | [] -> fallback
  let _ = picked
  (mapped, count)

use_all : Unit -> (Shape, Tree Int, List String, Expr String, ShowBox)
let use_all () =
  let s = Rect 3.0 4.0
  let t = Node (Leaf 1) (Leaf 2)
  let shown = List.map show [1, 2]
  let applied : Expr String = Apply (Lift show) (IntLit 42)
  let boxed = ShowBox 42
  (s, t, shown, applied, boxed)

helper_id : a -> a
let helper_id x = x

rewritten : Unit -> (Int, String)
let rewritten () =
  let n = helper_id (20 + 22)
  let label = helper_id \"ok\"
  (n, label)

main : Unit -> Unit
let main () = ()
";

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_str().expect("string").to_owned())
        .collect()
}

#[test]
#[ignore = "requires just wasm-backends"]
#[allow(clippy::too_many_lines)]
fn fixture_backend_reads_full_ir_types_in_debug_and_release() {
    let config = pinned().replace("entry = \"App\"", "entry = \"Fixture.TypedIr\"");
    let root = project(
        "backend_typed_ir",
        &[
            ("flick.toml", &config),
            ("src/Fixture/TypedIr.fl", MODULE_SOURCE),
        ],
    );
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds the fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    let config = config.replace(&"a".repeat(64), &flick_backend::sha256(&bytes));
    fs::write(root.join("flick.toml"), config).expect("pin");

    for profile in ["debug", "release"] {
        let built = flick_in(&root, &["build", "--profile", profile, "--offline"]);
        assert!(built.status.success(), "{profile}: {built:?}");
        let report_text = fs::read_to_string(root.join("dist/typed-ir-report.json"))
            .expect("typed-ir-report.json");
        let report: Value = serde_json::from_str(&report_text).expect("valid json");
        assert_eq!(report["ir_version"], "0.18.0");
        let is_release = profile == "release";
        assert_eq!(report["profile"], profile);
        assert_eq!(report["optimized"], is_release);

        let module = &report["modules"]["Fixture.TypedIr"];
        // Shape field types
        let shape = &module["types"]["Shape"];
        assert_eq!(strings(&shape["params"]), Vec::<String>::new());
        assert_eq!(
            strings(&shape["constructors"]["Circle"]["fields"]),
            vec!["Prelude.Float"]
        );
        assert_eq!(
            strings(&shape["constructors"]["Rect"]["fields"]),
            vec!["Prelude.Float", "Prelude.Float"]
        );

        // Parameterized Tree a field types
        let tree = &module["types"]["Tree"];
        assert_eq!(strings(&tree["params"]), vec!["a"]);
        assert_eq!(strings(&tree["constructors"]["Leaf"]["fields"]), vec!["a"]);
        assert_eq!(
            strings(&tree["constructors"]["Node"]["fields"]),
            vec!["Fixture.TypedIr.Tree a", "Fixture.TypedIr.Tree a"]
        );

        // GADT declarations
        let expr_gadt = &module["types"]["Expr"];
        assert_eq!(
            strings(&expr_gadt["constructors"]["IntLit"]["fields"]),
            vec!["Prelude.Int"]
        );
        assert_eq!(
            strings(&expr_gadt["constructors"]["IntLit"]["result_args"]),
            vec!["Prelude.Int"]
        );
        assert_eq!(
            strings(&expr_gadt["constructors"]["Apply"]["existentials"]),
            vec!["b"]
        );
        assert_eq!(
            strings(&expr_gadt["constructors"]["Apply"]["fields"]),
            vec!["Fixture.TypedIr.Expr (b -> a)", "Fixture.TypedIr.Expr b"]
        );
        let show_box = &module["types"]["ShowBox"]["constructors"]["ShowBox"];
        assert_eq!(strings(&show_box["existentials"]), vec!["b"]);
        assert_eq!(show_box["constraints"][0]["class"], "Show");
        assert_eq!(strings(&show_box["fields"]), vec!["b"]);

        // Polymorphic function binders (`param`, `let`, pattern var, `let mut`) + `List.map`
        let poly = &module["values"]["poly"];
        assert_eq!(strings(&poly["type_params"]), vec!["a", "b"]);
        assert_eq!(
            poly["ty"],
            "(a -> b) -> Prelude.List a -> b -> (Prelude.List b, Prelude.Int)"
        );
        let params: Vec<&str> = poly["params"]
            .as_array()
            .expect("params")
            .iter()
            .filter_map(|p| p["ty"].as_str())
            .collect();
        assert_eq!(params, vec!["a -> b", "Prelude.List a", "b"]);
        assert!(
            poly["lets"]
                .as_array()
                .expect("lets")
                .iter()
                .any(|l| l.get("mapped").and_then(Value::as_str) == Some("Prelude.List b")),
            "{profile}: missing `mapped` let binder in {poly}"
        );
        assert!(
            poly["vars"]
                .as_array()
                .expect("vars")
                .iter()
                .any(|v| v["name"] == "count" && v["ty"] == "Prelude.Int"),
            "{profile}: missing `count` var binder in {poly}"
        );
        assert!(
            poly["arms"]
                .as_array()
                .expect("arms")
                .iter()
                .any(|a| a.get("head").and_then(Value::as_str) == Some("b")),
            "{profile}: missing `head` pattern binder in {poly}"
        );
        assert!(
            poly["globals"]
                .as_array()
                .expect("globals")
                .iter()
                .any(|g| {
                    g["module"] == "List"
                        && g["name"] == "map"
                        && strings(&g["type_args"]) == vec!["a", "b"]
                }),
            "{profile}: missing polymorphic List.map type_args in {poly}"
        );

        // Concrete calls: `List.map` and GADT constructors (`Apply`, `ShowBox`)
        let use_all = &module["values"]["use_all"];
        assert!(
            use_all["globals"]
                .as_array()
                .expect("globals")
                .iter()
                .any(|g| {
                    g["module"] == "List"
                        && g["name"] == "map"
                        && strings(&g["type_args"]) == vec!["Prelude.Int", "Prelude.String"]
                }),
            "{profile}: missing concrete List.map type_args in {use_all}"
        );
        assert!(
            use_all["con_uses"]
                .as_array()
                .expect("con_uses")
                .iter()
                .any(|c| {
                    c["name"] == "Apply"
                        && c["type_args"].as_array().is_some_and(|a| a.len() == 2)
                        && strings(&c["fields"])
                            == vec![
                                "Fixture.TypedIr.Expr (Prelude.Int -> Prelude.String)",
                                "Fixture.TypedIr.Expr Prelude.Int",
                            ]
                }),
            "{profile}: missing GADT Apply call type_args in {use_all}"
        );
        assert!(
            use_all["con_uses"]
                .as_array()
                .expect("con_uses")
                .iter()
                .any(|c| {
                    c["name"] == "ShowBox"
                        && c["dicts"] == 1
                        && strings(&c["type_args"]) == vec!["Prelude.Int"]
                        && strings(&c["fields"]) == vec!["Prelude.Int"]
                }),
            "{profile}: missing GADT ShowBox call type_args in {use_all}"
        );

        // Release profile runs a type-preserving rewrite inside the wasm32-wasip2
        // fixture backend (inlining polymorphic `helper_id` and constant-folding
        // `20 + 22`) and re-validates the transformed IR with `flick_ir::validate`.
        let rewritten = &module["values"]["rewritten"];
        assert_eq!(
            rewritten["ty"],
            "Prelude.Unit -> (Prelude.Int, Prelude.String)"
        );
        let calls_helper = rewritten["globals"]
            .as_array()
            .expect("globals")
            .iter()
            .any(|g| g["name"] == "helper_id");
        let has_folded_42 = rewritten["ints"]
            .as_array()
            .expect("ints")
            .iter()
            .any(|n| n["value"] == 42 && n["ty"] == "Prelude.Int");
        let has_typed_ok = rewritten["strings"]
            .as_array()
            .expect("strings")
            .iter()
            .any(|s| s["value"] == "ok" && s["ty"] == "Prelude.String");
        assert!(
            has_typed_ok,
            "{profile}: missing typed \"ok\" in {rewritten}"
        );
        if is_release {
            assert!(
                module["values"].get("helper_id").is_none(),
                "release rewrite should inline and drop private helper_id: {}",
                module["values"]
            );
            assert!(
                !calls_helper,
                "release rewrite should inline helper_id in rewritten: {rewritten}"
            );
            assert!(
                has_folded_42,
                "release rewrite should fold helper_id(20+22) to typed 42: {rewritten}"
            );
        } else {
            assert!(
                module["values"].get("helper_id").is_some(),
                "debug should keep private helper_id unoptimized"
            );
            assert!(
                calls_helper,
                "debug should keep uninlined call to helper_id in rewritten: {rewritten}"
            );
            assert!(
                !has_folded_42,
                "debug should not constant-fold helper_id(20+22) to 42: {rewritten}"
            );
        }
    }
    fs::remove_dir_all(root).expect("cleanup");
}
