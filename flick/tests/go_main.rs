//! A program built for the go target ends as one on Node does: the exit
//! code follows its outcome, and SIGINT or SIGTERM close its scopes before
//! it exits with 130 or 143. Ignored by default: part of `just e2e`, needs
//! Go.

mod common;

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use common::{APP, flick_in, project};
use flick_core::build::go::executable;

/// An app that prints `started`, waits for a signal and prints `cleanup`
/// when its scope closes.
const WAITING: &str = "module App.Main exposing (main)

main : Unit ->{; AppError} Unit
let main () =
  scope {
    Resource.acquire_release (task { Platform.stdout_line \"started\" }) (fun _ -> Platform.stdout_line \"cleanup\")
    Effect.never ()
  }
";

#[cfg(unix)]
fn signal(child: &Child, name: &str) {
    Command::new("kill")
        .args(["-s", name, &child.id().to_string()])
        .status()
        .expect("kill");
}

/// Lines of the child's stdout as they come.
fn lines(child: &mut Child) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().expect("piped stdout");
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

#[test]
#[ignore = "needs Go"]
fn unchanged_go_build_reuses_and_restores_its_executable() {
    let main = "module App.Main exposing (main)\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"hi\"\n";
    let dir = project(
        "go_build_noop",
        &[("flick.toml", APP), ("src/App/Main.fl", main)],
    );
    let binary = dir.join("dist").join(executable("main"));
    assert!(flick_in(&dir, &["build", "--emit", "go"]).status.success());
    let before = fs::metadata(&binary)
        .expect("binary")
        .modified()
        .expect("mtime");
    let repeated = flick_in(&dir, &["build", "--emit", "go"]);
    assert!(repeated.status.success(), "{repeated:?}");
    assert!(!String::from_utf8_lossy(&repeated.stderr).contains("Building Go executable"));
    assert_eq!(
        fs::metadata(&binary)
            .expect("binary")
            .modified()
            .expect("mtime"),
        before
    );
    fs::remove_file(&binary).expect("remove binary");
    let restored = flick_in(&dir, &["build", "--emit", "go"]);
    assert!(restored.status.success(), "{restored:?}");
    assert!(String::from_utf8_lossy(&restored.stderr).contains("Building Go executable"));
    assert!(binary.is_file());
}

#[test]
#[ignore = "needs Go"]
fn exit_codes_follow_the_outcome_and_signals() {
    let fail = "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = App.fail \"nope\"\n";
    let done = WAITING.replace("Effect.never ()", "()");
    let mut cases: Vec<(&str, &str, Option<&str>, i32)> = vec![
        ("go_exit_ok", &done, None, 0),
        ("go_exit_fail", fail, None, 1),
    ];
    // windows has no signals to send another process: Go reads a console's
    // Ctrl+C and close as SIGINT and SIGTERM, which only a console delivers
    if cfg!(unix) {
        cases.push(("go_exit_int", WAITING, Some("INT"), 130));
        cases.push(("go_exit_term", WAITING, Some("TERM"), 143));
    }
    for (name, source, signal_name, expected) in cases {
        let dir = project(name, &[("flick.toml", APP), ("src/App/Main.fl", source)]);
        let build = flick_in(&dir, &["build", "--emit", "go"]);
        assert!(build.status.success(), "{name}: {build:?}");
        let mut child = Command::new(dir.join("dist").join(executable("main")))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the executable runs");
        let rx = lines(&mut child);
        if let Some(s) = signal_name {
            let started = rx.recv_timeout(Duration::from_mins(1));
            assert_eq!(started.as_deref(), Ok("started"), "{name}");
            #[cfg(unix)]
            signal(&child, s);
            #[cfg(not(unix))]
            let _ = s;
            let cleanup = rx.recv_timeout(Duration::from_mins(1));
            assert_eq!(
                cleanup.as_deref(),
                Ok("cleanup"),
                "{name}: the scope closes"
            );
        }
        let status = child.wait().expect("wait");
        assert_eq!(status.code(), Some(expected), "{name}");
    }
}

/// A project that emits go: `flick run` starts its executable with the
/// arguments, and `flick test` runs its tests in one executable, as the
/// node runner reports them.
#[test]
#[ignore = "needs Go"]
fn run_and_test_use_the_go_executable() {
    let config = format!("{APP}test-dir = \"test\"\n\n[build]\nemit = \"go\"\n");
    let main = "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (String.join \",\" (Platform.args ()))\n";
    let tests = "module App.MainTest exposing (tests)

import Test exposing (..)

tests : List Test
let tests = [
  test \"adds\" (fun () -> equal 4 (2 + 2)),
  property \"small\" (Gen.int 0 9) (fun x -> is_true \"too big\" (x < 5)),
  effect_test \"typed\" (task { expect (equal \"a\" \"b\") })
]
";
    let dir = project(
        "go_run_test",
        &[
            ("flick.toml", &config),
            ("src/App/Main.fl", main),
            ("test/App/MainTest.fl", tests),
        ],
    );
    let run = flick_in(&dir, &["run", "--", "a", "b"]);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "a,b\n", "{run:?}");
    let test = flick_in(&dir, &["test", "--seed", "7", "--reporter", "json"]);
    assert_eq!(test.status.code(), Some(1), "{test:?}");
    let report: serde_json::Value = serde_json::from_slice(&test.stdout).expect("JSON report");
    assert_eq!(
        report,
        serde_json::json!({
            "seed": 7, "passed": 1, "failed": 2,
            "tests": [
                {"name": "App.MainTest > adds", "status": "passed"},
                {"name": "App.MainTest > small", "status": "failed", "kind": "assertion", "message": "too big (case 1: 7)"},
                {"name": "App.MainTest > typed", "status": "failed", "kind": "assertion", "message": "expected \"a\", got \"b\""}
            ]
        })
    );
    assert!(dir.join(".flick/test").join(executable("test")).is_file());
}

/// Failing tests read the same on Node and as a Go executable: the kind of
/// each failure, its message and the case of a property, whatever it holds.
#[test]
#[ignore = "needs Go and Node"]
fn failures_report_alike_on_both_targets() {
    let tests = "module App.MainTest exposing (tests)

import Test exposing (..)

type error Boom = Boom String Int

fails : a -> Assertion
let fails _ = is_true \"no\" false

tests : List Test
let tests = [
  test \"floats\" (fun () -> equal [1.5, -0.0] [1.5, 0.0 / 0.0]),
  effect_test \"typed\" (task { from_result (Err (Boom \"x\" 1)) }),
  effect_test \"defect\" (task { ignore (Int.unsafe_div 1 (List.length [])) }),
  effect_test \"cleanup\" (task {
    let _r = Resource.acquire_release (task { 1 }) (fun _ -> ignore (Int.unsafe_rem 1 (List.length [])))
    from_result (Err (Boom \"y\" 2))
  }),
  property \"draws\" (Gen.pair (Gen.int 0 100) (Gen.list 3 (Gen.string 4))) (fun (n, _) -> is_true \"small\" (n < 30)),
  property \"numbers\" (Gen.constant [0.0 / 0.0, -0.0]) fails,
  property \"values\" (Gen.constant ([5n], { a = (); b = Some (Ok 1 : Result String Int) })) fails,
  property \"functions\" (Gen.constant [fun x -> x + 1]) fails
]
";
    let dir = project(
        "go_report_alike",
        &[
            ("flick.toml", &format!("{APP}test-dir = \"test\"\n")),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nmain : Unit ->{} Unit\nlet main () = ()\n",
            ),
            ("test/App/MainTest.fl", tests),
        ],
    );
    let run = |emit| {
        flick_in(
            &dir,
            &["test", "--seed", "42", "--emit", emit, "--reporter", "json"],
        )
    };
    let (js, go) = (run("js"), run("go"));
    assert_eq!(js.status.code(), Some(1), "{js:?}");
    assert_eq!(go.status.code(), Some(1), "{go:?}");
    let js_report: serde_json::Value =
        serde_json::from_slice(&js.stdout).expect("Node JSON report");
    let go_report: serde_json::Value = serde_json::from_slice(&go.stdout).expect("Go JSON report");
    assert_eq!(go_report, js_report);
    assert_eq!(
        js_report["tests"][7]["message"],
        "no (case 0: [\"<function>\"])"
    );
}

#[test]
#[ignore = "needs Go and Node"]
fn human_results_stream_after_cleanup() {
    let tests = r#"module App.TestOutput exposing (tests)

tests : List Test.Test
let tests =
  [
    Test.effect_test "first" (task {
      let _resource = Resource.acquire_release (task { () }) (fun _ -> Platform.stdout_line "first-cleaned")
      ()
    }),
    Test.effect_test "second" (task { Platform.stdout_line "second-started" })
  ]
"#;
    for emit in ["js", "go"] {
        let dir = project(
            &format!("test_output_{emit}"),
            &[
                ("flick.toml", &format!("{APP}test-dir = \"test\"\n")),
                (
                    "src/App/Main.fl",
                    "module App.Main exposing (main)\n\nmain : Unit ->{} Unit\nlet main () = ()\n",
                ),
                ("test/App/TestOutput.fl", tests),
            ],
        );
        for jobs in ["1", "2"] {
            let output = flick_in(
                &dir,
                &[
                    "test", "--emit", emit, "--jobs", jobs, "--color", "always", "--seed", "7",
                ],
            );
            assert!(output.status.success(), "{emit}, jobs={jobs}: {output:?}");
            let text = String::from_utf8(output.stdout).expect("UTF-8 output");
            let position = |needle: &str| {
                text.find(needle)
                    .unwrap_or_else(|| panic!("{needle:?}: {text}"))
            };
            assert_eq!(text.matches("\x1b[32m✓\x1b[0m").count(), 2, "{text}");
            assert_eq!(text.matches("App.TestOutput > first").count(), 1, "{text}");
            assert_eq!(text.matches("App.TestOutput > second").count(), 1, "{text}");
            assert_eq!(text.matches("  Tests  ").count(), 1, "{text}");
            assert!(
                text.contains("\x1b[32m2\x1b[0m passed · \x1b[2m0\x1b[0m failed · 2 total"),
                "{text}"
            );
            let first = position("App.TestOutput > first");
            let second = position("App.TestOutput > second");
            let summary = position("  Tests  ");
            assert!(
                position("first-cleaned") < first && first < summary,
                "{text}"
            );
            assert!(
                position("second-started") < second && second < summary,
                "{text}"
            );
            if jobs == "1" {
                assert!(first < position("second-started"), "{text}");
            }
        }
    }
}
