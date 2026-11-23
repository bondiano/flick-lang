//! End-to-end tests for `from inline` foreign bindings and `foreign module inline`
//! preambles (#665) across JS, Go, and third-party WASM backends (`test/literal`).

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::common::{flick_in, project};
use super::{CONFIG, pinned};

fn parse_json_diags(stderr: &[u8]) -> Vec<Value> {
    String::from_utf8(stderr.to_vec())
        .expect("UTF-8")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("diagnostic json"))
        .collect()
}

fn diag_line_and_slice<'a>(source: &'a str, diag: &Value) -> (usize, &'a str) {
    let start =
        usize::try_from(diag["range"]["start"].as_u64().expect("start")).expect("start fits");
    let end = usize::try_from(diag["range"]["end"].as_u64().expect("end")).expect("end fits");
    let line = source[..start].bytes().filter(|&b| b == b'\n').count() + 1;
    (line, &source[start..end])
}

#[test]
fn inline_js_syntax_errors_map_to_source_spans_in_check_and_build() {
    let cases = [
        (
            "single_line",
            "\
module App exposing (main)
// π preserves UTF-8 offsets
cos : Float -> Float
foreign pure cos from inline \"(d) => (1 + )\"
main : Unit -> Unit
let main () = ()
",
            4usize,
            ")",
        ),
        (
            "multiline_binding",
            "\
module App exposing (main)
// π preserves UTF-8 offsets
calc : Float -> Float
foreign pure calc from inline \"\"\"
  (d) => {
    const ok = d + 1;
    return (ok * );
  }
  \"\"\"
main : Unit -> Unit
let main () = ()
",
            7usize,
            ")",
        ),
        (
            "multiline_preamble",
            "\
module App exposing (main)
// π preserves UTF-8 offsets
foreign module inline \"\"\"
  const scale = 2;
  function broken(x) {
    return [1, ;
  }
  \"\"\"
main : Unit -> Unit
let main () = ()
",
            6usize,
            ";",
        ),
    ];

    for (name, source, expected_line, expected_slice) in cases {
        let root = project(
            &format!("inline_js_err_{name}"),
            &[("flick.toml", CONFIG), ("src/App.fl", source)],
        );
        for cmd in ["check", "build"] {
            let out = flick_in(&root, &[cmd, "--offline", "--diagnostics", "json"]);
            assert!(!out.status.success(), "{name}/{cmd} unexpectedly succeeded");
            let diags = parse_json_diags(&out.stderr);
            let diag = diags
                .iter()
                .find(|d| d["code"] == "FLICK-F011")
                .unwrap_or_else(|| panic!("{name}/{cmd}: expected FLICK-F011 in {diags:?}"));
            assert_eq!(diag["path"], "src/App.fl", "{name}/{cmd}: {diag}");
            let (line, slice) = diag_line_and_slice(source, diag);
            assert_eq!(line, expected_line, "{name}/{cmd}: {diag}");
            assert_eq!(slice, expected_slice, "{name}/{cmd}: {diag}");
        }
        fs::remove_dir_all(&root).expect("cleanup");
    }
}

#[test]
fn inline_go_syntax_errors_map_to_source_spans_in_check_and_build() {
    let config = format!("{CONFIG}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
    let cases = [
        (
            "single_line",
            "\
module App exposing (main)
// π preserves UTF-8 offsets
cos : Float -> Float
foreign pure cos from inline \"func(fb *flickrt.Fiber, d flickrt.Value) flickrt.Value { return (1 + ) }\"
main : Unit -> Unit
let main () = ()
",
            4usize,
            ")",
        ),
        (
            "multiline_binding",
            "\
module App exposing (main)
// π preserves UTF-8 offsets
calc : Float -> Float
foreign pure calc from inline \"\"\"
  func(fb *flickrt.Fiber, d flickrt.Value) flickrt.Value {
    ok := d.(float64)
    return (ok * )
  }
  \"\"\"
main : Unit -> Unit
let main () = ()
",
            7usize,
            ")",
        ),
        (
            "multiline_preamble",
            "\
module App exposing (main)
// π preserves UTF-8 offsets
foreign module inline \"\"\"
  import \"math\"

  func helper(x float64 {
    return math.Cos(x)
  }
  \"\"\"
main : Unit -> Unit
let main () = ()
",
            6usize,
            "{",
        ),
    ];

    for (name, source, expected_line, expected_slice) in cases {
        let root = project(
            &format!("inline_go_err_{name}"),
            &[("flick.toml", &config), ("src/App.fl", source)],
        );
        for cmd in ["check", "build"] {
            let out = flick_in(&root, &[cmd, "--offline", "--diagnostics", "json"]);
            assert!(!out.status.success(), "{name}/{cmd} unexpectedly succeeded");
            let diags = parse_json_diags(&out.stderr);
            let diag = diags
                .iter()
                .find(|d| d["code"] == "FLICK-F011")
                .unwrap_or_else(|| panic!("{name}/{cmd}: expected FLICK-F011 in {diags:?}"));
            assert_eq!(diag["path"], "src/App.fl", "{name}/{cmd}: {diag}");
            let (line, slice) = diag_line_and_slice(source, diag);
            assert_eq!(line, expected_line, "{name}/{cmd}: {diag}");
            assert_eq!(slice, expected_slice, "{name}/{cmd}: {diag}");
        }
        fs::remove_dir_all(&root).expect("cleanup");
    }
}

#[test]
#[ignore = "requires Node"]
fn inline_js_binding_and_shared_preamble_run_on_node() {
    let source = "\
module App exposing (main)

foreign module inline \"\"\"
  const OFFSET = 10;
  function scaleAndShift(x, factor) {
    return x * factor + OFFSET;
  }
  \"\"\"

cos : Float -> Float
foreign pure cos from inline \"(d) => Math.cos(d)\"

double_plus : Int -> Int
foreign pure double_plus from inline \"\"\"
  (n) => scaleAndShift(n, 2)
  \"\"\"

triple_plus : Int -> Int
foreign pure triple_plus from inline \"(n) => scaleAndShift(n, 3)\"

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (cos 0.0))
  Platform.stdout_line (show (double_plus 2))
  Platform.stdout_line (show (triple_plus 4))
";
    let root = project(
        "inline_js_run",
        &[("flick.toml", CONFIG), ("src/App.fl", source)],
    );
    for profile in ["debug", "release"] {
        let built = flick_in(&root, &["build", "--profile", profile, "--offline"]);
        assert!(
            built.status.success(),
            "{profile} build failed: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        let run = flick_in(&root, &["run", "--profile", profile, "--offline"]);
        assert!(
            run.status.success(),
            "{profile} run failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8(run.stdout).expect("UTF-8"),
            "1\n14\n22\n",
            "{profile}"
        );
    }
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
#[ignore = "requires Go"]
fn inline_go_binding_and_shared_preamble_run_native() {
    let config = format!("{CONFIG}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
    let source = "\
module App exposing (main)

foreign module inline \"\"\"
  import \"math\"

  const inlineOffset int32 = 10

  func scaleAndShift(x int32, factor int32) int32 {
    return x*factor + inlineOffset
  }
  \"\"\"

cos : Float -> Float
foreign pure cos from inline \"func(fb *flickrt.Fiber, d flickrt.Value) flickrt.Value { return math.Cos(d.(float64)) }\"

double_plus : Int -> Int
foreign pure double_plus from inline \"\"\"
  func(fb *flickrt.Fiber, n flickrt.Value) flickrt.Value {
    return scaleAndShift(n.(int32), 2)
  }
  \"\"\"

triple_plus : Int -> Int
foreign pure triple_plus from inline \"func(fb *flickrt.Fiber, n flickrt.Value) flickrt.Value { return scaleAndShift(n.(int32), 3) }\"

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (cos 0.0))
  Platform.stdout_line (show (double_plus 2))
  Platform.stdout_line (show (triple_plus 4))
";
    let root = project(
        "inline_go_run",
        &[("flick.toml", &config), ("src/App.fl", source)],
    );
    for profile in ["debug", "release"] {
        let built = flick_in(&root, &["build", "--profile", profile, "--offline"]);
        assert!(
            built.status.success(),
            "{profile} build failed: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        let run = flick_in(&root, &["run", "--profile", profile, "--offline"]);
        assert!(
            run.status.success(),
            "{profile} run failed: {}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            String::from_utf8(run.stdout).expect("UTF-8"),
            "1\n14\n22\n",
            "{profile}"
        );
    }
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn literal_backend_receives_inline_foreign_entries_and_maps_spans() {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm");
    let config = format!(
        "{}\n[target.literal]\ncase = \"foreign\"\n",
        pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes))
    );
    let ok_source = "\
module App exposing (main, custom)

foreign module inline \"\"\"
  preamble line 1
  preamble line 2
  \"\"\"

custom : Int -> Int
foreign pure custom from inline \"inline body\"

main : Unit -> Unit
let main () = ()
";
    let root = project(
        "inline_literal_wit",
        &[("flick.toml", &config), ("src/App.fl", ok_source)],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("write component");

    let checked = flick_in(&root, &["check", "--offline"]);
    assert!(checked.status.success(), "check failed: {checked:?}");
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(built.status.success(), "build failed: {built:?}");
    let report: Value = serde_json::from_str(
        &fs::read_to_string(root.join("dist/inline-foreign.json")).expect("inline-foreign.json"),
    )
    .expect("valid json");
    let entries = report.as_array().expect("entries array");
    assert_eq!(entries.len(), 2, "{report}");
    assert_eq!(entries[0]["module"], "App");
    assert!(entries[0]["name"].is_null());
    assert_eq!(entries[0]["code"], "preamble line 1\npreamble line 2");
    assert_eq!(entries[1]["module"], "App");
    assert_eq!(entries[1]["name"], "custom");
    assert_eq!(entries[1]["code"], "inline body");

    let bad_source = "\
module App exposing (main, custom)
// π preserves UTF-8 offsets
custom : Int -> Int
foreign pure custom from inline \"\"\"
  good line
  SYNTAX_ERROR here
  \"\"\"
main : Unit -> Unit
let main () = ()
";
    fs::write(root.join("src/App.fl"), bad_source).expect("write bad");
    for cmd in ["check", "build"] {
        let out = flick_in(&root, &[cmd, "--offline", "--diagnostics", "json"]);
        assert!(!out.status.success(), "{cmd} unexpectedly succeeded");
        let diags = parse_json_diags(&out.stderr);
        let diag = diags
            .iter()
            .find(|d| d["code"] == "FLICK-F011")
            .unwrap_or_else(|| panic!("{cmd}: expected FLICK-F011 in {diags:?}"));
        assert_eq!(diag["path"], "src/App.fl");
        let (line, slice) = diag_line_and_slice(bad_source, diag);
        assert_eq!(line, 6, "{cmd}: {diag}");
        assert_eq!(slice, "SYNTAX_ERROR", "{cmd}: {diag}");
    }
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
#[ignore = "requires Go"]
fn inline_go_unreached_binding_skips_preamble_imports() {
    let config = format!("{CONFIG}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
    let source = "\
module App exposing (main, unused_cos)

foreign module inline \"\"\"
  import \"math\"

  func unusedHelper(x float64) float64 {
    return math.Cos(x)
  }
  \"\"\"

unused_cos : Float -> Float
foreign pure unused_cos from inline \"func(fb *flickrt.Fiber, d flickrt.Value) flickrt.Value { return unusedHelper(d.(float64)) }\"

main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line \"ok\"
";
    let root = project(
        "inline_go_unreached_preamble",
        &[("flick.toml", &config), ("src/App.fl", source)],
    );
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(
        built.status.success(),
        "build failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = flick_in(&root, &["run", "--offline"]);
    assert!(
        run.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8(run.stdout).expect("UTF-8"), "ok\n");
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn inspect_iface_skips_js_inline_validation_for_uncached_custom_backend() {
    let config = pinned().replace("test/literal", "custom/nonjs");
    let source = "\
module App exposing (main, custom)

foreign module inline \"not valid js (((\"

custom : Int -> Int
foreign pure custom from inline \"also not valid js (((\"
  custom_adapter

main : Unit -> Unit
let main () = ()
";
    let root = project(
        "inline_uncached_custom_iface",
        &[("flick.toml", &config), ("src/App.fl", source)],
    );
    let out = flick_in(
        &root,
        &[
            "inspect",
            "App",
            "--emit",
            "iface",
            "--offline",
            "--diagnostics",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "inspect --emit iface failed with false-positive backend validation: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).expect("UTF-8");
    assert!(stdout.contains("custom"), "{stdout}");
    fs::remove_dir_all(&root).expect("cleanup");
}
