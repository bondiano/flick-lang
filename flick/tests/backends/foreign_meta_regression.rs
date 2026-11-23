//! Regression tests for backend-declared foreign adapter metadata (#664):
//! - pruning inactive `@when` modules before `check-target` in `flick check`
//!   and selecting per-module backend specs for inactive `@when` modules
//! - validating foreign adapter words in `flick pack` and `flick doc`
//!   (including `flick doc --check-examples`)

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::common::{flick_in, project};
use super::pinned;

fn fixture_backend_bytes() -> Vec<u8> {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm")
}

fn parse_json_diags(stderr: &[u8]) -> Vec<Value> {
    String::from_utf8(stderr.to_vec())
        .expect("utf8")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("json diag"))
        .collect()
}

fn span_slice<'a>(source: &'a str, diag: &Value) -> &'a str {
    let start = usize::try_from(diag["range"]["start"].as_u64().expect("start")).expect("fits");
    let end = usize::try_from(diag["range"]["end"].as_u64().expect("end")).expect("fits");
    &source[start..end]
}

#[test]
fn go_pack_and_doc_validate_foreign_adapter_words() {
    let toml = concat!(
        "schema-version = 1\n",
        "[package]\n",
        "name = \"demo-go-foreign\"\n",
        "version = \"0.1.0\"\n",
        "[project]\n",
        "kind = \"lib\"\n",
        "[build]\n",
        "target = \"go\"\n",
    );
    let bad_source = concat!(
        "module Demo exposing (ping)\n",
        "ping : Unit ->{} Unit\n",
        "foreign ping from \"./demo.mjs\" \"ping\"\n",
        "  arity 0\n",
        "  this\n",
    );
    let root = project(
        "foreign_meta_go_pack_and_doc",
        &[("flick.toml", toml), ("src/Demo.fl", bad_source)],
    );

    // `flick pack` on Go (which declares no `pack` command) must still validate
    // foreign adapter words and reject `this` on arity 0 with FLICK-F001.
    let pack_out = root.join("pack-out");
    let packed = flick_in(
        &root,
        &[
            "pack",
            "--out-dir",
            pack_out.to_str().expect("utf8"),
            "--diagnostics",
            "json",
        ],
    );
    assert!(
        !packed.status.success(),
        "flick pack unexpectedly succeeded on invalid Go foreign metadata"
    );
    let diags = parse_json_diags(&packed.stderr);
    let d = diags
        .iter()
        .find(|d| d["code"] == "FLICK-F001")
        .unwrap_or_else(|| panic!("expected FLICK-F001 in {diags:?}"));
    assert_eq!(span_slice(bad_source, d), "this");
    assert!(
        !pack_out.exists(),
        "failed pack must not write output artifacts"
    );

    // `flick doc` (via `inspect::compile`) must also reject invalid adapter metadata.
    let doc_out = root.join("doc-out");
    let doc = flick_in(
        &root,
        &[
            "doc",
            "--out-dir",
            doc_out.to_str().expect("utf8"),
            "--diagnostics",
            "json",
        ],
    );
    assert!(
        !doc.status.success(),
        "flick doc unexpectedly succeeded on invalid Go foreign metadata"
    );
    let diags = parse_json_diags(&doc.stderr);
    let d = diags
        .iter()
        .find(|d| d["code"] == "FLICK-F001")
        .unwrap_or_else(|| panic!("expected FLICK-F001 in {diags:?}"));
    assert_eq!(span_slice(bad_source, d), "this");

    // Fixing the binding lets both `flick pack` and `flick doc` succeed.
    let good_source = concat!(
        "module Demo exposing (ping)\n",
        "ping : String ->{} Unit\n",
        "foreign ping from \"./demo.mjs\" \"ping\"\n",
        "  arity 1\n",
        "  this\n",
    );
    fs::write(root.join("src/Demo.fl"), good_source).expect("write good");
    let packed_ok = flick_in(
        &root,
        &["pack", "--out-dir", pack_out.to_str().expect("utf8")],
    );
    assert!(
        packed_ok.status.success(),
        "valid Go pack failed: {}",
        String::from_utf8_lossy(&packed_ok.stderr)
    );
    let doc_ok = flick_in(
        &root,
        &["doc", "--out-dir", doc_out.to_str().expect("utf8")],
    );
    assert!(
        doc_ok.status.success(),
        "valid Go doc failed: {}",
        String::from_utf8_lossy(&doc_ok.stderr)
    );
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn go_and_js_preserve_inactive_when_foreign_modules() {
    let go_toml = concat!(
        "schema-version = 1\n",
        "[project]\n",
        "kind = \"lib\"\n",
        "[build]\n",
        "target = \"go\"\n",
    );
    let go_active = concat!(
        "module ActiveGo exposing (Handle, fetch)\n",
        "@go \"any\"\n",
        "type Handle\n\n",
        "fetch : Handle ->{Async} Unit\n",
        "foreign fetch from \"./m.mjs\" \"fetch\"\n",
        "  arity 1\n",
        "  cancel context\n",
    );
    let js_inactive = concat!(
        "@when \"js\"\n",
        "module JsOnly exposing (JsDate, iso)\n",
        "@ts \"Date\"\n",
        "type JsDate\n\n",
        "iso : JsDate ->{Async} String\n",
        "foreign iso from \"./js.mjs\" \"toISOString\"\n",
        "  arity 1\n",
        "  this\n",
        "  cancel signal\n",
    );
    let go_root = project(
        "foreign_meta_go_preserves_when_js",
        &[
            ("flick.toml", go_toml),
            ("src/ActiveGo.fl", go_active),
            ("src/JsOnly.fl", js_inactive),
        ],
    );
    let checked = flick_in(&go_root, &["check"]);
    assert!(
        checked.status.success(),
        "flick check on Go failed with inactive @when \"js\" module: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    fs::remove_dir_all(&go_root).expect("cleanup");

    let js_toml = concat!(
        "schema-version = 1\n",
        "[project]\n",
        "kind = \"lib\"\n",
        "[build]\n",
        "target = \"js\"\n",
    );
    let js_active = concat!(
        "module ActiveJs exposing (JsDate, iso)\n",
        "@ts \"Date\"\n",
        "type JsDate\n\n",
        "iso : JsDate ->{Async} String\n",
        "foreign iso from \"./js.mjs\" \"toISOString\"\n",
        "  arity 1\n",
        "  this\n",
        "  cancel signal\n",
    );
    let go_inactive = concat!(
        "@when \"go\"\n",
        "module GoOnly exposing (Handle, fetch)\n",
        "@go \"any\"\n",
        "type Handle\n\n",
        "fetch : Handle ->{Async} Unit\n",
        "foreign fetch from \"./m.mjs\" \"fetch\"\n",
        "  arity 1\n",
        "  cancel context\n",
    );
    let js_root = project(
        "foreign_meta_js_preserves_when_go",
        &[
            ("flick.toml", js_toml),
            ("src/ActiveJs.fl", js_active),
            ("src/GoOnly.fl", go_inactive),
        ],
    );
    let checked = flick_in(&js_root, &["check"]);
    assert!(
        checked.status.success(),
        "flick check on JS failed with inactive @when \"go\" module: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    fs::remove_dir_all(&js_root).expect("cleanup");
}

#[test]
fn js_effect_prunes_inactive_go_control_handler_module() {
    let toml = concat!(
        "schema-version = 1\n",
        "[project]\n",
        "kind = \"lib\"\n",
        "[build]\n",
        "target = \"js\"\n",
        "[target.js]\n",
        "runtime = \"effect\"\n",
    );
    let active_js = concat!(
        "module ActiveJs exposing (call_it)\n",
        "call_it : String ->{} Unit\n",
        "foreign call_it from \"./m.mjs\" \"callIt\"\n",
        "  arity 1\n",
        "  this\n",
    );
    let inactive_go = concat!(
        "@when \"go\"\n",
        "module GoControl exposing (collect)\n",
        "effect Emit a =\n",
        "  control emit : a ->{} Unit\n",
        "collect : (Unit ->{Emit a | r; e} Unit) ->{r; e} List a\n",
        "let collect body =\n",
        "  try body () with\n",
        "  | Emit.emit x -> [x] ++ resume ()\n",
        "  | return () -> []\n",
    );
    let root = project(
        "foreign_meta_js_effect_prunes_go_control",
        &[
            ("flick.toml", toml),
            (
                "package.json",
                "{\"name\":\"effect-prune\",\"type\":\"module\",\"dependencies\":{\"@flick-lang/effect-ts\":\"0.1.0\"}}\n",
            ),
            (
                "node_modules/@flick-lang/effect-ts/package.json",
                "{\"name\":\"@flick-lang/effect-ts\",\"version\":\"0.1.0\",\"dependencies\":{\"effect\":\"4.1.2\"}}\n",
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/effect/package.json",
                "{\"version\":\"4.1.2\"}\n",
            ),
            ("src/ActiveJs.fl", active_js),
            ("src/GoControl.fl", inactive_go),
        ],
    );

    let checked = flick_in(&root, &["check"]);
    assert!(
        checked.status.success(),
        "inactive @when \"go\" control handler should pass JS effect check: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(
        built.status.success(),
        "inactive @when \"go\" control handler should be pruned before JS effect build: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    // Removing `@when "go"` makes the control handler active on JS effect, so build rejects it.
    fs::write(
        root.join("src/GoControl.fl"),
        inactive_go.trim_start_matches("@when \"go\"\n"),
    )
    .expect("write active control");
    let rejected = flick_in(&root, &["build", "--offline", "--diagnostics", "json"]);
    assert!(
        !rejected.status.success(),
        "active control handler on JS effect must be rejected by build"
    );
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("FLICK-P012"), "{stderr}");
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
#[allow(clippy::too_many_lines)]
fn literal_backend_prunes_inactive_when_modules_and_validates_pack_and_doc() {
    let bytes = fixture_backend_bytes();
    let sha = flick_backend::sha256(&bytes);
    let config = format!(
        "schema-version = 1\n\
         [package]\n\
         name = \"demo-literal\"\n\
         version = \"0.1.0\"\n\
         {}\n\
         [target.literal]\n\
         case = \"foreign\"\n",
        pinned()
            .replace("schema-version = 1\n", "")
            .replace("entry = \"App\"", "entry = \"Fixture.Foreign\"")
            .replace(&"a".repeat(64), &sha)
    );
    let active_source = concat!(
        "module Fixture.Foreign exposing (main, register, Handle)\n",
        "@literal \"MyHandle\"\n",
        "type Handle\n\n",
        "register : (Int -> Unit) -> Handle ->{} Unit\n",
        "foreign register from \"./native.c\" \"register_cb\"\n",
        "  arity 2\n",
        "  callback 0 data 1\n\n",
        "main : Unit -> Unit\n",
        "let main () = ()\n",
    );
    let inactive_js_source = concat!(
        "@when \"js\"\n",
        "module Fixture.JsOnly exposing (JsDate, js_method)\n",
        "@ts \"Date\"\n",
        "type JsDate\n\n",
        "js_method : JsDate ->{} String\n",
        "foreign js_method from \"./js_only.mjs\" \"toISOString\"\n",
        "  arity 1\n",
        "  this\n",
    );
    // Inactive on default `alpha` dialect: `method` on `arity 0` would be rejected
    // by `test/literal`'s `check-target` if inactive `@when` modules were not pruned.
    let inactive_beta_source = concat!(
        "@when \"literal && beta\"\n",
        "module Fixture.BetaOnly exposing (beta_only)\n",
        "beta_only : Unit ->{} Unit\n",
        "foreign beta_only from \"./beta.c\" \"beta_only\"\n",
        "  arity 0\n",
        "  method\n",
    );

    let root = project(
        "backend_foreign_meta_prune_and_pack",
        &[
            ("flick.toml", &config),
            ("src/Fixture/Foreign.fl", active_source),
            ("src/Fixture/JsOnly.fl", inactive_js_source),
            ("src/Fixture/BetaOnly.fl", inactive_beta_source),
        ],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("component");

    // 1. Inactive `@when "js"` (with `this` and `@ts`) and inactive `@when "literal && beta"`
    // pass BOTH `check` and `build`.
    let checked = flick_in(&root, &["check", "--offline"]);
    assert!(
        checked.status.success(),
        "flick check must prune inactive @when modules before literal check-target: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(
        built.status.success(),
        "flick build failed with inactive @when modules: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    // 2. Active-module misuse `callback 0 data 0` is rejected by `check`, `pack`,
    // `doc` (`inspect::compile`), and `doc --check-examples` (`target_override`)
    // at the exact span of `callback 0 data 0`.
    let bad_active = active_source.replace("  callback 0 data 1\n", "  callback 0 data 0\n");
    fs::write(root.join("src/Fixture/Foreign.fl"), &bad_active).expect("write bad active");

    let pack_out = root.join("pack-out");
    let doc_out = root.join("doc-out");
    let doc_checked_out = root.join("doc-checked-out");
    for args in [
        vec!["check", "--offline", "--diagnostics", "json"],
        vec![
            "pack",
            "--offline",
            "--out-dir",
            pack_out.to_str().expect("utf8"),
            "--diagnostics",
            "json",
        ],
        vec![
            "doc",
            "--offline",
            "--out-dir",
            doc_out.to_str().expect("utf8"),
            "--diagnostics",
            "json",
        ],
        vec![
            "doc",
            "--offline",
            "--check-examples",
            "--out-dir",
            doc_checked_out.to_str().expect("utf8"),
            "--diagnostics",
            "json",
        ],
    ] {
        let out = flick_in(&root, &args);
        assert!(
            !out.status.success(),
            "{args:?} unexpectedly succeeded on invalid `callback 0 data 0`"
        );
        let diags = parse_json_diags(&out.stderr);
        let d = diags
            .iter()
            .find(|d| d["code"] == "FLICK-F001")
            .unwrap_or_else(|| panic!("{args:?}: expected FLICK-F001 in {diags:?}"));
        assert_eq!(
            span_slice(&bad_active, d),
            "callback 0 data 0",
            "{args:?}: {d}"
        );
    }
    assert!(!pack_out.exists(), "failed pack must not write a tarball");

    // 3. Restoring valid `callback 0 data 1` lets `flick pack`, `flick doc`,
    // and `flick doc --check-examples` succeed.
    fs::write(root.join("src/Fixture/Foreign.fl"), active_source).expect("restore valid");
    let packed_ok = flick_in(
        &root,
        &[
            "pack",
            "--offline",
            "--out-dir",
            pack_out.to_str().expect("utf8"),
        ],
    );
    assert!(
        packed_ok.status.success(),
        "valid literal pack failed: {}",
        String::from_utf8_lossy(&packed_ok.stderr)
    );
    let doc_ok = flick_in(
        &root,
        &[
            "doc",
            "--offline",
            "--out-dir",
            doc_out.to_str().expect("utf8"),
        ],
    );
    assert!(
        doc_ok.status.success(),
        "valid literal doc failed: {}",
        String::from_utf8_lossy(&doc_ok.stderr)
    );
    let doc_checked_ok = flick_in(
        &root,
        &[
            "doc",
            "--offline",
            "--check-examples",
            "--out-dir",
            doc_checked_out.to_str().expect("utf8"),
        ],
    );
    assert!(
        doc_checked_ok.status.success(),
        "valid literal doc --check-examples failed: {}",
        String::from_utf8_lossy(&doc_checked_ok.stderr)
    );
    fs::remove_dir_all(&root).expect("cleanup");
}
