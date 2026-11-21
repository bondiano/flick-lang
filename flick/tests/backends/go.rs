//! The official Go component through the CLI, with no pin: build, run and
//! test agree with independent expectations, and what it cannot compile, or
//! a project that does not allow `go`, publishes nothing.

use std::fs;

use super::CONFIG;
use super::common::{flick_in, project};

const MAIN: &str = "module App exposing (main)\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (20 + 22))\n";

#[test]
#[ignore = "requires Go"]
fn official_go_component_builds_runs_and_tests_with_independent_results() {
    let config = format!("{CONFIG}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "official_go_component",
        &[
            ("flick.toml", &config),
            ("src/App.fl", MAIN),
            (
                "test/Suite.fl",
                "module Suite exposing (tests)\ntests : List Test.Test\nlet tests = [Test.test \"arithmetic\" (fun () -> Test.equal 42 (20 + 22)), Test.test \"expected failure\" (fun () -> Test.Failed \"independent error\")]\n",
            ),
        ],
    );
    for profile in ["debug", "release"] {
        let built = flick_in(&root, &["build", "--profile", profile, "--offline"]);
        assert!(built.status.success(), "{built:?}");
        assert!(
            root.join("dist/.flick-backend.cbor").is_file(),
            "the common component driver owns the launch"
        );
        assert!(
            root.join("dist/go/main/flickrt").is_dir(),
            "the runtime ships with the program"
        );
        let run = flick_in(&root, &["run", "--profile", profile, "--offline"]);
        assert!(run.status.success(), "{run:?}");
        assert_eq!(String::from_utf8(run.stdout).expect("UTF-8"), "42\n");
        let tested = flick_in(
            &root,
            &[
                "test",
                "--profile",
                profile,
                "--reporter",
                "json",
                "--offline",
            ],
        );
        assert_eq!(tested.status.code(), Some(1), "{tested:?}");
        let report: serde_json::Value =
            serde_json::from_slice(&tested.stdout).expect("test report");
        let tests = report["tests"].as_array().expect("tests");
        assert_eq!(tests.len(), 2, "{report}");
        assert_eq!(tests[0]["status"], "passed", "{report}");
        assert_eq!(tests[1]["status"], "failed", "{report}");
    }

    let published = fs::read(root.join("dist/go/main/go.mod")).expect("published module");
    let unsupported = "module App exposing (main)\nnow : Unit ->{} Float\nforeign now from global \"Date.now\"\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (now ()))\n";
    fs::write(root.join("src/App.fl"), unsupported).expect("unsupported source");
    let refused = flick_in(&root, &["build", "--offline", "--diagnostics", "json"]);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    let errors = String::from_utf8_lossy(&refused.stderr);
    assert!(
        errors.contains("FLICK-P012") && errors.contains("src/App.fl"),
        "{errors}"
    );
    assert_eq!(
        fs::read(root.join("dist/go/main/go.mod")).expect("kept"),
        published
    );

    fs::write(root.join("src/App.fl"), MAIN).expect("supported source");
    let ungranted = config.replace("[tools]\n\"flick/go\" = [\"go\"]\n", "");
    fs::write(root.join("flick.toml"), ungranted).expect("no grant");
    let denied = flick_in(&root, &["build", "--offline", "--diagnostics", "json"]);
    assert_eq!(denied.status.code(), Some(1), "{denied:?}");
    assert!(
        String::from_utf8_lossy(&denied.stderr).contains("FLICK-P029"),
        "{denied:?}"
    );
    assert_eq!(
        fs::read(root.join("dist/go/main/go.mod")).expect("kept"),
        published
    );
}

#[test]
#[ignore = "requires Go"]
fn official_go_cgo_option_builds_and_invalidates_cache() {
    let config = format!("{CONFIG}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "official_go_cgo",
        &[("flick.toml", &config), ("src/App.fl", MAIN)],
    );
    let default_build = flick_in(&root, &["build", "--offline"]);
    assert!(default_build.status.success(), "{default_build:?}");
    let after_default = fs::read_dir(root.join(".flick/cache"))
        .expect("cache")
        .count();
    assert_eq!(after_default, 1);

    let cgo_build = flick_in(&root, &["build", "--offline", "--cgo"]);
    assert!(cgo_build.status.success(), "{cgo_build:?}");
    let after_cgo = fs::read_dir(root.join(".flick/cache"))
        .expect("cache")
        .count();
    assert_eq!(after_cgo, 2);
}
