//! Additional end-to-end CLI integration tests for Phase C3 contract
//! verification: cache bypass on helper edits, non-lowering in `flick check`/
//! `flick build`/`flick test`, cross-module contract calls, soundness guards,
//! and solver budget exhaustion.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::common::{flick_in, project};
use crate::prove_contracts::{CONSUME_BAD, CONSUME_OK, LIB, json_diagnostics, parse_int_binding};

#[test]
fn cache_bypass_reproves_after_check_and_after_helper_body_edits_and_unfolds_int_helpers() {
    let helper_ok = r"module App.Math exposing (clamp_nonneg, bump)

clamp_nonneg : Int -> Int
let clamp_nonneg x = Int.max 0 x

bump : Int -> Int
let bump x = x + 1
";
    let helper_bad = helper_ok.replace("x + 1", "x");
    let main = r"module App.Use exposing (safe_bump, clamped)

import App.Math exposing (bump, clamp_nonneg)

safe_bump : Int -> Int
let safe_bump x
where
  require (x < Int.max_value)
  ensure (fun y -> y > x)
=
  bump x

clamped : Int -> Int
let clamped x
where
  ensure (fun y -> 0 <= y && y >= x)
=
  clamp_nonneg x
";

    let dir = project(
        "prove_contracts_cache_bypass",
        &[
            ("flick.toml", LIB),
            ("src/App/Math.fl", helper_ok),
            ("src/App/Use.fl", main),
        ],
    );

    // 1. Warm the persistent module store with `flick check` first.
    let checked = flick_in(&dir, &["check"]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );

    // 2. `flick prove` must bypass the check-only cache and prove using Int.max / Int.max_value + App.Math.
    let proved = flick_in(&dir, &["prove"]);
    assert_eq!(
        proved.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&proved.stderr)
    );

    // 3. Edit the helper body WITHOUT changing its signature; `flick check` stays clean,
    //    but `flick prove` MUST detect the broken body and report FLICK-T087!
    fs::write(dir.join("src/App/Math.fl"), &helper_bad).expect("break helper body");
    assert_eq!(flick_in(&dir, &["check"]).status.code(), Some(0));

    let broken = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(
        broken.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&broken.stderr)
    );
    let diags = json_diagnostics(&broken.stderr);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "FLICK-T087");

    // 4. Restore helper body -> `flick prove` passes again.
    fs::write(dir.join("src/App/Math.fl"), helper_ok).expect("restore helper body");
    let restored = flick_in(&dir, &["prove"]);
    assert_eq!(
        restored.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
}

#[test]
fn check_and_build_succeed_on_false_contract_without_lowering_it() {
    let dir = project(
        "prove_contracts_not_lowered",
        &[("flick.toml", LIB), ("src/App/Inventory.fl", CONSUME_BAD)],
    );

    let checked = flick_in(&dir, &["check"]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );

    let built = flick_in(&dir, &["build", "--emit-ir=json"]);
    assert_eq!(
        built.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    // Confirm the emitted IR for `App.Inventory` contains only the `available`
    // body and no contract check.
    let ir_json: Value = serde_json::from_str(
        &fs::read_to_string(dir.join("dist/program.ir.json")).expect("program.ir.json"),
    )
    .expect("valid program.ir.json");
    let inventory = ir_json["modules"]
        .as_array()
        .expect("modules array")
        .iter()
        .find(|m| m["name"] == "App.Inventory")
        .expect("App.Inventory module in IR");
    let inventory_text = inventory.to_string();
    assert!(
        inventory_text.contains("\"consume\""),
        "missing consume in IR"
    );
    assert!(
        !inventory_text.contains("remaining"),
        "contract ensure binder must never be lowered into runtime IR: {inventory_text}"
    );

    // Whereas `flick prove` catches the false contract!
    let proved = flick_in(&dir, &["prove"]);
    assert_eq!(proved.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&proved.stderr).contains("FLICK-T087"));
}

#[test]
fn cross_module_contract_calls_check_precondition_and_never_unfold_unverified_callees() {
    let good_caller = r"module App.Caller exposing (take)

import App.Inventory exposing (consume)

take : Int -> Int -> Int
let take available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  consume available used
";
    let loose_caller = r"module App.Caller exposing (take)

import App.Inventory exposing (consume)

take : Int -> Int -> Int
let take available used
where
  require (0 <= used && 0 <= available)
  ensure (fun _ -> true)
=
  consume available used
";

    let dir = project(
        "prove_contracts_cross_module",
        &[
            ("flick.toml", LIB),
            ("src/App/Inventory.fl", CONSUME_OK),
            ("src/App/Caller.fl", good_caller),
        ],
    );

    // 1. Both modules proved in one run -> Caller certifies using Inventory.consume's summary.
    let ok = flick_in(&dir, &["prove"]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&ok.stdout),
        String::from_utf8_lossy(&ok.stderr)
    );

    // 2. Narrowing to ONLY Caller.fl leaves Inventory.consume unverified in this run ->
    //    must refuse with FLICK-T082 instead of silently unfolding consume's body!
    let narrowed = flick_in(
        &dir,
        &["--diagnostics", "json", "prove", "src/App/Caller.fl"],
    );
    assert_eq!(narrowed.status.code(), Some(1));
    let diags = json_diagnostics(&narrowed.stderr);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "FLICK-T082");

    // 3. Looser guard in Caller calling verified Inventory.consume -> FLICK-T088 with counterexample!
    fs::write(dir.join("src/App/Caller.fl"), loose_caller).expect("write loose caller");
    let bad = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(bad.status.code(), Some(1));
    let diags = json_diagnostics(&bad.stderr);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "FLICK-T088");
    let msg = diags[0]["message"].as_str().expect("message");
    let available = parse_int_binding(msg, "available").expect("available binding");
    let used = parse_int_binding(msg, "used").expect("used binding");
    assert!(
        0 <= used && 0 <= available && used > available,
        "counterexample must violate consume's require: {msg}"
    );

    // 4. Failed callee contract in Inventory -> Caller refuses with FLICK-T082 (never unfolds).
    fs::write(dir.join("src/App/Inventory.fl"), CONSUME_BAD).expect("break inventory");
    fs::write(dir.join("src/App/Caller.fl"), good_caller).expect("restore good caller");
    let broken_dep = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(broken_dep.status.code(), Some(1));
    let diags = json_diagnostics(&broken_dep.stderr);
    let codes: Vec<&str> = diags.iter().filter_map(|d| d["code"].as_str()).collect();
    assert!(
        codes.contains(&"FLICK-T087"),
        "expected T087 for Inventory: {diags:?}"
    );
    assert!(
        codes.contains(&"FLICK-T082"),
        "expected T082 for Caller: {diags:?}"
    );
}

#[test]
fn foreign_builtin_helper_refusal_does_not_panic_and_reports_t082() {
    let source = r"module App.BuiltinRefusal exposing (keep_pos)

keep_pos : Option Int -> Option Int
let keep_pos opt
where
  ensure (fun _ -> true)
=
  Option.filter (fun x -> x > 0) opt
";
    let dir = project(
        "prove_contracts_foreign_span",
        &[("flick.toml", LIB), ("src/App/BuiltinRefusal.fl", source)],
    );

    let out = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "must fail cleanly without panic; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let diags = json_diagnostics(&out.stderr);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "FLICK-T082");
}

#[test]
fn closures_higher_order_and_overflowing_spec_helpers_are_refused_with_t082() {
    let source = r"module App.SoundnessGuards exposing (consume, leak_pre, leak_ovf, pass_ho, exploit)

consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used

leak_pre : Int -> (Int -> Int)
let leak_pre a
where
  ensure (fun _ -> true)
=
  fun u -> consume a u

leak_ovf : Int -> (Int -> Int)
let leak_ovf x
where
  ensure (fun _ -> true)
=
  fun y -> x + y

pass_ho : ((Int -> Int) -> Int) -> Int -> Int
let pass_ho k a
where
  ensure (fun r -> r == 0)
=
  let _ = k (consume a)
  0

overflows : Int -> Int
let overflows x =
  if x + 1 > x then 1 else 0

exploit : Int -> Int
let exploit x
where
  require (overflows x == 0)
  ensure (fun r -> r == 999)
=
  0
";
    let dir = project(
        "prove_contracts_soundness_guards",
        &[("flick.toml", LIB), ("src/App/SoundnessGuards.fl", source)],
    );

    let out = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(out.status.code(), Some(1));
    let diags: Vec<Value> = json_diagnostics(&out.stderr)
        .into_iter()
        .filter(|d| d["severity"] == "error")
        .collect();
    assert_eq!(diags.len(), 4, "expected 4 T082 refusals, got {diags:?}");
    for d in &diags {
        assert_eq!(d["code"], "FLICK-T082", "{d:?}");
    }
}

#[test]
fn budget_exhaustion_reports_t084_end_to_end() {
    let source = r"module App.Budget exposing (many_branches)

many_branches : Bool -> Bool -> Bool -> Bool -> Bool -> Bool -> Bool -> Bool -> Bool -> Bool -> Bool -> Int
let many_branches b0 b1 b2 b3 b4 b5 b6 b7 b8 b9 b10
where
  ensure (fun r -> r >= 0)
=
  let s0 = if b0 then 1 else 0
  let s1 = if b1 then s0 + 1 else s0
  let s2 = if b2 then s1 + 1 else s1
  let s3 = if b3 then s2 + 1 else s2
  let s4 = if b4 then s3 + 1 else s3
  let s5 = if b5 then s4 + 1 else s4
  let s6 = if b6 then s5 + 1 else s5
  let s7 = if b7 then s6 + 1 else s6
  let s8 = if b8 then s7 + 1 else s7
  let s9 = if b9 then s8 + 1 else s8
  if b10 then s9 + 1 else s9
";
    let dir = project(
        "prove_contracts_budget",
        &[("flick.toml", LIB), ("src/App/Budget.fl", source)],
    );

    let out = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let diags: Vec<Value> = json_diagnostics(&out.stderr)
        .into_iter()
        .filter(|d| d["severity"] == "error")
        .collect();
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "FLICK-T084");
}

#[test]
#[ignore = "needs node"]
fn flick_test_succeeds_on_false_contract_outside_prove() {
    let suite = r#"module Test.Inventory exposing (tests)

import App.Inventory exposing (consume)

tests : List Test.Test
let tests =
  [Test.test "false contract does not affect runtime" (fun () -> Test.equal 10 (consume 10 3))]
"#;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/tmp/prove_contracts_test_cmd");
    let _ = fs::remove_dir_all(&dir);
    for (file, contents) in [
        ("flick.toml", LIB),
        ("src/App/Inventory.fl", CONSUME_BAD),
        ("tests/Test/Inventory.fl", suite),
    ] {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(path, contents).expect("write");
    }

    let tested = flick_in(&dir, &["test", "--rt"]);
    assert_eq!(
        tested.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&tested.stdout),
        String::from_utf8_lossy(&tested.stderr)
    );
}
