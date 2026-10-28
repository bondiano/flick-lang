//! `flick test` on a project of the repository's target directory, which
//! resolves `@flick-lang/runtime` from the workspace. Needs Node and
//! `pnpm install`: run with `--ignored` (part of `just e2e`).

mod common;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{fs, thread};

use common::flick_in;
use serde_json::Value;

const SUITE: &str = r#"module Test.Sample exposing (tests)

type error Boom = Boom String

crash : String ->{} Unit
foreign sync crash from global "JSON.parse"

note : String ->{} Unit
let note text = Platform.stderr_line text

tests : List Test.Test
let tests =
  [
    Test.test "adds" (fun () -> Test.equal 2 (1 + 1)),
    Test.test "fails an assertion" (fun () -> Test.equal 3 (1 + 1)),
    Test.effect_test "expects in a task" (task { Test.expect (Test.equal "a" "b") }),
    Test.effect_test "fails with a typed error" (task { from_result (Err (Boom "x")) }),
    Test.effect_test "dies" (task { crash "{" }),
    Test.effect_test
      "fails to clean up"
      (task {
        let r = Resource.acquire_release (task { 1 }) (fun _ -> crash "{")
        ()
      }),
    Test.effect_test
      "releases"
      (task {
        let r = Resource.acquire_release (task { 1 }) (fun _ -> note "released")
        ()
      }),
    Test.group
      "numbers"
      [
        Test.property
          "reverse twice"
          (Gen.list 10 (Gen.int 0 100))
          (fun xs -> Test.equal xs (List.reverse (List.reverse xs))),
        Test.property "small" (Gen.int 0 100) (fun n -> Test.is_true "too big" (n < 50))
      ]
  ]
"#;

fn project(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/tmp/test_command")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    fs::create_dir_all(dir.join("test/Test")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{ \"name\": \"@acme/sample\" }\n").expect("write");
    fs::write(
        dir.join("src/App/Lib.fl"),
        "module App.Lib exposing (one)\n\none : Float\nlet one = 1\n",
    )
    .expect("write");
    fs::write(dir.join("test/Test/Sample.fl"), SUITE).expect("write");
    dir
}

fn results(out: &Output) -> Value {
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).unwrap_or_else(|e| panic!("{e}: {out:?}"))
}

fn status<'a>(json: &'a Value, name: &str) -> (&'a str, &'a str) {
    let test = json["tests"]
        .as_array()
        .expect("tests")
        .iter()
        .find(|t| t["name"].as_str().is_some_and(|n| n.ends_with(name)))
        .unwrap_or_else(|| panic!("no test {name}: {json}"));
    (
        test["status"].as_str().unwrap_or_default(),
        test["kind"].as_str().unwrap_or_default(),
    )
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn outcomes_are_reported_apart() {
    let dir = project("outcomes");
    let out = flick_in(&dir, &["test", "--reporter", "json", "--seed", "7"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let json = results(&out);
    assert_eq!(json["seed"], 7);
    assert_eq!(status(&json, "adds"), ("passed", ""));
    assert_eq!(status(&json, "fails an assertion"), ("failed", "assertion"));
    assert_eq!(status(&json, "expects in a task"), ("failed", "assertion"));
    assert_eq!(
        status(&json, "fails with a typed error"),
        ("failed", "failure")
    );
    assert_eq!(status(&json, "dies"), ("failed", "defect"));
    assert_eq!(status(&json, "fails to clean up"), ("failed", "cleanup"));
    assert_eq!(status(&json, "releases"), ("passed", ""));
    assert_eq!(status(&json, "numbers > reverse twice"), ("passed", ""));
    assert_eq!(status(&json, "numbers > small"), ("failed", "assertion"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("released"));
    // the same seed draws the same values
    let again = results(&flick_in(
        &dir,
        &["test", "--reporter", "json", "--seed", "7"],
    ));
    let message = |j: &Value| {
        j["tests"]
            .as_array()
            .and_then(|ts| {
                ts.iter()
                    .find(|t| t["name"].as_str().is_some_and(|n| n.ends_with("small")))
            })
            .map(|t| t["message"].clone())
    };
    assert_eq!(message(&json), message(&again));
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn filter_jobs_and_the_human_reporter() {
    let dir = project("filter");
    let out = flick_in(&dir, &["test", "--filter", "adds", "--jobs", "4"]);
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("ok    Test.Sample > adds"), "{text}");
    assert!(text.contains("1 passed, 0 failed"), "{text}");
    let failing = flick_in(&dir, &["test", "--filter", "small", "--seed", "3"]);
    let text = String::from_utf8_lossy(&failing.stdout);
    assert!(text.contains("seed: 3 (rerun with --seed 3)"), "{text}");
    let zero = flick_in(&dir, &["test", "--jobs", "0"]);
    assert_eq!(zero.status.code(), Some(2), "{zero:?}");
}

const SLOW: &str = r#"module Test.Slow exposing (tests)

sleep : Float ->{} Unit
foreign async sleep from "./sleep.mjs" "sleep"

tests : List Test.Test
let tests =
  [
    Test.effect_test
      "holds a resource"
      (task {
        let r =
          Resource.acquire_release
            (task { Platform.stdout_line "acquired" })
            (fun _ -> Platform.stdout_line "released")
        sleep 3000
      })
  ]
"#;

/// Collects the child's stdout as it comes.
fn collect(child: &mut Child) -> Arc<Mutex<String>> {
    let out = Arc::new(Mutex::new(String::new()));
    let mut stdout = child.stdout.take().expect("piped");
    let sink = out.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 256];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock()
                .expect("lock")
                .push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    });
    out
}

fn wait_for(out: &Mutex<String>, text: &str, count: usize) -> String {
    for _ in 0..200 {
        let now = out.lock().expect("lock").clone();
        if now.matches(text).count() >= count {
            return now;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("no {count}× {text:?} in {:?}", out.lock().expect("lock"));
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn watch_runs_again_after_the_previous_run_cleaned_up() {
    let dir = project("watch");
    fs::remove_file(dir.join("test/Test/Sample.fl")).expect("remove");
    fs::write(dir.join("test/Test/Slow.fl"), SLOW).expect("write");
    fs::write(
        dir.join("test/Test/sleep.mjs"),
        "export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));\n",
    )
    .expect("write");
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["test", "--watch"])
        .current_dir(&dir)
        .stdout(Stdio::piped())
        .spawn()
        .expect("flick runs");
    let out = collect(&mut child);
    wait_for(&out, "acquired", 1);
    thread::sleep(Duration::from_millis(300));
    fs::write(
        dir.join("src/App/Lib.fl"),
        "module App.Lib exposing (one)\n\none : Float\nlet one = 2\n",
    )
    .expect("write");
    let text = wait_for(&out, "acquired", 2);
    // the runner of the second run stops by itself: its stdin closes with the watcher
    let _ = child.kill();
    let released = text
        .find("released")
        .expect("the first run released its resource");
    let changed = text.find("a file changed").expect("the change was noticed");
    let second = text.rfind("acquired").expect("second run");
    assert!(released < changed && changed < second, "{text}");
}

const CLOCKS: &str = r#"module Test.Clocks exposing (tests)

hour : Duration.Duration
let hour = Result.with_default Duration.zero (Duration.hours 1)

tests : List Test.Test
let tests =
  [
    Test.clock_test
      "an hour passes when the test moves the clock"
      (task {
        scope s {
          let sleeper = Fiber.spawn s (fun _ -> ()) (task {
            Effect.sleep hour
            Clock.current_time_millis ()
          })
          Test.adjust hour
          Test.expect (Test.equal 3600000 (Fiber.join sleeper))
        }
      }),
    Test.clock_test
      "each test starts at zero"
      (task {
        Test.expect (Test.equal 0 (Clock.current_time_millis ()))
        Test.set_time 1000
        Test.expect (Test.equal 1000 (Clock.current_time_millis ()))
      }),
    Test.clock_test
      "another test's clock is not this one"
      (task { Test.expect (Test.equal 0 (Clock.current_time_millis ())) }),
  ]
"#;

#[test]
#[ignore = "needs Node and pnpm install"]
fn clock_tests_run_on_virtual_time() {
    let dir = project("clocks");
    fs::write(
        dir.join("test/Test/Sample.fl"),
        CLOCKS.replace("Test.Clocks", "Test.Sample"),
    )
    .expect("write");
    let started = Instant::now();
    let out = flick_in(&dir, &["test", "--reporter", "json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let json = results(&out);
    for name in [
        "an hour passes when the test moves the clock",
        "each test starts at zero",
        "another test's clock is not this one",
    ] {
        assert_eq!(status(&json, name), ("passed", ""), "{json}");
    }
    // no real hour, nor a real wait at all
    assert!(started.elapsed() < Duration::from_secs(60));
}
