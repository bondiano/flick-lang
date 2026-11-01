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

broken : forall e. Failure e ->{} Unit
let broken _ = todo

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
      "a supervisor that dies"
      (task {
        scope o {
          let _f = Fiber.spawn o broken (task { todo })
          ()
        }
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
    assert_eq!(
        status(&json, "a supervisor that dies"),
        ("failed", "defect")
    );
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
fn filter_jobs_and_seed_select_results() {
    let dir = project("filter");
    let out = flick_in(
        &dir,
        &[
            "test",
            "--filter",
            "adds",
            "--jobs",
            "4",
            "--reporter",
            "json",
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let json = results(&out);
    assert_eq!(json["passed"], 1);
    assert_eq!(json["failed"], 0);
    assert_eq!(json["tests"].as_array().expect("tests").len(), 1);
    assert_eq!(status(&json, "adds"), ("passed", ""));
    let failing = flick_in(
        &dir,
        &[
            "test",
            "--filter",
            "small",
            "--seed",
            "3",
            "--reporter",
            "json",
        ],
    );
    assert_eq!(failing.status.code(), Some(1), "{failing:?}");
    let json = results(&failing);
    assert_eq!(json["seed"], 3);
    assert_eq!(status(&json, "small"), ("failed", "assertion"));
    assert!(
        json["tests"][0]["message"]
            .as_str()
            .expect("message")
            .contains("too big")
    );
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

/// Waits up to a minute: each run of `flick test` compiles the project
/// first, which takes over 10 s in a debug build.
fn wait_for(out: &Mutex<String>, text: &str, count: usize) -> String {
    for _ in 0..1200 {
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

type error Again = Again

hour : Duration.Duration
let hour = Result.with_default Duration.zero (Duration.hours 1)

two_hours : Duration.Duration
let two_hours = Result.with_default Duration.zero (Duration.hours 2)

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
    Test.clock_test
      "sleepers wake in order, each run before the clock moves on"
      (task {
        scope s {
          let order = Ref.make []
          let _ = Fiber.spawn s (fun _ -> ()) (task {
            Effect.sleep two_hours
            Ref.update order (fun xs -> xs ++ ["two"])
          })
          let _ = Fiber.spawn s (fun _ -> ()) (task {
            Effect.sleep hour
            Ref.update order (fun xs -> xs ++ ["one"])
          })
          Test.adjust hour
          Test.expect (Test.equal ["one"] (Ref.get order))
          Test.adjust hour
          Test.expect (Test.equal ["one", "two"] (Ref.get order))
        }
      }),
    Test.clock_test
      "a retry waits for its schedule's delays"
      (task {
        scope s {
          let calls = Ref.make 0
          let retried = Fiber.spawn s (fun _ -> ()) (task {
            try
              Effect.retry
                (Schedule.spaced hour)
                (task {
                  Ref.set calls (Ref.get calls + 1)
                  if Ref.get calls < 3 then from_result (Err Again) else "done"
                })
            with
            | Again -> "gave up"
          })
          Test.adjust hour
          Test.expect (Test.equal 2 (Ref.get calls))
          Test.adjust hour
          Test.expect (Test.equal "done" (Fiber.join retried))
        }
      }),
  ]
"#;

/// The names of the tests of `CLOCKS`.
const CLOCK_TESTS: [&str; 5] = [
    "an hour passes when the test moves the clock",
    "each test starts at zero",
    "another test's clock is not this one",
    "sleepers wake in order, each run before the clock moves on",
    "a retry waits for its schedule's delays",
];

/// Runs `CLOCKS` by `flick test` with `args`: each test passes, without
/// waiting the hours they move their clocks by.
fn clock_tests_pass(name: &str, args: &[&str]) {
    let dir = project(name);
    fs::write(
        dir.join("test/Test/Sample.fl"),
        CLOCKS.replace("Test.Clocks", "Test.Sample"),
    )
    .expect("write");
    let started = Instant::now();
    let out = flick_in(&dir, &[&["test", "--reporter", "json"], args].concat());
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let json = results(&out);
    for name in CLOCK_TESTS {
        assert_eq!(status(&json, name), ("passed", ""), "{json}");
    }
    // no real hour, nor a real wait at all
    assert!(started.elapsed() < Duration::from_secs(60));
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn clock_tests_run_on_virtual_time() {
    clock_tests_pass("clocks", &[]);
}

#[test]
#[ignore = "needs Go"]
fn clock_tests_run_on_virtual_time_on_go() {
    clock_tests_pass("clocks_go", &["--emit", "go"]);
}

/// Properties over `Schema.arbitrary`, each failing at once so its report
/// shows what was drawn.
const DRAWS: &str = r#"module Test.Sample exposing (tests)

type Tree = Leaf | Node Float (List Tree)
  deriving Show, Schema

refined type Digit = Int where fun n -> n >= 0 && n < 10
  deriving Show, Schema

type alias Item = {
  name : String;
  tags : List String;
  count : Option BigInt;
  done : Result String Bool;
  pair : (Bool, Float);
  tree : Tree;
} deriving Schema

digits : List Digit
let digits = List.filter_map (fun n -> Result.to_option (Digit.parse n)) (List.range 0 10)

drawn : Show a => String -> Schema a -> Test.Test
let drawn name witness = Test.property name (Schema.arbitrary witness) (fun v -> Test.equal "" (show v))

tests : List Test.Test
let tests =
  [
    drawn "items" (schema Item),
    drawn "trees" (schema Tree),
    drawn "generated" (Schema.with_generator (Gen.one_of digits) (schema Digit)),
    drawn "secrets" (Schema.redacted Schema.string),
    Test.property "durations" (Schema.arbitrary Schema.duration) (fun _ -> Test.is_true "drawn" true),
  ]
"#;

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn schema_arbitrary_draws_alike_on_go() {
    let report = |name: &str, args: &[&str]| {
        let dir = project(name);
        fs::write(dir.join("test/Test/Sample.fl"), DRAWS).expect("write");
        let out = flick_in(&dir, &[&["test", "--reporter", "json", "--seed", "11"], args].concat());
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        results(&out)["tests"]
            .as_array()
            .expect("tests")
            .iter()
            .map(|t| (t["name"].clone(), t["status"].clone(), t["kind"].clone(), t["message"].clone()))
            .collect::<Vec<_>>()
    };
    let js = report("draws", &[]);
    assert_eq!(js.len(), 5, "{js:?}");
    assert!(js.iter().all(|t| t.1 == "failed"), "{js:?}");
    // the generator a schema with a check needs is Schema.with_generator's
    assert!(js[2].3.as_str().is_some_and(|m| m.starts_with("expected \"\", got \"")), "{js:?}");
    assert_eq!(js[4].2, "defect", "{js:?}");
    let go = report("draws_go", &["--emit", "go"]);
    for (j, g) in js.iter().zip(&go).take(4) {
        assert_eq!(j, g);
    }
    assert_eq!((&go[4].1, &go[4].2), (&js[4].1, &js[4].2));
}
