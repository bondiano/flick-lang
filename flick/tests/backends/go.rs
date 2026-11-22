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

#[test]
#[ignore = "requires Go"]
fn go_scaffolded_templates_test_and_run() {
    let root = project("go_scaffold_e2e", &[]);
    fs::create_dir_all(&root).expect("dir");

    let app_created = flick_in(&root, &["new", "go-app", "--target", "go"]);
    assert!(app_created.status.success(), "{app_created:?}");
    let app = root.join("go-app");
    let tested = flick_in(&app, &["test", "--offline"]);
    assert!(tested.status.success(), "{tested:?}");
    let ran = flick_in(&app, &["run", "--offline", "--", "Ada"]);
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "Hello, Ada!\n");

    let cli_created = flick_in(
        &root,
        &["new", "go-cli", "--target", "go", "--template", "cli"],
    );
    assert!(cli_created.status.success(), "{cli_created:?}");
    let cli = root.join("go-cli");
    let tested = flick_in(&cli, &["test", "--offline"]);
    assert!(tested.status.success(), "{tested:?}");
    let help = flick_in(&cli, &["run", "--offline", "--", "--help"]);
    assert!(help.status.success(), "{help:?}");
    let help_text = String::from_utf8_lossy(&help.stdout);
    assert!(
        help_text.contains("go-cli") && help_text.contains("--loud"),
        "{help_text}"
    );
    let ran = flick_in(&cli, &["run", "--offline", "--", "--loud", "Ada"]);
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "HELLO, ADA!\n");

    let lib_created = flick_in(
        &root,
        &["new", "go-lib", "--target", "go", "--template", "lib"],
    );
    assert!(lib_created.status.success(), "{lib_created:?}");
    let lib = root.join("go-lib");
    let tested = flick_in(&lib, &["test", "--offline"]);
    assert!(tested.status.success(), "{tested:?}");
}

#[test]
#[ignore = "requires Go"]
fn go_empty_test_run_passes() {
    let config = format!("{CONFIG}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "official_go_empty_test",
        &[("flick.toml", &config), ("src/App.fl", MAIN)],
    );
    fs::create_dir_all(root.join("test")).expect("test dir");
    let tested = flick_in(&root, &["test", "--reporter", "json", "--offline"]);
    assert!(tested.status.success(), "{tested:?}");
    let report: serde_json::Value = serde_json::from_slice(&tested.stdout).expect("json report");
    assert_eq!(
        report["tests"].as_array().map(Vec::len),
        Some(0),
        "{report}"
    );
}

#[cfg(unix)]
#[test]
#[ignore = "requires Go"]
fn go_dev_restarts_and_shuts_down_cleanly() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    let waiting = |version: &str| {
        format!(
            "module App exposing (main)\n\n\
             main : Unit ->{{Host.Console, Async; AppError}} Unit\n\
             let main () =\n  \
               scope {{\n    \
                 Resource.acquire_release (task {{ Platform.stdout_line \"started {version}\" }}) (fun _ -> Platform.stdout_line \"cleanup {version}\")\n    \
                 Effect.never ()\n  \
               }}\n"
        )
    };
    let config = format!("{CONFIG}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project(
        "official_go_dev",
        &[("flick.toml", &config), ("src/App.fl", &waiting("v1"))],
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["dev", "--offline"])
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn flick dev");
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let expect = |wanted: &str| {
        let deadline = Instant::now() + Duration::from_mins(1);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) if line == wanted => return,
                Ok(_) => {}
                Err(err) => panic!("timed out waiting for `{wanted}`: {err}"),
            }
        }
    };
    expect("started v1");
    thread::sleep(Duration::from_millis(1100));
    fs::write(root.join("src/App.fl"), waiting("v2")).expect("update App.fl");
    expect("cleanup v1");
    expect("started v2");
    Command::new("kill")
        .args(["-s", "TERM", &child.id().to_string()])
        .status()
        .expect("kill");
    expect("cleanup v2");
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(143), "{status:?}");
}

#[test]
#[ignore = "requires Go"]
fn cross_target_go_override_in_js_project_invalidates_cache_on_go_file_edit() {
    // Default target is JS (no `[build] target = "go"`), overridden with `--target go`.
    let config = format!("{CONFIG}[tools]\n\"flick/go\" = [\"go\"]\n");
    let main = concat!(
        "module App exposing (main)\n\n",
        "say : Unit ->{} String\n",
        "foreign say from \"./native.mjs\" \"say\"\n\n",
        "main : Unit ->{Host.Console} Unit\n",
        "let main () = Platform.stdout_line (say ())\n",
    );
    let go_v1 = "package main\n\nimport \"flick.app/flickrt\"\n\nfunc say(_ *flickrt.Fiber, _ flickrt.Value) flickrt.Value { return \"go-v1\" }\n";
    let go_v2 = "package main\n\nimport \"flick.app/flickrt\"\n\nfunc say(_ *flickrt.Fiber, _ flickrt.Value) flickrt.Value { return \"go-v2\" }\n";
    let root = project(
        "cross_target_go_cache",
        &[
            ("flick.toml", &config),
            ("src/App.fl", main),
            ("src/native.go", go_v1),
        ],
    );
    let inputs = flick_in(&root, &["build", "--inputs", "--target", "go"]);
    assert!(inputs.status.success(), "{inputs:?}");
    assert!(
        String::from_utf8_lossy(&inputs.stdout).contains("native.go"),
        "{inputs:?}"
    );

    let first = flick_in(&root, &["run", "--target", "go", "--offline"]);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(String::from_utf8_lossy(&first.stdout), "go-v1\n");

    fs::write(root.join("src/native.go"), go_v2).expect("update native.go");
    let second = flick_in(&root, &["run", "--target", "go", "--offline"]);
    assert!(second.status.success(), "{second:?}");
    assert_eq!(String::from_utf8_lossy(&second.stdout), "go-v2\n");
}
