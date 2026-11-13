//! A backend package selection takes effect before any old output is restored.

mod common;

use std::fs;
#[cfg(feature = "wasm-backends")]
use std::path::Path;

use common::{flick_in, project};

const CONFIG: &str = "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n";

fn pinned() -> String {
    format!(
        "{CONFIG}\n[build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
         sha256 = \"{}\"\ncomponent = \"backend.wasm\"\n",
        "a".repeat(64)
    )
}

#[test]
fn a_backend_package_selection_cannot_fall_through_to_native_codegen() {
    let root = project(
        "backend_selection",
        &[
            ("flick.toml", CONFIG),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            ("backend.wasm", "not a component"),
        ],
    );
    let native = flick_in(&root, &["build"]);
    assert!(native.status.success(), "{native:?}");
    let old = fs::read(root.join("dist/_flick/App.js")).expect("old output");
    fs::write(root.join("flick.toml"), pinned()).expect("select a backend");
    for command in ["build", "run", "test", "build"] {
        let selected = flick_in(&root, &[command, "--diagnostics", "json"]);
        assert_eq!(selected.status.code(), Some(1), "{selected:?}");
        let errors = String::from_utf8_lossy(&selected.stderr);
        assert!(errors.contains("FLICK-P013"), "{errors}");
        #[cfg(not(feature = "wasm-backends"))]
        assert!(
            errors.contains("built without WebAssembly backends"),
            "{errors}"
        );
        #[cfg(feature = "wasm-backends")]
        assert!(errors.contains("digest"), "{errors}");
        assert_eq!(
            fs::read(root.join("dist/_flick/App.js")).expect("old output"),
            old
        );
    }
}

#[cfg(feature = "wasm-backends")]
#[test]
#[ignore = "requires just wasm-backends"]
fn a_pinned_component_builds_runs_and_restores_binary_output() {
    let root = project(
        "backend_component_run",
        &[
            ("flick.toml", CONFIG),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds the fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    let config = pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes));
    fs::write(root.join("flick.toml"), config).expect("pin");
    for profile in ["debug", "release"] {
        for _ in 0..2 {
            let build = flick_in(&root, &["build", "--profile", profile, "--offline"]);
            assert!(build.status.success(), "{build:?}");
            assert_eq!(
                fs::read(root.join("dist/data.bin")).expect("binary"),
                [0, 255, 128]
            );
            assert_eq!(
                fs::read_to_string(root.join("dist/_flick/App.js")).expect("source"),
                "export const main = (_) => undefined;\n"
            );
            assert!(
                root.join("dist/invoke-main.mjs").is_file(),
                "runner chosen by the backend"
            );
            assert!(!root.join("dist/flick-main.js").exists());
            fs::remove_dir_all(root.join("dist")).expect("remove outputs");
        }
        let run = flick_in(&root, &["run", "--profile", profile, "--offline"]);
        assert!(run.status.success(), "{run:?}");
        assert!(run.stdout.is_empty(), "pure Unit main prints nothing");
    }
    fs::write(root.join("backend.wasm"), "changed").expect("change component bytes");
    let invalidated = flick_in(&root, &["build", "--offline"]);
    assert_eq!(invalidated.status.code(), Some(1), "{invalidated:?}");
    assert!(String::from_utf8_lossy(&invalidated.stderr).contains("digest"));
}

#[cfg(feature = "wasm-backends")]
#[test]
#[ignore = "requires just wasm-backends"]
fn a_component_test_launch_executes_assertions_and_preserves_reporter_options() {
    let root = project(
        "backend_component_tests",
        &[
            ("flick.toml", CONFIG),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            (
                "test/Suite.fl",
                "module Suite exposing (tests)\ntests : List Test.Test\nlet tests = [Test.test \"pass\" (fun () -> Test.Pass), Test.test \"failure\" (fun () -> Test.Failed \"expected failure\")]\n",
            ),
        ],
    );
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    fs::write(
        root.join("flick.toml"),
        pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes)),
    )
    .expect("pin");
    for profile in ["debug", "release"] {
        for _ in 0..2 {
            let result = flick_in(
                &root,
                &[
                    "test",
                    "--profile",
                    profile,
                    "--reporter",
                    "json",
                    "--seed",
                    "7",
                    "--jobs",
                    "2",
                    "--offline",
                ],
            );
            assert_eq!(result.status.code(), Some(1), "{result:?}");
            let json: serde_json::Value =
                serde_json::from_slice(&result.stdout).expect("real test report");
            assert_eq!(json["seed"], 7);
            let tests = json["tests"].as_array().expect("test results");
            assert_eq!(tests.len(), 2, "{json}");
            assert_eq!(tests[0]["status"], "passed", "{json}");
            assert_eq!(tests[1]["status"], "failed", "{json}");
            assert!(
                result
                    .stdout
                    .windows(b"expected failure".len())
                    .any(|s| s == b"expected failure")
            );
            assert!(root.join(".flick/test/invoke-tests.mjs").is_file());
            assert!(
                !root.join(".flick/test/test.js").exists(),
                "host must use the component launch"
            );
            fs::remove_dir_all(root.join(".flick/test")).expect("remove outputs");
        }
        let filtered = flick_in(
            &root,
            &[
                "test",
                "--profile",
                profile,
                "--filter",
                "pass",
                "--reporter",
                "json",
                "--offline",
            ],
        );
        assert!(filtered.status.success(), "{filtered:?}");
    }
}

#[cfg(feature = "wasm-backends")]
#[test]
#[ignore = "requires just wasm-backends"]
fn component_preflight_refusals_preserve_spans_and_existing_output() {
    let root = project(
        "backend_component_refusals",
        &[
            ("flick.toml", CONFIG),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    let config = pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes));
    fs::write(root.join("flick.toml"), &config).expect("pin");
    let build = flick_in(&root, &["build"]);
    assert!(build.status.success(), "{build:?}");
    let old = fs::read(root.join("dist/_flick/App.js")).expect("previous output");
    let source = "module App exposing (main)\n// π preserves UTF-8 byte offsets\nmain : Unit -> Unit\nlet main unit = unit\n";
    fs::write(root.join("src/App.fl"), source).expect("unsupported identity function");
    let rejected = flick_in(&root, &["build", "--diagnostics", "json"]);
    assert_eq!(rejected.status.code(), Some(1), "{rejected:?}");
    let diagnostics: Vec<serde_json::Value> = String::from_utf8(rejected.stderr)
        .expect("UTF-8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("diagnostic"))
        .collect();
    let diagnostic = diagnostics
        .iter()
        .find(|d| d["code"] == "FLICK-P012")
        .expect("backend unsupported diagnostic");
    assert_eq!(diagnostic["path"], "src/App.fl");
    let start =
        usize::try_from(diagnostic["range"]["start"].as_u64().expect("start")).expect("start fits");
    let end = usize::try_from(diagnostic["range"]["end"].as_u64().expect("end")).expect("end fits");
    assert_eq!(&source[start..end], "let main unit = unit");
    assert!(
        diagnostic["message"]
            .as_str()
            .expect("message")
            .contains("test/literal@1.0.0#")
    );
    assert_eq!(
        fs::read(root.join("dist/_flick/App.js")).expect("previous output"),
        old
    );
    fs::write(
        root.join("src/App.fl"),
        "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("supported main");
    for invalid in [
        config.replace("1.0.0", "1.0.1"),
        config.replace(
            "[build.backend]",
            "[build]\nenvironment = \"js/browser\"\n[build.backend]",
        ),
    ] {
        fs::write(root.join("flick.toml"), invalid).expect("incompatible target");
        let rejected = flick_in(&root, &["build", "--diagnostics", "json"]);
        assert_eq!(rejected.status.code(), Some(1), "{rejected:?}");
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("FLICK-P013"));
        assert_eq!(
            fs::read(root.join("dist/_flick/App.js")).expect("previous output"),
            old
        );
    }
}

#[test]
fn an_environment_cannot_be_silently_ignored_by_the_native_baseline() {
    let root = project(
        "backend_native_environment",
        &[
            ("flick.toml", CONFIG),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    fs::write(
        root.join("flick.toml"),
        format!("{CONFIG}\n[build]\nenvironment = \"js/node\"\n"),
    )
    .expect("node");
    let node = flick_in(&root, &["build"]);
    assert!(node.status.success(), "{node:?}");
    let old = fs::read(root.join("dist/_flick/App.js")).expect("node output");
    fs::write(
        root.join("flick.toml"),
        format!("{CONFIG}\n[build]\nenvironment = \"js/browser\"\n"),
    )
    .expect("browser");
    let browser = flick_in(&root, &["build", "--diagnostics", "json"]);
    assert_eq!(browser.status.code(), Some(1), "{browser:?}");
    let errors = String::from_utf8_lossy(&browser.stderr);
    assert!(
        errors.contains("FLICK-P013") && errors.contains("js/browser"),
        "{errors}"
    );
    assert_eq!(
        fs::read(root.join("dist/_flick/App.js")).expect("node output"),
        old
    );
}
