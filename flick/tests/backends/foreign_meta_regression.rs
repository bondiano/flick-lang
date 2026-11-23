//! Regression tests for backend-declared foreign adapter metadata (#664):
//! - pruning inactive `@when` modules before `check-target` in `flick check`
//! - validating foreign adapter words in `flick pack` (when a backend has no
//!   `pack` command) and `flick doc` (`inspect::compile`)

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

    // `flick pack` on Go (which declares no `pack` command) must still run
    // backend `check-target` and reject `this` on arity 0 with FLICK-F001.
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
    let start = usize::try_from(d["range"]["start"].as_u64().expect("start")).expect("fits");
    let end = usize::try_from(d["range"]["end"].as_u64().expect("end")).expect("fits");
    assert_eq!(&bad_source[start..end], "this");
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
    let start = usize::try_from(d["range"]["start"].as_u64().expect("start")).expect("fits");
    let end = usize::try_from(d["range"]["end"].as_u64().expect("end")).expect("fits");
    assert_eq!(&bad_source[start..end], "this");

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
}

#[test]
fn js_effect_check_prunes_inactive_go_control_handler_module() {
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
            ("src/ActiveJs.fl", active_js),
            ("src/GoControl.fl", inactive_go),
        ],
    );

    let checked = flick_in(&root, &["check"]);
    assert!(
        checked.status.success(),
        "inactive @when \"go\" control handler should be pruned before JS effect check-target: {}",
        String::from_utf8_lossy(&checked.stderr)
    );

    // Removing `@when "go"` makes the control handler active on JS effect, so check-target rejects it.
    fs::write(
        root.join("src/GoControl.fl"),
        inactive_go.trim_start_matches("@when \"go\"\n"),
    )
    .expect("write active control");
    let rejected = flick_in(&root, &["check", "--diagnostics", "json"]);
    assert!(
        !rejected.status.success(),
        "active control handler on JS effect must be rejected by check-target"
    );
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("FLICK-P012"), "{stderr}");
}

#[test]
#[ignore = "requires just wasm-backends"]
#[allow(clippy::too_many_lines)]
fn literal_backend_prunes_inactive_when_js_module_and_validates_pack_and_doc() {
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
        "module Fixture.Foreign exposing (main, call_two, Handle)\n",
        "@lit \"i32\"\n",
        "type Handle\n\n",
        "call_two : Handle -> String ->{} Unit\n",
        "foreign call_two from \"./lit.js\" \"callTwo\"\n",
        "  arity 2\n",
        "  slot 0\n",
        "  slot 1\n\n",
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

    let root = project(
        "backend_foreign_meta_prune_and_pack",
        &[
            ("flick.toml", &config),
            ("src/Fixture/Foreign.fl", active_source),
            ("src/Fixture/JsOnly.fl", inactive_js_source),
        ],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("component");

    // 1. Inactive `@when "js"` module with `this` and `@ts` passes BOTH `check` and `build`.
    let checked = flick_in(&root, &["check"]);
    assert!(
        checked.status.success(),
        "flick check must prune inactive @when \"js\" module before literal check-target: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(
        built.status.success(),
        "flick build failed with inactive @when \"js\" module: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    // 2. Active-module misuse `slot 5` is rejected by `check`, `pack` (literal has no
    // `pack` command), and `doc` (`inspect::compile`) at the exact span of `slot 5`.
    let bad_active = active_source.replace("  slot 1\n", "  slot 5\n");
    fs::write(root.join("src/Fixture/Foreign.fl"), &bad_active).expect("write bad active");

    let pack_out = root.join("pack-out");
    let doc_out = root.join("doc-out");
    for args in [
        vec!["check", "--diagnostics", "json"],
        vec![
            "pack",
            "--out-dir",
            pack_out.to_str().expect("utf8"),
            "--diagnostics",
            "json",
        ],
        vec![
            "doc",
            "--out-dir",
            doc_out.to_str().expect("utf8"),
            "--diagnostics",
            "json",
        ],
    ] {
        let out = flick_in(&root, &args);
        assert!(
            !out.status.success(),
            "{args:?} unexpectedly succeeded on invalid `slot 5`"
        );
        let diags = parse_json_diags(&out.stderr);
        let d = diags
            .iter()
            .find(|d| d["code"] == "FLICK-F001")
            .unwrap_or_else(|| panic!("{args:?}: expected FLICK-F001 in {diags:?}"));
        let start = usize::try_from(d["range"]["start"].as_u64().expect("start")).expect("fits");
        let end = usize::try_from(d["range"]["end"].as_u64().expect("end")).expect("fits");
        assert_eq!(&bad_active[start..end], "slot 5", "{args:?}: {d}");
    }
    assert!(!pack_out.exists(), "failed pack must not write a tarball");

    // 3. Restoring valid `slot 1` lets `flick pack` and `flick doc` succeed.
    fs::write(root.join("src/Fixture/Foreign.fl"), active_source).expect("restore valid");
    let packed_ok = flick_in(
        &root,
        &["pack", "--out-dir", pack_out.to_str().expect("utf8")],
    );
    assert!(
        packed_ok.status.success(),
        "valid literal pack failed: {}",
        String::from_utf8_lossy(&packed_ok.stderr)
    );
    let doc_ok = flick_in(
        &root,
        &["doc", "--out-dir", doc_out.to_str().expect("utf8")],
    );
    assert!(
        doc_ok.status.success(),
        "valid literal doc failed: {}",
        String::from_utf8_lossy(&doc_ok.stderr)
    );
}
