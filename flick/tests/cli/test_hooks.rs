//! The hooks of a suite and the timeouts of its cases under `flick test`:
//! the order hooks run in around the selected cases, a failed setup, a case
//! out of time and an interrupted run, whose cleanup still runs, alike on
//! Flick's runtime, through the Effect adapter and on Go.

use crate::common;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use common::{flick_in, project};
use serde_json::Value;

const HOOKS: &str = r#"module App.Test

suite "outer" {
  before_all { Platform.stdout_line "before_all outer" }
  before_each { Platform.stdout_line "before_each outer" }
  after_each { Platform.stdout_line "after_each outer" }
  after_all { Platform.stdout_line "after_all outer" }

  suite "inner" {
    before_all { Platform.stdout_line "before_all inner" }
    before_each { Platform.stdout_line "before_each inner" }
    after_each { Platform.stdout_line "after_each inner" }
    after_all { Platform.stdout_line "after_all inner" }

    test "one" { Platform.stdout_line "one" }
    test "two" { Platform.stdout_line "two" }
  }

  test "three" { Platform.stdout_line "three" }

  suite.skip "skipped" {
    before_all { Platform.stdout_line "skipped ran" }
    test "four" {}
  }
}

suite "setup" {
  before_all { Test.expect (Test.equal 1 2) }
  after_all { Platform.stdout_line "after_all setup" }
  test "never runs" { Platform.stdout_line "never" }
}

suite "slow" {
  after_each { Platform.stdout_line "after_each slow" }
  test "own" timeout 20 { Effect.sleep (minutes 60) }
  test "the run's" { Effect.sleep (minutes 60) }
}

minutes : Float -> Duration.Duration
let minutes n = Result.with_default Duration.zero (Duration.minutes n)
"#;

const INTERRUPTED: &str = r#"module App.Test

suite "long" {
  after_all { Platform.stdout_line "after_all ran" }
  after_each { Platform.stdout_line "after_each ran" }
  test "waits" {
    Platform.stdout_line "started"
    Effect.sleep (Result.with_default Duration.zero (Duration.minutes 60))
  }
}
"#;

/// A library whose tests are `test`, run on `effect`'s engine.
fn library(name: &str, effect: &str, test: &str) -> PathBuf {
    let manifest = format!(
        "schema-version = 1\n\n[tools]\n\"flick/go\" = [\"go\"]\n\n[project]\nkind = \"lib\"\n\n[build]\neffect = \"{effect}\"\n"
    );
    project(
        name,
        &[
            ("flick.toml", &manifest),
            ("package.json", "{ \"name\": \"@acme/hooks\" }\n"),
            ("test/App/Test.fl", test),
        ],
    )
}

/// The engines: Flick's runtime and the Effect adapter on Node, and Go.
const ENGINES: [(&str, &str, &str); 3] = [
    ("rt", "rt", "js"),
    ("effect", "effect", "js"),
    ("go", "rt", "go"),
];

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn hooks_run_around_the_selected_cases_on_every_engine() {
    let each = |body: &str| {
        [
            "before_each outer",
            "before_each inner",
            body,
            "after_each inner",
            "after_each outer",
        ]
        .map(str::to_owned)
    };
    let mut order = vec!["before_all outer".to_owned(), "before_all inner".to_owned()];
    order.extend(each("one"));
    order.extend(each("two"));
    order.extend(
        [
            "after_all inner",
            "before_each outer",
            "three",
            "after_each outer",
            "after_all outer",
            "after_all setup",
            "after_each slow",
            "after_each slow",
        ]
        .map(str::to_owned),
    );
    for (name, effect, emit) in ENGINES {
        let dir = library(&format!("test_hooks_{name}"), effect, HOOKS);
        let out = flick_in(
            &dir,
            &[
                "test",
                "--emit",
                emit,
                "--reporter",
                "json",
                "--timeout",
                "50ms",
            ],
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{name}: {out:?}");
        let (printed, report) = stdout
            .trim_end()
            .rsplit_once('\n')
            .expect("lines and a report");
        assert_eq!(report.lines().count(), 1, "{name}: {stdout}");
        assert_eq!(printed.lines().collect::<Vec<_>>(), order, "{name}");
        let json: Value =
            serde_json::from_str(report).unwrap_or_else(|e| panic!("{name}: {e}: {report}"));
        let results: Vec<String> = (json["tests"].as_array().expect("tests").iter())
            .map(|t| {
                let field = |key: &str| t[key].as_str().unwrap_or_default().to_owned();
                [
                    field("name"),
                    field("status"),
                    field("kind"),
                    field("message"),
                ]
                .join(" | ")
            })
            .collect();
        assert_eq!(
            results,
            [
                "App.Test > outer > inner > one | passed |  | ",
                "App.Test > outer > inner > two | passed |  | ",
                "App.Test > outer > three | passed |  | ",
                "App.Test > setup > never runs | failed | setup | expected 1, got 2",
                "App.Test > slow > own | failed | timeout | timed out after 20 ms",
                "App.Test > slow > the run's | failed | timeout | timed out after 50 ms",
                "App.Test > outer > skipped > four | skipped |  | ",
            ],
            "{name}"
        );
    }
}

/// Sends SIGINT to `flick`, which hands it to the runner.
#[cfg(unix)]
fn interrupt(id: u32) {
    let sent = Command::new("kill")
        .args(["-s", "INT", &id.to_string()])
        .status();
    assert!(sent.is_ok_and(|s| s.success()), "kill");
}

#[cfg(unix)]
#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn an_interrupted_run_still_runs_the_after_hooks_on_every_engine() {
    for (name, effect, emit) in ENGINES {
        let dir = library(
            &format!("test_hooks_interrupted_{name}"),
            effect,
            INTERRUPTED,
        );
        // the run is built first, so that the signal finds it running
        let built = flick_in(&dir, &["test", "--emit", emit, "--list"]);
        assert!(built.status.success(), "{name}: {built:?}");
        let log = dir.join("out.log");
        let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["test", "--emit", emit])
            .current_dir(&dir)
            .stdout(fs::File::create(&log).expect("log"))
            .stderr(Stdio::piped())
            .spawn()
            .expect("flick runs");
        let started = Instant::now();
        while !read(&log).contains("started") {
            assert!(
                started.elapsed() < Duration::from_mins(2),
                "{name}: never started: {}",
                read(&log)
            );
            thread::sleep(Duration::from_millis(50));
        }
        interrupt(child.id());
        let status = child.wait().expect("flick ends");
        let mut stderr = String::new();
        child
            .stderr
            .take()
            .expect("stderr")
            .read_to_string(&mut stderr)
            .expect("stderr");
        let text = read(&log);
        assert_eq!(status.code(), Some(130), "{name}: {text}{stderr}");
        let at = |line: &str| {
            text.find(line)
                .unwrap_or_else(|| panic!("{name}: no {line:?} in {text}"))
        };
        assert!(
            at("started") < at("after_each ran") && at("after_each ran") < at("after_all ran"),
            "{name}: {text}"
        );
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}
