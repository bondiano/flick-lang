//! Ordinary commands must use the shipped component without a project pin.

mod common;
use common::{flick_in, project};
use flick_backend::BackendPin;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
struct Manifest {
    backend: BackendPin,
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
    assert_eq!(manifest.backend.identity.version, "0.1.0");
    let component = fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../backends/js/package/component.wasm"
    ))
    .expect("shipped component");
    assert_eq!(manifest.backend.sha256, flick_backend::sha256(&component));
}

#[test]
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
