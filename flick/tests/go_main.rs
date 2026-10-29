//! A program built for the go target ends as one on Node does: the exit
//! code follows its outcome, and SIGINT or SIGTERM close its scopes before
//! it exits with 130 or 143. Ignored by default: part of `just e2e`, needs
//! Go.

mod common;

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
            let started = rx.recv_timeout(Duration::from_secs(60));
            assert_eq!(started.as_deref(), Ok("started"), "{name}");
            #[cfg(unix)]
            signal(&child, s);
            #[cfg(not(unix))]
            let _ = s;
            let cleanup = rx.recv_timeout(Duration::from_secs(60));
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
    let test = flick_in(&dir, &["test", "--seed", "7"]);
    assert_eq!(test.status.code(), Some(1), "{test:?}");
    assert_eq!(
        String::from_utf8_lossy(&test.stdout),
        "ok    App.MainTest > adds\n\
         FAIL  App.MainTest > small: assertion: too big (case 1: 7)\n\
         FAIL  App.MainTest > typed: assertion: expected \"a\", got \"b\"\n\
         \n1 passed, 2 failed\nseed: 7 (rerun with --seed 7)\n"
    );
    assert!(dir.join(".flick/test").join(executable("test")).is_file());
}
