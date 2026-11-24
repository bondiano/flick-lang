//! Ordinary commands must use the shipped component without a project pin.

mod common;
use common::{flick_in, project};
use flick_backend::BackendPin;
use serde::Deserialize;
use std::fs;
use std::process::Command;

#[derive(Deserialize)]
struct Manifest {
    backend: BackendPin,
    launches: Vec<Launch>,
}

#[derive(Deserialize)]
struct Launch {
    name: String,
}

#[test]
fn compilation_without_node_can_be_rebuilt_with_a_pinned_launch() {
    let root = project(
        "default_js_without_node",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let empty = root.join("empty-bin");
    fs::create_dir(&empty).expect("empty PATH");
    let built = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["build", "--offline"])
        .env("PATH", &empty)
        .current_dir(&root)
        .output()
        .expect("compiler runs without Node");
    assert!(built.status.success(), "{built:?}");
    assert!(root.join("dist/flick-main.js").is_file());
    let manifest = || {
        let bytes = fs::read(root.join("dist/.flick-backend.cbor")).expect("component manifest");
        ciborium::from_reader::<Manifest, _>(&bytes[..]).expect("manifest")
    };
    assert!(manifest().launches.is_empty());
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(built.status.success(), "{built:?}");
    assert_eq!(manifest().launches[0].name, "main");
    let run = flick_in(&root, &["run", "--offline"]);
    assert!(run.status.success(), "{run:?}");
}

#[test]
fn release_profile_defaults_to_the_official_component() {
    let root = project(
        "default_js_release_component",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let built = flick_in(&root, &["build", "--profile", "release", "--offline"]);
    assert!(built.status.success(), "{built:?}");
    let bytes = fs::read(root.join("dist/.flick-backend.cbor"))
        .expect("ordinary release builds execute the component");
    let manifest: Manifest = ciborium::from_reader(&bytes[..]).expect("launch manifest");
    assert_eq!(manifest.backend.identity.id, "flick/js");
    assert_eq!(manifest.backend.identity.version, "0.2.0");
    let component = fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../backends/js/package/component.wasm"
    ))
    .expect("shipped component");
    assert_eq!(manifest.backend.sha256, flick_backend::sha256(&component));
}

#[test]
#[ignore = "needs the installed runtime; run just e2e-tests or just wasm-backends"]
fn ordinary_run_and_empty_test_use_the_selected_component() {
    let root = project(
        "default_js_run_and_empty_tests",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let run = flick_in(&root, &["run", "--profile", "release", "--offline"]);
    assert!(run.status.success(), "{run:?}");
    let tested = flick_in(
        &root,
        &["test", "--seed", "42", "--reporter", "json", "--offline"],
    );
    assert!(tested.status.success(), "{tested:?}");
    let json: serde_json::Value =
        serde_json::from_slice(&tested.stdout).expect("empty test report");
    assert_eq!(json["seed"], 42);
    assert_eq!(json["passed"], 0);
    assert_eq!(json["failed"], 0);
    assert_eq!(json["tests"], serde_json::json!([]));
    assert!(root.join(".flick/test/.flick-backend.cbor").is_file());
}

#[test]
fn the_release_js_component_folds_an_immediate_tuple_match() {
    // Opaque callbacks keep the scrutinee dynamic through release evaluation.
    let root = project(
        "default_js_tuple_match",
        &[(
            "Probe.fl",
            "module Probe exposing (first)
first : (Int -> Int) -> (Int -> Int) -> Int -> Int
let first f g x =
  match (f x, g x) with
  | (a, _) -> a
",
        )],
    );
    let built = flick_in(
        &root,
        &[
            "build",
            "Probe.fl",
            "--offline",
            "--target",
            "js",
            "--profile",
            "release",
            "--out-dir",
            "out",
        ],
    );
    assert!(built.status.success(), "{built:?}");
    let js = fs::read_to_string(root.join("out/Probe.js")).expect("emitted library");
    let first = js
        .split("export const ")
        .find_map(|declaration| declaration.strip_prefix("first$3 = "))
        .expect("the exported three-argument worker");
    assert!(
        !first.contains('['),
        "tuple construction or projection survived in first:\n{first}"
    );
}
