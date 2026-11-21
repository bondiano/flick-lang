//! `suite` and `test` blocks under `flick test`: a module of blocks is found
//! without naming `tests` in its `exposing`, runs beside a module that
//! builds its tests by hand, and `.skip`, `.only`, `--list` and
//! `--forbid-only` select its cases, on both targets and on Flick's own
//! runtime.

use crate::common;

use std::fs;
use std::path::{Path, PathBuf};

use common::flick_in;

const BLOCKS: &str = r#"module Test.Blocks

suite "Strings" {
  test "trims" {
    let text = String.trim "  Ada  "
    Platform.stdout_line "ran trims"
    Test.check (Test.equal "Ada" text)
  }

  suite.skip "later" {
    test "never" { Test.check (Test.equal 1 2) }
  }

  suite "nested" {
    test "adds" { Test.check (Test.equal 2 (1 + 1)) }
  }
}

test "alone" {}
"#;

const BY_HAND: &str = r#"module Test.ByHand exposing (tests)

tests : List Test.Test
let tests = [Test.test "by hand" (fun () -> Test.equal 1 1)]
"#;

fn project(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../target/tmp/{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("test/Test")).expect("dir");
    fs::create_dir_all(dir.join("src")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[tools]\n\"flick/go\" = [\"go\"]\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{ \"name\": \"@acme/blocks\" }\n").expect("write");
    fs::write(dir.join("test/Test/Blocks.fl"), BLOCKS).expect("write");
    fs::write(dir.join("test/Test/ByHand.fl"), BY_HAND).expect("write");
    dir
}

/// The flags of a run on `emit`: `js` (Effect adapter), `go` or Flick's own runtime, `rt`.
fn on(emit: &str) -> Vec<&str> {
    match emit {
        "rt" => vec!["--target", "js", "--runtime", "rt"],
        "js" => vec!["--target", "js", "--runtime", "effect"],
        _ => vec!["--target", emit],
    }
}

/// Whether the run exited 0, 1 for a failed one, and what it printed.
fn run(dir: &Path, emit: &str, flags: &[&str]) -> (Option<i32>, String) {
    let out = flick_in(dir, &[&["test"], &on(emit)[..], flags].concat());
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    (out.status.code(), stdout)
}

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn blocks_run_as_cases_and_skip_lists_and_filters_select_them() {
    let dir = project("test_blocks");
    for emit in ["js", "go", "rt"] {
        let (ok, stdout) = run(&dir, emit, &[]);
        assert_eq!(ok, Some(0), "{emit}: {stdout}");
        assert!(stdout.contains("ran trims"), "{emit}: {stdout}");
        assert!(
            stdout.contains("Test.Blocks > Strings > nested > adds"),
            "{emit}: {stdout}"
        );
        assert!(
            stdout.contains("- Test.Blocks > Strings > later > never  (skipped)"),
            "{emit}: {stdout}"
        );
        assert!(
            stdout.contains("4 passed · 0 failed · 1 skipped · 5 total"),
            "{emit}: {stdout}"
        );
        // the names a run would run, none of them run
        let (ok, listed) = run(&dir, emit, &["--list"]);
        assert_eq!(ok, Some(0), "{emit}: {listed}");
        assert_eq!(
            listed,
            "Test.Blocks > Strings > trims\nTest.Blocks > Strings > nested > adds\n\
             Test.Blocks > alone\nTest.ByHand > by hand\n",
            "{emit}"
        );
        let (ok, filtered) = run(&dir, emit, &["--list", "--filter", "Strings > nested"]);
        assert_eq!(ok, Some(0), "{emit}: {filtered}");
        assert_eq!(
            filtered, "Test.Blocks > Strings > nested > adds\n",
            "{emit}"
        );
        let (ok, json) = run(&dir, emit, &["--reporter", "json", "--seed", "1"]);
        assert_eq!(ok, Some(0), "{emit}: {json}");
        assert!(
            json.contains(r#""passed":4,"failed":0,"skipped":1,"#),
            "{emit}: {json}"
        );
        assert!(
            json.contains(r#"{"name":"Test.Blocks > Strings > later > never","status":"skipped"}"#),
            "{emit}: {json}"
        );
    }
}

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn only_keeps_the_run_to_its_cases_and_forbid_only_fails_the_run() {
    let dir = project("test_blocks_only");
    let only = BLOCKS.replace("test \"alone\" {}", "test.only \"alone\" {}");
    fs::write(dir.join("test/Test/Blocks.fl"), only).expect("write");
    for emit in ["js", "go", "rt"] {
        let (ok, stdout) = run(&dir, emit, &[]);
        assert_eq!(ok, Some(0), "{emit}: {stdout}");
        assert!(!stdout.contains("ran trims"), "{emit}: {stdout}");
        assert!(
            stdout.contains("1 passed · 0 failed · 4 skipped · 5 total"),
            "{emit}: {stdout}"
        );
        let (ok, listed) = run(&dir, emit, &["--list"]);
        assert_eq!(ok, Some(0), "{emit}: {listed}");
        assert_eq!(listed, "Test.Blocks > alone\n", "{emit}");
        let (ok, forbidden) = run(&dir, emit, &["--forbid-only"]);
        assert_eq!(ok, Some(1), "{emit}: {forbidden}");
        assert_eq!(
            forbidden, "--forbid-only: 1 test is marked only\n  Test.Blocks > alone\n",
            "{emit}"
        );
    }
}
