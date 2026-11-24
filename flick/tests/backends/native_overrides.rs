//! End-to-end tests for `@native "<binding>" when <cond>` overrides (#666):
//! JS + Go execution, fixture-backend fallback body execution, implication
//! selection & ambiguity diagnostics naming both spans, release-optimizer
//! opacity, and conformance runs with overrides enabled and disabled.

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

fn target_config(target: &str) -> String {
    if target == "go" {
        format!("{CONFIG}\n[build]\ntarget = \"go\"\n\n[tools]\n\"flick/go\" = [\"go\"]\n")
    } else {
        format!("{CONFIG}\n[build]\ntarget = \"{target}\"\n")
    }
}

fn diag_line_and_slice<'a>(source: &'a str, range: &Value) -> (usize, &'a str) {
    let start = usize::try_from(range["start"].as_u64().expect("start")).expect("start fits");
    let end = usize::try_from(range["end"].as_u64().expect("end")).expect("end fits");
    let line = source[..start].bytes().filter(|&b| b == b'\n').count() + 1;
    (line, &source[start..end])
}

#[test]
#[ignore = "requires just wasm-backends"]
fn native_override_runs_js_go_and_fixture_fallback_with_same_output() {
    let source = "\
module App exposing (hypot, main)

import Float
import Platform

hypot : Float -> Float -> Float
let hypot a b = Float.sqrt (a * a + b * b)
  @native \"Math.hypot\" when js
  @native \"math.Hypot\" when go

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (hypot 3.0 4.0))
  Platform.stdout_line (show (hypot 5.0 12.0))
  Platform.stdout_line (show (hypot 7.0 24.0))
";
    let expected = "5\n13\n25\n";

    // 1. JS target runs `Math.hypot`.
    let js_root = project(
        "native_ov_js",
        &[("flick.toml", CONFIG), ("src/App.fl", source)],
    );
    let js_build = flick_in(&js_root, &["build", "--target", "js", "--offline"]);
    assert!(
        js_build.status.success(),
        "js build failed: {}",
        String::from_utf8_lossy(&js_build.stderr)
    );
    let emitted_js =
        fs::read_to_string(js_root.join("dist/_flick/App.js")).expect("emitted App.js");
    assert!(
        emitted_js.contains("Math.hypot"),
        "expected JS override Math.hypot in:\n{emitted_js}"
    );
    let js_run = flick_in(&js_root, &["run", "--target", "js", "--offline"]);
    assert!(
        js_run.status.success(),
        "js run failed: {}",
        String::from_utf8_lossy(&js_run.stderr)
    );
    assert_eq!(String::from_utf8(js_run.stdout).expect("UTF-8"), expected);
    fs::remove_dir_all(&js_root).expect("cleanup js");

    // 2. Go target runs `math.Hypot`.
    let go_config = target_config("go");
    let go_root = project(
        "native_ov_go",
        &[("flick.toml", &go_config), ("src/App.fl", source)],
    );
    let go_build = flick_in(&go_root, &["build", "--offline"]);
    assert!(
        go_build.status.success(),
        "go build failed: {}",
        String::from_utf8_lossy(&go_build.stderr)
    );
    let emitted_go: String = fs::read_dir(go_root.join("dist/go/main"))
        .expect("dist/go/main")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("go"))
        .map(|path| fs::read_to_string(path).expect("read emitted go"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        emitted_go.contains("math.Hypot"),
        "expected Go override math.Hypot in:\n{emitted_go}"
    );
    let go_run = flick_in(&go_root, &["run", "--offline"]);
    assert!(
        go_run.status.success(),
        "go run failed: {}",
        String::from_utf8_lossy(&go_run.stderr)
    );
    assert_eq!(String::from_utf8(go_run.stdout).expect("UTF-8"), expected);
    fs::remove_dir_all(&go_root).expect("cleanup go");

    // 3. Fixture backend `test/literal` has no override and executes the Flick fallback body.
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm");
    let lit_config = format!(
        "{}\n[target.literal]\ncase = \"fallback\"\n",
        pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes))
    );
    let lit_root = project(
        "native_ov_literal_fallback",
        &[("flick.toml", &lit_config), ("src/App.fl", source)],
    );
    fs::write(lit_root.join("backend.wasm"), &bytes).expect("write backend.wasm");
    let lit_run = flick_in(&lit_root, &["run", "--offline"]);
    assert!(
        lit_run.status.success(),
        "literal fallback run failed: {}",
        String::from_utf8_lossy(&lit_run.stderr)
    );
    assert_eq!(String::from_utf8(lit_run.stdout).expect("UTF-8"), expected);
    fs::remove_dir_all(&lit_root).expect("cleanup literal");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn native_override_selects_by_implication_and_reports_ambiguous_pair_with_both_spans() {
    // 1. Implication: `bun` implies `js`, so `[build] environment = "js/bun"` selects `bun` over `js`.
    let bun_source = "\
module App exposing (pick, main)

pick : Int -> Int
let pick x = x
  @native inline \"(x) => x + 10\" when js
  @native inline \"(x) => x + 20\" when bun

main : Unit -> Unit
let main () = ignore (pick 1)
";
    let bun_config = format!("{CONFIG}\n[build]\nenvironment = \"js/bun\"\n");
    let bun_root = project(
        "native_ov_bun_implies_js",
        &[("flick.toml", &bun_config), ("src/App.fl", bun_source)],
    );
    let built = flick_in(&bun_root, &["build", "--offline"]);
    assert!(
        built.status.success(),
        "bun implication build failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let emitted = fs::read_to_string(bun_root.join("dist/_flick/App.js")).expect("App.js");
    assert!(
        emitted.contains("x + 20") && !emitted.contains("x + 10"),
        "expected `bun` override to win over `js` in:\n{emitted}"
    );
    fs::remove_dir_all(&bun_root).expect("cleanup bun");

    // 2. Ambiguous pair reports `FLICK-F012` naming BOTH lines/spans.
    let ambig_source = "\
module App exposing (hypot, main)
// π keeps UTF-8 offsets honest
import Float

hypot : Float -> Float -> Float
let hypot a b = Float.sqrt (a * a + b * b)
  @native \"Math.hypot\" when js && node
  @native \"Math.hypot\" when js && rt

main : Unit -> Unit
let main () = ()
";
    let ambig_root = project(
        "native_ov_ambig",
        &[("flick.toml", CONFIG), ("src/App.fl", ambig_source)],
    );
    for cmd in ["check", "build"] {
        let out = flick_in(&ambig_root, &[cmd, "--offline", "--diagnostics", "json"]);
        assert_eq!(out.status.code(), Some(1), "{cmd}: {out:?}");
        let diags = parse_json_diags(&out.stderr);
        let diag = diags
            .iter()
            .find(|d| d["code"] == "FLICK-F012")
            .unwrap_or_else(|| panic!("{cmd}: expected FLICK-F012 in {diags:?}"));
        let (line_a, slice_a) = diag_line_and_slice(ambig_source, &diag["range"]);
        assert_eq!(line_a, 7, "{cmd}: {diag}");
        assert_eq!(slice_a, "@native \"Math.hypot\" when js && node");
        let related = diag["related"].as_array().expect("related spans");
        assert!(
            !related.is_empty(),
            "{cmd}: expected related span in {diag}"
        );
        let (line_b, slice_b) = diag_line_and_slice(ambig_source, &related[0]["range"]);
        assert_eq!(line_b, 8, "{cmd}: {diag}");
        assert_eq!(slice_b, "@native \"Math.hypot\" when js && rt");
    }
    fs::remove_dir_all(&ambig_root).expect("cleanup ambig");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn release_optimizer_treats_overridden_function_as_opaque_on_js_and_go() {
    // Sentinel overrides (purely to prove release optimizer never inlines or constant-folds
    // away the override) plus `--no-native-overrides` verifying cache invalidation & fallback.
    let source = "\
module App exposing (bump, main)

import Platform

bump : Int -> Int
let bump x = x + 1
  @native inline \"(x) => (x + 100) | 0\" when js
  @native inline \"func(fb *flickrt.Fiber, x flickrt.Value) flickrt.Value { return x.(int32) + 200 }\" when go

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (bump 5))
";
    for (target, expected_on, expected_off) in [("js", "105\n", "6\n"), ("go", "205\n", "6\n")] {
        let cfg = target_config(target);
        let root = project(
            &format!("native_ov_opaque_{target}"),
            &[("flick.toml", &cfg), ("src/App.fl", source)],
        );
        let on = flick_in(&root, &["run", "--profile", "release", "--offline"]);
        assert!(
            on.status.success(),
            "{target} release (on) failed: {}",
            String::from_utf8_lossy(&on.stderr)
        );
        assert_eq!(
            String::from_utf8(on.stdout).expect("UTF-8"),
            expected_on,
            "{target} release must keep override opaque"
        );
        // Running immediately after with `--no-native-overrides` must NOT hit the stale cache.
        let off = flick_in(
            &root,
            &[
                "run",
                "--profile",
                "release",
                "--no-native-overrides",
                "--offline",
            ],
        );
        assert!(
            off.status.success(),
            "{target} release (off) failed: {}",
            String::from_utf8_lossy(&off.stderr)
        );
        assert_eq!(
            String::from_utf8(off.stdout).expect("UTF-8"),
            expected_off,
            "{target} release --no-native-overrides must run fallback body"
        );
        fs::remove_dir_all(&root).expect("cleanup");
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn native_override_conformance_corpus_matches_expected_outputs() {
    let source = "\
module App exposing (hypot, abs_f, starts_with, clamp01, main)

import Float
import Platform
import String

hypot : Float -> Float -> Float
let hypot a b = Float.sqrt (a * a + b * b)
  @native \"Math.hypot\" when js
  @native \"math.Hypot\" when go

abs_f : Float -> Float
let abs_f x = if x < 0.0 then -x else x
  @native \"Math.abs\" when js
  @native \"math.Abs\" when go

starts_with : String -> String -> Bool
let starts_with s prefix = String.starts_with prefix s
  @native inline \"(s, prefix) => s.startsWith(prefix)\" when js
  @native \"strings.HasPrefix\" when go

clamp01 : Float -> Float
let clamp01 x =
  if x < 0.0 then 0.0
  else if x > 1.0 then 1.0
  else x
  @native inline \"(x) => (x < 0 ? 0 : x > 1 ? 1 : x)\" when js
  @native inline \"\"\"
    func(fb *flickrt.Fiber, x flickrt.Value) flickrt.Value {
      v := x.(float64)
      if v < 0 {
        return float64(0)
      }
      if v > 1 {
        return float64(1)
      }
      return v
    }
    \"\"\" when go

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (hypot 3.0 4.0))
  Platform.stdout_line (show (hypot 8.0 15.0))
  Platform.stdout_line (show (abs_f (-42.5)))
  Platform.stdout_line (show (abs_f 12.25))
  Platform.stdout_line (show (starts_with \"flick-lang\" \"flick\"))
  Platform.stdout_line (show (starts_with \"flick-lang\" \"rust\"))
  Platform.stdout_line (show (clamp01 (-0.5)))
  Platform.stdout_line (show (clamp01 0.25))
  Platform.stdout_line (show (clamp01 2.5))
";
    // Independent expected output for every corpus case.
    let expected = "5\n17\n42.5\n12.25\ntrue\nfalse\n0\n0.25\n1\n";

    for target in ["js", "go"] {
        let cfg = target_config(target);
        let root = project(
            &format!("native_ov_conformance_{target}"),
            &[("flick.toml", &cfg), ("src/App.fl", source)],
        );
        for profile in ["debug", "release"] {
            for disable_overrides in [false, true] {
                let mut args = vec!["run", "--profile", profile, "--offline"];
                if disable_overrides {
                    args.push("--no-native-overrides");
                }
                let out = flick_in(&root, &args);
                assert!(
                    out.status.success(),
                    "conformance failed ({target}/{profile}/disable={disable_overrides}): {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert_eq!(
                    String::from_utf8(out.stdout).expect("UTF-8"),
                    expected,
                    "conformance mismatch ({target}/{profile}/disable={disable_overrides})"
                );
            }
        }
        fs::remove_dir_all(&root).expect("cleanup");
    }
}
