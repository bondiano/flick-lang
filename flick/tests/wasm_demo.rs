mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use common::flick_in;
use flick_core::build::runner_path;
use serde_json::Value;

const MODULE: &str = "module Main exposing (total, main)\n\nimport List\n\ntotal : List Number -> Number\nlet total xs = List.fold_left (fun acc x -> acc + x) 0 xs\n\nmain : Unit ->{none; AppError} Unit\nlet main () =\n  Platform.stdout_line (Number.to_string (total [1, 2, 3]))\n";

#[test]
fn the_demo_compiles_a_module_as_flick_build_does() {
    for profile in ["debug", "release"] {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("wasm_demo_{profile}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("Main.fl"), MODULE).expect("write");
        let out = flick_in(&dir, &["build", "Main.fl", "--profile", profile]);
        assert!(out.status.success(), "{out:?}");
        let built = fs::read_to_string(dir.join("dist/Main.js")).expect("module");
        let demo: Value =
            serde_json::from_str(&flick_wasm::compile(MODULE, profile)).expect("json");
        assert_eq!(demo["diagnostics"], Value::Array(Vec::new()));
        assert_eq!(demo["js"].as_str(), Some(built.as_str()), "{profile}");
    }
}

#[test]
fn app_artifacts_do_not_collide_on_case_insensitive_filesystems() {
    for profile in ["debug", "release"] {
        let built: Value = serde_json::from_str(&flick_wasm::build(MODULE, profile)).expect("json");
        let mut paths = BTreeSet::new();
        for path in built["files"].as_object().expect("files").keys() {
            assert!(
                paths.insert(path.to_lowercase()),
                "{profile}: case-insensitive artifact collision at `{path}`"
            );
        }
    }
}

#[test]
fn the_demo_reports_diagnostics_as_json_events() {
    let broken = "module Main exposing (x)\nx : Number\nlet x = \"no\"\n";
    let checked: Value = serde_json::from_str(&flick_wasm::check(broken)).expect("json");
    let event = &checked[0];
    assert_eq!(event["type"], "diagnostic");
    assert!(
        event["code"]
            .as_str()
            .is_some_and(|c| c.starts_with("FLICK-T")),
        "{event}"
    );
    let compiled: Value =
        serde_json::from_str(&flick_wasm::compile(broken, "debug")).expect("json");
    assert_eq!(compiled["js"], Value::Null);
    assert_eq!(compiled["diagnostics"], checked);
    let fine: Value = serde_json::from_str(&flick_wasm::check(MODULE)).expect("json");
    assert_eq!(fine, Value::Array(Vec::new()));
}

#[test]
fn the_demo_builds_everything_a_run_needs() {
    let built: Value = serde_json::from_str(&flick_wasm::build(MODULE, "debug")).expect("json");
    assert_eq!(built["entry"], runner_path(false));
    let files = built["files"].as_object().expect("files");
    assert!(files.contains_key("Main.js") && files.contains_key("_flick/Platform.js"));
    assert!(files.contains_key(built["entry"].as_str().expect("runner")));
    let library = "module Lib exposing (one)\none : Number\nlet one = 1\n";
    let built: Value = serde_json::from_str(&flick_wasm::build(library, "release")).expect("json");
    assert_eq!(built["entry"], Value::Null);
    let broken = "module Main exposing (x)\nx : Number\nlet x = \"no\"\n";
    let built: Value = serde_json::from_str(&flick_wasm::build(broken, "debug")).expect("json");
    assert_eq!(built["files"], Value::Null);
}
