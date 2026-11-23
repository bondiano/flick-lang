//! End-to-end CLI integration tests for Phase C3 contract verification:
//! `flick prove`, `flick prove <path>`, `--diagnostics json`, diagnostics
//! `FLICK-T087`–`FLICK-T089` and `FLICK-T082`, cache bypass on body/helper
//! edits, and isolation from `flick check`, `flick build`, `flick run`, and
//! `flick test`.

use std::fs;

use serde_json::Value;

use crate::common;

use common::{flick_in, project};

pub(super) const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

pub(super) const CONSUME_OK: &str = r"module App.Inventory exposing (consume)

consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used
";

pub(super) const CONSUME_BAD: &str = r"module App.Inventory exposing (consume)

consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available
";

/// Parses NDJSON `diagnostic` events emitted by `--diagnostics json` on stderr.
pub(super) fn json_diagnostics(stderr: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(stderr)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event.get("type").and_then(Value::as_str) == Some("diagnostic"))
        .collect()
}

/// Extracts `name = value` integer bindings from a counterexample message suffix.
pub(super) fn parse_int_binding(message: &str, name: &str) -> Option<i64> {
    let needle = format!("{name} = ");
    let start = message.find(&needle)? + needle.len();
    let rest = &message[start..];
    let len = rest
        .bytes()
        .take_while(|b| *b == b'-' || b.is_ascii_digit())
        .count();
    rest[..len].parse::<i64>().ok()
}

/// Returns the source slice covered by a JSON diagnostic's byte `range`.
fn diag_slice<'a>(source: &'a str, diag: &Value) -> &'a str {
    let range = diag.get("range").expect("diagnostic has range");
    let start = usize::try_from(
        range
            .get("start")
            .and_then(Value::as_u64)
            .expect("range.start"),
    )
    .expect("start fits usize");
    let end = usize::try_from(range.get("end").and_then(Value::as_u64).expect("range.end"))
        .expect("end fits usize");
    &source[start..end]
}

#[test]
fn consume_certifies_and_buggy_consume_reports_t087_with_verified_counterexample() {
    let dir = project(
        "prove_contracts_consume",
        &[("flick.toml", LIB), ("src/App/Inventory.fl", CONSUME_OK)],
    );

    let ok = flick_in(&dir, &["prove"]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&ok.stdout),
        String::from_utf8_lossy(&ok.stderr)
    );

    fs::write(dir.join("src/App/Inventory.fl"), CONSUME_BAD).expect("write buggy consume");
    let bad = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(
        bad.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&bad.stderr)
    );

    let diags = json_diagnostics(&bad.stderr);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
    let d = &diags[0];
    assert_eq!(d["code"], "FLICK-T087");
    let msg = d["message"].as_str().expect("message");
    assert!(msg.contains("e.g. when"), "{msg}");
    let available = parse_int_binding(msg, "available").expect("available binding");
    let used = parse_int_binding(msg, "used").expect("used binding");
    // Counterexample must actually satisfy `require` and falsify `ensure`!
    assert!(
        0 <= used && used <= available,
        "counterexample must satisfy require: {msg}"
    );
    assert!(
        !(0 <= available && available + used == available),
        "counterexample must falsify ensure for body `available`: {msg}"
    );
}

#[test]
fn int_overflow_and_unary_negation_report_t089_at_operator_and_certify_when_guarded() {
    let unguarded_add = r"module App.Arith exposing (next_ok)

next_ok : Int -> Bool
let next_ok x
where
  ensure (fun ok -> ok)
=
  x + 1 > x
";
    let guarded_add = r"module App.Arith exposing (next_ok)

next_ok : Int -> Bool
let next_ok x
where
  require (x < 2147483647)
  ensure (fun ok -> ok)
=
  x + 1 > x
";
    let unguarded_neg = r"module App.Arith exposing (flip)

flip : Int -> Int
let flip x
where
  ensure (fun y -> y + x == 0)
=
  -x
";
    let guarded_neg = r"module App.Arith exposing (flip)

flip : Int -> Int
let flip x
where
  require (x > -2147483648)
  ensure (fun y -> y + x == 0)
=
  -x
";

    let dir = project(
        "prove_contracts_overflow",
        &[("flick.toml", LIB), ("src/App/Arith.fl", unguarded_add)],
    );

    // Unguarded `x + 1 > x` -> FLICK-T089 pointing at `x + 1`
    let bad_add = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(bad_add.status.code(), Some(1));
    let diags = json_diagnostics(&bad_add.stderr);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "FLICK-T089");
    let slice = diag_slice(unguarded_add, &diags[0]);
    assert_eq!(slice, "x + 1", "expected span at `+` expr, got `{slice}`");
    let msg = diags[0]["message"].as_str().expect("message");
    assert_eq!(parse_int_binding(msg, "x"), Some(2_147_483_647), "{msg}");

    // Guarded `x < 2147483647` -> certified
    fs::write(dir.join("src/App/Arith.fl"), guarded_add).expect("write guarded add");
    let ok_add = flick_in(&dir, &["prove"]);
    assert_eq!(
        ok_add.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok_add.stderr)
    );

    // Unguarded unary `-x` -> FLICK-T089 pointing at `-x`
    fs::write(dir.join("src/App/Arith.fl"), unguarded_neg).expect("write unguarded neg");
    let bad_neg = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(bad_neg.status.code(), Some(1));
    let diags = json_diagnostics(&bad_neg.stderr);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "FLICK-T089");
    let slice = diag_slice(unguarded_neg, &diags[0]);
    assert_eq!(
        slice, "-x",
        "expected span at unary `-` expr, got `{slice}`"
    );
    let msg = diags[0]["message"].as_str().expect("message");
    assert_eq!(parse_int_binding(msg, "x"), Some(-2_147_483_648), "{msg}");

    // Guarded `x > -2147483648` -> certified
    fs::write(dir.join("src/App/Arith.fl"), guarded_neg).expect("write guarded neg");
    let ok_neg = flick_in(&dir, &["prove"]);
    assert_eq!(
        ok_neg.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok_neg.stderr)
    );
}

#[test]
fn caller_precondition_certifies_and_looser_guard_reports_t088_with_counterexample() {
    let strong_caller = r"module App.Caller exposing (consume, take)

consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used

take : Int -> Int -> Int
let take available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining)
=
  consume available used
";
    let loose_caller = strong_caller.replace(
        "  require (0 <= used && used <= available)\n  ensure (fun remaining -> 0 <= remaining)",
        "  require (0 <= used)\n  ensure (fun remaining -> 0 <= remaining)",
    );

    let dir = project(
        "prove_contracts_caller",
        &[("flick.toml", LIB), ("src/App/Caller.fl", strong_caller)],
    );
    let ok = flick_in(&dir, &["prove"]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    fs::write(dir.join("src/App/Caller.fl"), &loose_caller).expect("write loose caller");
    let bad = flick_in(&dir, &["--diagnostics", "json", "prove"]);
    assert_eq!(bad.status.code(), Some(1));
    let diags = json_diagnostics(&bad.stderr);
    assert_eq!(diags.len(), 1, "{diags:?}");
    let d = &diags[0];
    assert_eq!(d["code"], "FLICK-T088");
    let msg = d["message"].as_str().expect("message");
    assert!(msg.contains("`consume`"), "{msg}");
    let available = parse_int_binding(msg, "available").expect("available binding");
    let used = parse_int_binding(msg, "used").expect("used binding");
    assert!(
        0 <= used && used > available,
        "counterexample must satisfy caller require and violate callee require: {msg}"
    );
    let related = d["related"].as_array().expect("related array");
    assert!(
        !related.is_empty(),
        "T088 should link to callee require: {d:?}"
    );
}

#[test]
fn unsupported_ops_and_cyclic_contracts_report_t082_and_are_never_assumed() {
    let unsupported = r"module App.Unsupported exposing (mul_bad, div_bad, rec_bad, eff_bad)

mul_bad : Int -> Int -> Int
let mul_bad a b
where
  ensure (fun r -> r == r)
=
  a * b

div_bad : Int -> Int -> Int
let div_bad a b
where
  require (b != 0)
  ensure (fun r -> r == r)
=
  a / b

rec_bad : Int -> Int
let rec rec_bad n
where
  require (0 <= n)
  ensure (fun r -> 0 <= r)
=
  if n == 0 then 0 else rec_bad (n - 1)

eff_bad : Int ->{Host.Console} Int
let eff_bad x
where
  ensure (fun r -> r == x)
=
  x
";
    let cyclic = r"module App.Cyclic exposing (even_ish, odd_ish, caller)

rec {
  even_ish : Int -> Int
  let even_ish n
  where
    ensure (fun r -> r > 0)
  =
    if n == 0 then 1 else odd_ish (n - 1)

  odd_ish : Int -> Int
  let odd_ish n
  where
    ensure (fun r -> r > 0)
  =
    if n == 0 then 1 else even_ish (n - 1)
}

caller : Int -> Int
let caller n
where
  ensure (fun r -> r > 0)
=
  even_ish n
";

    let dir = project(
        "prove_contracts_unsupported",
        &[
            ("flick.toml", LIB),
            ("src/App/Unsupported.fl", unsupported),
            ("src/App/Cyclic.fl", cyclic),
        ],
    );

    let out = flick_in(
        &dir,
        &["--diagnostics", "json", "prove", "src/App/Unsupported.fl"],
    );
    assert_eq!(out.status.code(), Some(1));
    let diags = json_diagnostics(&out.stderr);
    assert_eq!(diags.len(), 4, "{diags:?}");
    for d in &diags {
        assert_eq!(d["code"], "FLICK-T082", "{d:?}");
    }

    let cyc = flick_in(
        &dir,
        &["--diagnostics", "json", "prove", "src/App/Cyclic.fl"],
    );
    assert_eq!(cyc.status.code(), Some(1));
    let diags = json_diagnostics(&cyc.stderr);
    // `even_ish` (cycle), `odd_ish` (cycle), and `caller` (cannot assume `even_ish`)
    assert_eq!(diags.len(), 3, "{diags:?}");
    for d in &diags {
        assert_eq!(d["code"], "FLICK-T082", "{d:?}");
    }
    let messages: Vec<&str> = diags.iter().filter_map(|d| d["message"].as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("`even_ish`'s contract did not check")),
        "caller must refuse to assume cyclic contract: {messages:?}"
    );
}

#[test]
fn prove_path_narrows_selection_even_with_failing_sibling_or_import() {
    let bad = r"module App.Bad exposing (bad_fn, helper)

helper : Int -> Int
let helper x = x

bad_fn : Int -> Int
let bad_fn x
where
  ensure (fun r -> r > x)
=
  x
";
    let good = r"module App.Good exposing (good_fn)

import App.Bad exposing (helper)

good_fn : Int -> Int
let good_fn x
where
  require (x < 2147483647)
  ensure (fun r -> r > x)
=
  helper x + 1
";

    let dir = project(
        "prove_contracts_narrowing",
        &[
            ("flick.toml", LIB),
            ("src/App/Bad.fl", bad),
            ("src/App/Good.fl", good),
        ],
    );

    // Proving only Good.fl succeeds even though Bad.fl (which Good imports!) has a failing contract.
    let only_good = flick_in(&dir, &["prove", "src/App/Good.fl"]);
    assert_eq!(
        only_good.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&only_good.stderr)
    );

    // Proving only Bad.fl fails with T087.
    let only_bad = flick_in(&dir, &["--diagnostics", "json", "prove", "src/App/Bad.fl"]);
    assert_eq!(only_bad.status.code(), Some(1));
    let diags = json_diagnostics(&only_bad.stderr);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0]["code"], "FLICK-T087");

    // Proving the whole project fails because Bad.fl is selected.
    let whole = flick_in(&dir, &["prove"]);
    assert_eq!(whole.status.code(), Some(1));
}

#[test]
fn mixed_project_with_proofs_and_contracts_verifies_both_and_check_ignores_both() {
    let proof_ok = "module App.Laws exposing (..)\n\nproof right_identity (v : Option a)\nwhere equal (flat_map pure v) v =\n  match v with\n  | None -> refl\n  | Some x -> refl\n";
    let dir = project(
        "prove_contracts_mixed",
        &[
            ("flick.toml", LIB),
            ("src/App/Inventory.fl", CONSUME_OK),
            ("proofs/App/Laws.fl", proof_ok),
        ],
    );

    let ok = flick_in(&dir, &["prove"]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    // Break the contract while proof stays valid -> `flick prove` fails with T087
    fs::write(dir.join("src/App/Inventory.fl"), CONSUME_BAD).expect("break contract");
    let broken_contract = flick_in(&dir, &["prove"]);
    let stderr = String::from_utf8_lossy(&broken_contract.stderr);
    assert_eq!(broken_contract.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("FLICK-T087"), "{stderr}");

    // `flick check` still succeeds!
    let checked = flick_in(&dir, &["check"]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );

    // Restore contract, break proof -> `flick prove` fails with T081
    fs::write(dir.join("src/App/Inventory.fl"), CONSUME_OK).expect("restore contract");
    let proof_bad = proof_ok.replace("equal (flat_map pure v) v", "equal (flat_map pure v) None");
    fs::write(dir.join("proofs/App/Laws.fl"), proof_bad).expect("break proof");
    let broken_proof = flick_in(&dir, &["prove"]);
    let stderr = String::from_utf8_lossy(&broken_proof.stderr);
    assert_eq!(broken_proof.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("FLICK-T08"), "{stderr}");
}
