//! CLI integration tests for the built-in in-process `vm` target (`--target vm`).

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use crate::common::{APP, flick_in, project};

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn flick_no_path(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("PATH", "")
        .output()
        .expect("flick runs without PATH")
}

#[test]
fn run_standalone_corpus_cases_without_node_or_go() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let cases = [
        "tests/run/bootstrap/hello.fl",
        "tests/run/bootstrap/inline_module.fl",
        "tests/run/bootstrap/packed_module.fl",
        "tests/run/effect/host_services.fl",
        "tests/run/sql/queries.fl",
    ];
    for rel in cases {
        let path = repo.join(rel);
        let expected = fs::read_to_string(path.with_extension("stdout"))
            .unwrap_or_else(|e| panic!("read expected stdout for {rel}: {e}"));
        for profile in ["debug", "release"] {
            let out = flick_no_path(&repo, &["run", "--target", "vm", "--profile", profile, rel]);
            assert!(
                out.status.success(),
                "{rel} ({profile}) failed: status={:?}\nstdout={}\nstderr={}",
                out.status,
                stdout(&out),
                stderr(&out)
            );
            assert_eq!(stdout(&out), expected, "{rel} ({profile}) stdout mismatch");
        }
    }
}

#[test]
fn run_project_with_vm_manifest_and_tier_selection() {
    let manifest = "\
schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[build]
target = \"vm\"
environment = \"vm/native\"

[target.vm]
tier = \"0\"
";
    let main = "\
module App.Main exposing (main)

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line \"hello from vm project\"
";
    let dir = project(
        "vm_project_run",
        &[("flick.toml", manifest), ("src/App/Main.fl", main)],
    );

    let default_run = flick_no_path(&dir, &["run"]);
    assert!(
        default_run.status.success(),
        "default vm project run failed:\n{}",
        stderr(&default_run)
    );
    assert_eq!(stdout(&default_run), "hello from vm project\n");

    for tier in ["0", "tier0"] {
        let out = flick_no_path(&dir, &["run", "--target", "vm", "--vm-tier", tier]);
        assert!(
            out.status.success(),
            "--vm-tier {tier} failed:\n{}",
            stderr(&out)
        );
        assert_eq!(stdout(&out), "hello from vm project\n");
    }

    let bad_cli = flick_no_path(&dir, &["run", "--target", "vm", "--vm-tier", "99"]);
    assert_eq!(bad_cli.status.code(), Some(2), "{bad_cli:?}");
    let err = stderr(&bad_cli);
    assert!(
        err.contains("FLICK-P058") && err.contains("99"),
        "expected FLICK-P058 for invalid --vm-tier, got:\n{err}"
    );

    let bad_manifest = "\
schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[build]
target = \"vm\"

[target.vm]
tier = \"nope\"
";
    let bad_dir = project(
        "vm_bad_tier_manifest",
        &[("flick.toml", bad_manifest), ("src/App/Main.fl", main)],
    );
    let bad_toml = flick_no_path(&bad_dir, &["run"]);
    assert_eq!(bad_toml.status.code(), Some(2), "{bad_toml:?}");
    let err = stderr(&bad_toml);
    assert!(
        err.contains("FLICK-P058") && err.contains("flick.toml"),
        "expected FLICK-P058 with manifest span, got:\n{err}"
    );
}

#[test]
fn test_command_runs_passing_failing_filtered_and_json_suites_in_process() {
    let passing = "\
module App.MathTest exposing (tests)

import Test

tests : List Test.Test
let tests = [
  Test.group \"math\" [
    Test.test \"adds\" (fun () -> Test.equal 4 (2 + 2)),
    Test.test \"muls\" (fun () -> Test.equal 9 (3 * 3)),
  ],
]
";
    let dir = project(
        "vm_test_pass",
        &[
            ("flick.toml", APP),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            ("test/App/MathTest.fl", passing),
        ],
    );

    let json_out = flick_no_path(
        &dir,
        &[
            "test",
            "--target",
            "vm",
            "--seed",
            "42",
            "--reporter",
            "json",
        ],
    );
    assert!(
        json_out.status.success(),
        "vm json test failed:\n{}",
        stderr(&json_out)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout(&json_out)).expect("valid JSON test report");
    assert_eq!(parsed["seed"], 42);
    assert_eq!(parsed["passed"], 2);
    assert_eq!(parsed["failed"], 0);
    assert_eq!(parsed["tests"][0]["name"], "App.MathTest > math > adds");
    assert_eq!(parsed["tests"][0]["status"], "passed");
    assert_eq!(parsed["tests"][1]["name"], "App.MathTest > math > muls");
    assert_eq!(parsed["tests"][1]["status"], "passed");

    let filtered = flick_no_path(
        &dir,
        &[
            "test",
            "--target",
            "vm",
            "--filter",
            "adds",
            "--reporter",
            "json",
            "--seed",
            "42",
        ],
    );
    assert!(filtered.status.success(), "{}", stderr(&filtered));
    let filtered_json: serde_json::Value =
        serde_json::from_str(&stdout(&filtered)).expect("filtered JSON");
    assert_eq!(filtered_json["passed"], 1);
    assert_eq!(filtered_json["tests"].as_array().map(Vec::len), Some(1));
}

#[test]
fn test_command_reports_failing_suites_in_json_with_exit_code_1() {
    let failing = "\
module App.FailTest exposing (tests)

import Test

tests : List Test.Test
let tests = [
  Test.group \"math\" [
    Test.test \"ok\" (fun () -> Test.equal 1 1),
    Test.test \"bad\" (fun () -> Test.equal 1 2),
  ],
]
";
    let fail_dir = project(
        "vm_test_fail",
        &[
            ("flick.toml", APP),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            ("test/App/FailTest.fl", failing),
        ],
    );
    let fail_out = flick_no_path(
        &fail_dir,
        &[
            "test",
            "--target",
            "vm",
            "--seed",
            "7",
            "--reporter",
            "json",
        ],
    );
    assert_eq!(fail_out.status.code(), Some(1), "{fail_out:?}");
    let fail_json: serde_json::Value =
        serde_json::from_str(&stdout(&fail_out)).expect("failing JSON report");
    assert_eq!(fail_json["passed"], 1);
    assert_eq!(fail_json["failed"], 1);
    assert_eq!(fail_json["tests"][1]["status"], "failed");
    assert_eq!(fail_json["tests"][1]["kind"], "assertion");
}

#[test]
fn reachable_js_only_use_fails_with_span_while_unused_import_succeeds() {
    let manifest = "\
schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[build]
target = \"vm\"
";
    let unused_main = "\
module App.Main exposing (main)

import Ui

main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line \"ok\"
";
    let unused_dir = project(
        "vm_unused_js_import",
        &[("flick.toml", manifest), ("src/App/Main.fl", unused_main)],
    );
    let unused_run = flick_no_path(&unused_dir, &["run"]);
    assert!(
        unused_run.status.success(),
        "unused JS import must not fail on vm:\n{}",
        stderr(&unused_run)
    );
    assert_eq!(stdout(&unused_run), "ok\n");

    let used_main = "\
module App.Main exposing (main)

import Ui

main : Unit -> Unit
let main () =
  let _ = Ui.safe_url \"https://example.com\"
  ()
";
    let used_dir = project(
        "vm_reachable_js_use",
        &[("flick.toml", manifest), ("src/App/Main.fl", used_main)],
    );
    let used_run = flick_no_path(&used_dir, &["run", "--diagnostics", "json"]);
    assert_eq!(used_run.status.code(), Some(1), "{used_run:?}");
    let raw_err = stderr(&used_run);
    let diag = raw_err
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|v| v["code"] == "FLICK-P064")
        .unwrap_or_else(|| panic!("expected FLICK-P064 in stderr:\n{raw_err}"));
    let start = usize::try_from(diag["range"]["start"].as_u64().expect("span start")).expect("fit");
    let end = usize::try_from(diag["range"]["end"].as_u64().expect("span end")).expect("fit");
    assert_eq!(
        &used_main[start..end],
        "Ui.safe_url",
        "diagnostic span must point at the reachable JS use"
    );
}

#[test]
fn when_vm_module_is_selected_on_vm_and_refused_on_go() {
    let pkg_manifest = "\
schema-version = 1

[package]
name = \"vm-only-pkg\"
version = \"0.1.0\"

[project]
kind = \"lib\"
";
    let vm_mod = "\
@when \"vm\"
module VmOnly.Greeting exposing (message)

message : String
let message = \"from vm-only module\"
";
    let app_manifest = "\
schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[dependencies]
vm-only-pkg = { path = \"pkg\" }
";
    let app_main = "\
module App.Main exposing (main)

import VmOnly.Greeting

main : Unit ->{Host.Console} Unit
let main () = Platform.stdout_line VmOnly.Greeting.message
";
    let dir = project(
        "vm_when_module",
        &[
            ("flick.toml", app_manifest),
            ("src/App/Main.fl", app_main),
            ("pkg/flick.toml", pkg_manifest),
            ("pkg/src/VmOnly/Greeting.fl", vm_mod),
        ],
    );

    let vm_run = flick_no_path(&dir, &["run", "--target", "vm"]);
    assert!(
        vm_run.status.success(),
        "when vm module must run on vm:\n{}",
        stderr(&vm_run)
    );
    assert_eq!(stdout(&vm_run), "from vm-only module\n");

    let go_dir = project(
        "vm_when_module_go",
        &[
            (
                "flick.toml",
                &format!("{app_manifest}\n[build]\ntarget = \"go\"\n"),
            ),
            ("src/App/Main.fl", app_main),
            ("pkg/flick.toml", pkg_manifest),
            ("pkg/src/VmOnly/Greeting.fl", vm_mod),
        ],
    );
    let go_check = flick_in(&go_dir, &["check"]);
    assert_eq!(go_check.status.code(), Some(1), "{go_check:?}");
    let err = stderr(&go_check);
    assert!(
        err.contains("FLICK-P064") && err.contains("VmOnly.Greeting"),
        "expected FLICK-P064 refusing when vm module on go, got:\n{err}"
    );
}

#[test]
fn build_target_vm_refuses_cleanly_and_doctor_reports_vm() {
    let dir = project(
        "vm_build_refusal",
        &[
            ("flick.toml", APP),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );

    let build_out = flick_no_path(&dir, &["build", "--target", "vm"]);
    assert!(!build_out.status.success(), "{build_out:?}");
    let err = stderr(&build_out);
    assert!(
        err.contains("FLICK-P012")
            && err.contains("flick run --target vm")
            && err.contains("flick test --target vm"),
        "expected informative build refusal, got:\n{err}"
    );

    let doctor_out = flick_no_path(&dir, &["doctor", "--target", "vm"]);
    assert!(
        doctor_out.status.success(),
        "doctor --target vm failed:\n{}",
        stderr(&doctor_out)
    );
    assert!(
        stdout(&doctor_out).contains("flick/vm"),
        "expected flick/vm in doctor output:\n{}",
        stdout(&doctor_out)
    );
}
