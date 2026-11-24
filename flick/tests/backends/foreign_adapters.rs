//! Backend-declared foreign adapter words and type-form attributes (#664):
//! exercises `test/literal` (a third-party WASM backend component) declaring
//! brand-new adapter words (`callback <fn> data <user_data>`, `method`) and
//! type-form attribute (`@literal`), verifying valid bindings, exact-span
//! misuse reporting in both `flick check` and `flick build`, the adapter
//! words, callbacks and `retention` the backend receives through the IR, and
//! cross-backend isolation against `flick/js` and `flick/go`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::common::{flick_in, project};
use super::{CONFIG, pinned};

fn parse_diagnostics(stderr: &[u8]) -> Vec<Value> {
    String::from_utf8(stderr.to_vec())
        .expect("UTF-8")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("diagnostic json"))
        .collect()
}

fn span_slice<'a>(source: &'a str, diag: &Value) -> &'a str {
    let start =
        usize::try_from(diag["range"]["start"].as_u64().expect("start")).expect("start fits");
    let end = usize::try_from(diag["range"]["end"].as_u64().expect("end")).expect("end fits");
    &source[start..end]
}

fn literal_project(name: &str, source: &str) -> PathBuf {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm");
    let config = format!(
        "{}\n[target.literal]\ncase = \"foreign\"\n",
        pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes))
    );
    let root = project(name, &[("flick.toml", &config), ("src/App.fl", source)]);
    fs::write(root.join("backend.wasm"), &bytes).expect("write component");
    root
}

#[test]
#[ignore = "requires just wasm-backends"]
fn fixture_backend_validates_custom_adapter_words_with_exact_spans() {
    // (a) A valid binding using test/literal's custom adapter words (`callback`,
    // `method`) and `@literal` attribute checks AND builds cleanly.
    let valid_source = "\
module App exposing (Handle, register, invoke, main)

@literal \"MyHandle\"
type Handle

register : (Int -> Unit) -> Handle ->{} Unit
foreign register from \"./native.c\" \"register_cb\"
  arity 2
  callback 0 data 1

invoke : Handle -> Int ->{} Unit
foreign invoke from \"./native.c\" \"invoke_method\"
  arity 2
  method

main : Unit -> Unit
let main () = ()
";
    let root = literal_project("backend_foreign_adapters_valid", valid_source);
    let checked = flick_in(&root, &["check", "--offline"]);
    assert!(checked.status.success(), "check failed: {checked:?}");
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(built.status.success(), "build failed: {built:?}");

    // (b) Misuse of each custom adapter word is reported by the fixture backend
    // at its EXACT byte span in BOTH `flick check` and `flick build` (including
    // after multi-byte UTF-8 characters).
    let misuse_cases = [
        (
            "non_callback_param",
            "\
module App exposing (Handle, register, main)
// π preserves UTF-8 byte offsets

@literal \"MyHandle\"
type Handle

register : Int -> Handle ->{} Unit
foreign register from \"./native.c\" \"register_cb\"
  arity 2
  callback 0 data 1

main : Unit -> Unit
let main () = ()
",
            "callback 0 data 1",
            "callback function",
        ),
        (
            "same_callback_and_userdata",
            "\
module App exposing (register, main)
// π preserves UTF-8 byte offsets

register : (Int -> Unit) ->{} Unit
foreign register from \"./native.c\" \"register_cb\"
  arity 1
  callback 0 data 0

main : Unit -> Unit
let main () = ()
",
            "callback 0 data 0",
            "distinct",
        ),
        (
            "method_with_arity_zero",
            "\
module App exposing (invoke, main)
// π preserves UTF-8 byte offsets

invoke : Unit ->{} Unit
foreign invoke from \"./native.c\" \"invoke_method\"
  arity 0
  method

main : Unit -> Unit
let main () = ()
",
            "method",
            "receiver",
        ),
    ];

    for (label, source, expected_span, expected_msg) in misuse_cases {
        fs::write(root.join("src/App.fl"), source).expect("write misuse source");
        for command in ["check", "build"] {
            let out = flick_in(&root, &[command, "--offline", "--diagnostics", "json"]);
            assert_eq!(
                out.status.code(),
                Some(1),
                "{label} ({command}): expected failure, got {out:?}"
            );
            let diags = parse_diagnostics(&out.stderr);
            let diag = diags
                .iter()
                .find(|d| d["code"] == "FLICK-F001")
                .unwrap_or_else(|| panic!("{label} ({command}): expected FLICK-F001 in {diags:?}"));
            assert_eq!(diag["path"], "src/App.fl", "{label} ({command}): {diag}");
            assert_eq!(
                span_slice(source, diag),
                expected_span,
                "{label} ({command}): wrong span in {diag}"
            );
            let message = diag["message"].as_str().expect("message");
            assert!(
                message.contains(expected_msg),
                "{label} ({command}): expected `{expected_msg}` in `{message}`"
            );
        }
    }

    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn fixture_backend_receives_callback_data_words_and_retention_through_the_ir() {
    let source = "\
module App exposing (Handle, register, sort_with, main)

@literal \"MyHandle\"
type Handle

register : (Int -> Unit) -> Handle ->{} Unit
foreign register from \"./native.c\" \"register_cb\"
  arity 2
  callback 0 data 1
  retention call

sort_with : FuncRef (Int -> Int -> Int) -> Handle ->{} Unit
foreign sort_with from \"./native.c\" \"sort_with\"
  arity 2

main : Unit -> Unit
let main () = ()
";
    let root = literal_project("backend_foreign_adapter_retention", source);
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(built.status.success(), "build failed: {built:?}");
    let report: Value = serde_json::from_str(
        &fs::read_to_string(root.join("dist/foreign-adapters.json"))
            .expect("foreign-adapters.json"),
    )
    .expect("valid json");
    let entries = report.as_array().expect("entries array");
    let entry = |name: &str| {
        entries
            .iter()
            .find(|e| e["name"] == name)
            .unwrap_or_else(|| panic!("no `{name}` in {report}"))
    };
    let register = entry("register");
    assert_eq!(
        register["callbacks"],
        serde_json::json!([1, null]),
        "{report}"
    );
    assert_eq!(register["retention"], "call", "{report}");
    let word = &register["words"][0];
    assert_eq!(word["name"], "callback", "{report}");
    assert_eq!(
        word["args"],
        serde_json::json!([{ "Int": 0 }, { "Word": "data" }, { "Int": 1 }]),
        "{report}"
    );
    // a `FuncRef` reaches the backend as a callback of the host's arity,
    // with no environment to keep
    let sort_with = entry("sort_with");
    assert_eq!(
        sort_with["callbacks"],
        serde_json::json!([2, null]),
        "{report}"
    );
    assert!(sort_with["retention"].is_null(), "{report}");
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
#[allow(clippy::too_many_lines)]
fn foreign_adapter_words_and_type_form_attributes_are_isolated_per_backend() {
    // (a) JS-only words (`this`, `cancel signal`, `variadic`) and `@ts` are
    // rejected under `test/literal` at their exact spans.
    let js_on_literal = "\
module App exposing (Handle, bad, main)

@ts \"MyHandle\"
type Handle

bad : Handle -> List Int ->{Async} Unit
foreign bad from \"./m.js\" \"bad\"
  arity 2
  this
  cancel signal
  variadic

main : Unit -> Unit
let main () = ()
";
    let literal_root = literal_project("backend_foreign_isolation_literal", js_on_literal);
    let out = flick_in(
        &literal_root,
        &["check", "--offline", "--diagnostics", "json"],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let diags = parse_diagnostics(&out.stderr);
    let s014: Vec<&str> = diags
        .iter()
        .filter(|d| d["code"] == "FLICK-S014")
        .map(|d| span_slice(js_on_literal, d))
        .collect();
    assert!(
        s014.iter().any(|s| s.contains("@ts")),
        "expected FLICK-S014 for `@ts` on test/literal, got {diags:?}"
    );
    let s011: Vec<&str> = diags
        .iter()
        .filter(|d| d["code"] == "FLICK-S011")
        .map(|d| span_slice(js_on_literal, d))
        .collect();
    assert_eq!(
        s011,
        vec!["this", "cancel", "variadic"],
        "expected exact FLICK-S011 spans on test/literal, got {diags:?}"
    );
    fs::remove_dir_all(&literal_root).expect("cleanup");

    // (b) `test/literal`-only words (`callback`, `method`) and `@literal` are
    // rejected under the default `flick/js` backend.
    let literal_on_js = "\
module App exposing (Handle, bad, main)

@literal \"MyHandle\"
type Handle

bad : (Int -> Unit) -> Handle ->{} Unit
foreign bad from \"./native.c\" \"bad\"
  arity 2
  callback 0 data 1
  method

main : Unit -> Unit
let main () = ()
";
    let js_root = project(
        "backend_foreign_isolation_js",
        &[("flick.toml", CONFIG), ("src/App.fl", literal_on_js)],
    );
    let out = flick_in(&js_root, &["check", "--offline", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let diags = parse_diagnostics(&out.stderr);
    assert!(
        diags.iter().any(|d| d["code"] == "FLICK-S014"),
        "expected FLICK-S014 for `@literal` on flick/js, got {diags:?}"
    );
    let s011: Vec<&str> = diags
        .iter()
        .filter(|d| d["code"] == "FLICK-S011")
        .map(|d| span_slice(literal_on_js, d))
        .collect();
    assert_eq!(
        s011,
        vec!["callback", "method"],
        "expected exact FLICK-S011 spans on flick/js, got {diags:?}"
    );
    fs::remove_dir_all(&js_root).expect("cleanup");

    // (c) `flick/go` accepts `cancel context`, `this`, `variadic`, and `@go`,
    // while rejecting `@ts`.
    let go_config = format!("{CONFIG}\n[build]\ntarget = \"go\"\n");
    let go_valid = "\
module App exposing (Handle, fetch, main)

@go \"any\"
type Handle

fetch : Handle -> List Int ->{Async} Unit
foreign fetch from \"./m.mjs\" \"fetch\"
  arity 2
  this
  cancel context
  variadic

main : Unit -> Unit
let main () = ()
";
    let go_root = project(
        "backend_foreign_isolation_go",
        &[("flick.toml", &go_config), ("src/App.fl", go_valid)],
    );
    let ok = flick_in(&go_root, &["check", "--offline"]);
    assert!(ok.status.success(), "go check failed: {ok:?}");

    let go_with_ts = go_valid.replace("@go \"any\"", "@ts \"any\"");
    fs::write(go_root.join("src/App.fl"), &go_with_ts).expect("write @ts on go");
    let rejected = flick_in(&go_root, &["check", "--offline", "--diagnostics", "json"]);
    assert_eq!(rejected.status.code(), Some(1), "{rejected:?}");
    let diags = parse_diagnostics(&rejected.stderr);
    assert!(
        diags.iter().any(|d| d["code"] == "FLICK-S014"),
        "expected FLICK-S014 for `@ts` on flick/go, got {diags:?}"
    );
    fs::remove_dir_all(&go_root).expect("cleanup");
}
