//! Negative / diagnostic E2E tests for `@export "name"` and library header
//! projection across `flick/js`, `flick/go`, and `test/literal` (#670).

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::common::{flick_in, project};

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

const JS_LIB_TOML: &str = "\
schema-version = 1
[project]
kind = \"lib\"
[target.js]
kind = \"library\"
";

#[test]
#[ignore = "requires just wasm-backends"]
#[allow(clippy::too_many_lines)]
fn js_and_go_reject_invalid_duplicate_generic_and_unprojectable_exports_at_span() {
    // 1. Invalid JS identifier & reserved word rejected at @export span in check & build
    for (label, bad_name, needle) in [
        (
            "digit_start",
            "123bad",
            "non-reserved JavaScript identifier",
        ),
        (
            "reserved_word",
            "class",
            "non-reserved JavaScript identifier",
        ),
    ] {
        let source = format!(
            "module App.Bad exposing (f)\n// π preserves UTF-8 byte offsets\n\n\
             @export \"{bad_name}\"\nf : Int -> Int\nlet f x = x + 1\n"
        );
        let root = project(
            &format!("export_js_invalid_{label}"),
            &[("flick.toml", JS_LIB_TOML), ("src/App/Bad.fl", &source)],
        );
        for cmd in ["check", "build"] {
            let out = flick_in(&root, &[cmd, "--offline", "--diagnostics", "json"]);
            assert_eq!(out.status.code(), Some(1), "{label} {cmd}: {out:?}");
            let diags = parse_diagnostics(&out.stderr);
            let diag = diags
                .iter()
                .find(|d| d["code"] == "FLICK-P012")
                .unwrap_or_else(|| panic!("{label} {cmd}: expected P012 in {diags:?}"));
            assert!(
                diag["message"].as_str().unwrap_or("").contains(needle),
                "{label} {cmd}: {diag}"
            );
            assert_eq!(
                span_slice(&source, diag),
                format!("@export \"{bad_name}\""),
                "{label} {cmd}: span mismatch"
            );
        }
    }

    // 2. Package-wide duplicate @export across two modules
    let mod_a = "\
module App.A exposing (a)

@export \"sharedName\"
a : Int -> Int
let a x = x + 1
";
    let mod_b = "\
module App.B exposing (b)
// π preserves UTF-8 byte offsets

@export \"sharedName\"
b : Int -> Int
let b x = x + 2
";
    let dup_root = project(
        "export_js_duplicate_pkg",
        &[
            ("flick.toml", JS_LIB_TOML),
            ("src/App/A.fl", mod_a),
            ("src/App/B.fl", mod_b),
        ],
    );
    for cmd in ["check", "build"] {
        let out = flick_in(&dup_root, &[cmd, "--offline", "--diagnostics", "json"]);
        assert_eq!(out.status.code(), Some(1), "dup {cmd}: {out:?}");
        let diags = parse_diagnostics(&out.stderr);
        let diag = diags
            .iter()
            .find(|d| d["code"] == "FLICK-P012")
            .unwrap_or_else(|| panic!("dup {cmd}: expected P012 in {diags:?}"));
        assert!(
            diag["message"].as_str().unwrap_or("").contains("duplicate"),
            "dup {cmd}: {diag}"
        );
        assert_eq!(diag["path"], "src/App/B.fl");
        assert_eq!(span_slice(mod_b, diag), "@export \"sharedName\"");
    }

    // 3. Generic and dictionary-taking @export rejected with clear reason
    for (label, decl, attr, expected_reason) in [
        (
            "generic",
            "@export \"polyId\"\nid_fn : a -> a\nlet id_fn x = x\n",
            "@export \"polyId\"",
            "generic",
        ),
        (
            "constrained",
            "@export \"showVal\"\nshow_val : Show a => a -> String\nlet show_val x = show x\n",
            "@export \"showVal\"",
            "dictionaries",
        ),
    ] {
        let source = format!("module App.Poly exposing (..)\n// π offset\n\n{decl}");
        let root = project(
            &format!("export_js_{label}"),
            &[("flick.toml", JS_LIB_TOML), ("src/App/Poly.fl", &source)],
        );
        for cmd in ["check", "build"] {
            let out = flick_in(&root, &[cmd, "--offline", "--diagnostics", "json"]);
            assert_eq!(out.status.code(), Some(1), "{label} {cmd}: {out:?}");
            let diags = parse_diagnostics(&out.stderr);
            let diag = diags
                .iter()
                .find(|d| d["code"] == "FLICK-P012")
                .unwrap_or_else(|| panic!("{label} {cmd}: expected P012 in {diags:?}"));
            assert!(
                diag["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains(expected_reason),
                "{label} {cmd}: {diag}"
            );
            assert_eq!(span_slice(&source, diag), attr);
        }
    }

    // 4. Unprojectable `.d.ts` export (`@ts none`) reports FLICK-F004 at @export span
    let unproj_source = "\
module App.Opaque exposing (Cursor, step)
// π preserves UTF-8 byte offsets

@ts none
type Cursor

@export \"stepCursor\"
step : Cursor -> Int
let step _ = 1
";
    let unproj_root = project(
        "export_js_unprojectable_dts",
        &[
            ("flick.toml", JS_LIB_TOML),
            ("src/App/Opaque.fl", unproj_source),
        ],
    );
    let built = flick_in(
        &unproj_root,
        &["build", "--offline", "--diagnostics", "json"],
    );
    assert_eq!(built.status.code(), Some(1), "unproj: {built:?}");
    let diags = parse_diagnostics(&built.stderr);
    let diag = diags
        .iter()
        .find(|d| d["code"] == "FLICK-F004")
        .unwrap_or_else(|| panic!("expected FLICK-F004 in {diags:?}"));
    assert_eq!(span_slice(unproj_source, diag), "@export \"stepCursor\"");
    assert!(!unproj_root.join("dist/index.d.ts").exists());

    // 5. Go target rejects invalid Go/C export names at @export span and rejects kind = "library"
    let go_bad_source = "\
module App.GoBad exposing (f)
// π preserves UTF-8 byte offsets

@export \"$bad\"
f : Int -> Int
let f x = x + 1
";
    let go_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[build]
target = \"go\"
";
    let go_bad_root = project(
        "export_go_invalid_name",
        &[("flick.toml", go_toml), ("src/App/GoBad.fl", go_bad_source)],
    );
    let checked = flick_in(
        &go_bad_root,
        &["check", "--offline", "--diagnostics", "json"],
    );
    assert_eq!(checked.status.code(), Some(1), "go check: {checked:?}");
    let diags = parse_diagnostics(&checked.stderr);
    let diag = diags
        .iter()
        .find(|d| d["code"] == "FLICK-P012")
        .unwrap_or_else(|| panic!("expected Go P012 in {diags:?}"));
    assert_eq!(span_slice(go_bad_source, diag), "@export \"$bad\"");

    let go_lib_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[build]
target = \"go\"
[target.go]
kind = \"library\"
";
    let go_ok_source = "\
module App.GoLib exposing (add)

@export \"add_nums\"
add : Int -> Int -> Int
let add a b = a + b
";
    let go_lib_root = project(
        "export_go_library_unsupported",
        &[
            ("flick.toml", go_lib_toml),
            ("src/App/GoLib.fl", go_ok_source),
        ],
    );
    let built = flick_in(
        &go_lib_root,
        &["build", "--offline", "--diagnostics", "json"],
    );
    assert_eq!(built.status.code(), Some(1), "go lib build: {built:?}");
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(
        stderr.contains("FLICK-P012") && stderr.contains("library"),
        "{stderr}"
    );
}

#[test]
#[ignore = "requires just wasm-backends"]
fn fixture_backend_rejects_invalid_and_unprojectable_c_header_exports() {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm");
    let digest = flick_backend::sha256(&bytes);
    let config = format!(
        "schema-version = 1\n[project]\nkind = \"lib\"\n\n\
         [build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
         sha256 = \"{digest}\"\ncomponent = \"backend.wasm\"\n\n\
         [target.literal]\nkind = \"library\"\nheaders = \"c\"\n"
    );

    let cases = [
        (
            "invalid_c_ident",
            "module App.C exposing (f)\n// π offset\n\n@export \"bad-name\"\nf : Int -> Int\nlet f x = x\n",
            "FLICK-P012",
            "@export \"bad-name\"",
            "C identifier",
        ),
        (
            "unprojectable_record_c_header",
            "module App.C exposing (f)\n// π offset\n\n@export \"take_rec\"\nf : { x : Int } -> Int\nlet f r = r.x\n",
            "FLICK-F004",
            "@export \"take_rec\"",
            "records have no C header type",
        ),
    ];

    for (label, source, expected_code, attr, expected_msg) in cases {
        let root = project(
            &format!("fixture_export_{label}"),
            &[("flick.toml", &config), ("src/App/C.fl", source)],
        );
        fs::write(root.join("backend.wasm"), &bytes).expect("write component");
        for cmd in ["check", "build"] {
            let out = flick_in(&root, &[cmd, "--offline", "--diagnostics", "json"]);
            assert_eq!(out.status.code(), Some(1), "{label} {cmd}: {out:?}");
            let diags = parse_diagnostics(&out.stderr);
            let diag = diags
                .iter()
                .find(|d| d["code"] == expected_code)
                .unwrap_or_else(|| panic!("{label} {cmd}: expected {expected_code} in {diags:?}"));
            assert!(
                diag["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains(expected_msg),
                "{label} {cmd}: {diag}"
            );
            assert_eq!(span_slice(source, diag), attr);
        }
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn plain_lib_check_without_target_validates_default_js_exports() {
    const PLAIN_LIB_TOML: &str = "\
schema-version = 1
[project]
kind = \"lib\"
";

    // Invalid, generic, and constrained exports in a plain `[project] kind = "lib"`
    // project without `[target.js]` or `[build]` (exercises `saved.is_some()` with
    // `environment_preflight == false`, on both cold and warm caches).
    for (label, decl, attr, needle) in [
        (
            "invalid",
            "@export \"123bad\"\nf : Int -> Int\nlet f x = x + 1\n",
            "@export \"123bad\"",
            "non-reserved JavaScript identifier",
        ),
        (
            "generic",
            "@export \"polyId\"\nid_fn : a -> a\nlet id_fn x = x\n",
            "@export \"polyId\"",
            "generic",
        ),
        (
            "constrained",
            "@export \"showVal\"\nshow_val : Show a => a -> String\nlet show_val x = show x\n",
            "@export \"showVal\"",
            "dictionaries",
        ),
    ] {
        let source = format!("module App.Plain exposing (..)\n// π offset\n\n{decl}");
        let root = project(
            &format!("export_plain_lib_check_{label}"),
            &[
                ("flick.toml", PLAIN_LIB_TOML),
                ("src/App/Plain.fl", &source),
            ],
        );
        for pass in ["cold", "warm"] {
            let out = flick_in(&root, &["check", "--offline", "--diagnostics", "json"]);
            assert_eq!(out.status.code(), Some(1), "{label} {pass}: {out:?}");
            let diags = parse_diagnostics(&out.stderr);
            let diag = diags
                .iter()
                .find(|d| d["code"] == "FLICK-P012")
                .unwrap_or_else(|| panic!("{label} {pass}: expected P012 in {diags:?}"));
            assert!(
                diag["message"].as_str().unwrap_or("").contains(needle),
                "{label} {pass}: {diag}"
            );
            assert_eq!(span_slice(&source, diag), attr);
        }
    }

    // Duplicate `@export` across two modules in a plain lib project.
    let mod_a = "\
module App.A exposing (a)

@export \"sharedName\"
a : Int -> Int
let a x = x + 1
";
    let mod_b = "\
module App.B exposing (b)
// π preserves UTF-8 byte offsets

@export \"sharedName\"
b : Int -> Int
let b x = x + 2
";
    let dup_root = project(
        "export_plain_lib_check_duplicate",
        &[
            ("flick.toml", PLAIN_LIB_TOML),
            ("src/App/A.fl", mod_a),
            ("src/App/B.fl", mod_b),
        ],
    );
    for pass in ["cold", "warm"] {
        let out = flick_in(&dup_root, &["check", "--offline", "--diagnostics", "json"]);
        assert_eq!(out.status.code(), Some(1), "dup {pass}: {out:?}");
        let diags = parse_diagnostics(&out.stderr);
        let diag = diags
            .iter()
            .find(|d| d["code"] == "FLICK-P012")
            .unwrap_or_else(|| panic!("dup {pass}: expected P012 in {diags:?}"));
        assert!(
            diag["message"].as_str().unwrap_or("").contains("duplicate"),
            "dup {pass}: {diag}"
        );
        assert_eq!(diag["path"], "src/App/B.fl");
        assert_eq!(span_slice(mod_b, diag), "@export \"sharedName\"");
    }
}
