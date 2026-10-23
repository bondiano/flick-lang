//! The `examples/first-slice` project: services with a test and an external
//! implementation, layers provided in `main`, and two loads in parallel.
//! Each test works on a copy in the target directory, which resolves
//! `@flick-lang/runtime` from the workspace and keeps the example clean.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::flick_in;

/// The `[log]` lines of a run, sorted: the parallel loads may finish in either order.
const LOGS: [&str; 2] = ["[log] Loaded Ada Lovelace", "[log] Loaded Grace Hopper"];

/// The other lines of a run, in order.
const RESULTS: [&str; 2] = ["Ada Lovelace and Grace Hopper", "not found: \"u3\""];

/// Copies `from` into `to`, leaving out build output and installed packages.
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("dir");
    for entry in fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name();
        if ["dist", ".flick", "node_modules"].contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(&name));
        } else {
            fs::copy(&path, to.join(&name)).expect("copy");
        }
    }
}

fn example(name: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/first-slice");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("first_slice")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    copy(&source, &dir);
    dir
}

fn success(dir: &Path, args: &[&str]) -> String {
    let out = flick_in(dir, args);
    assert!(out.status.success(), "{args:?} failed: {out:?}");
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

#[test]
fn the_first_slice_checks_and_is_formatted() {
    let dir = example("check");
    success(&dir, &["check", "--warnings-as-errors"]);
    success(&dir, &["fmt", "--check"]);
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_first_slice_tests_and_runs_in_both_profiles() {
    let dir = example("run");
    for profile in ["debug", "release"] {
        let tests = success(&dir, &["test", "--profile", profile]);
        assert!(tests.contains("3 passed, 0 failed"), "{profile}: {tests}");
        // the pair keeps the order of its tasks whichever load answers first
        let run = success(&dir, &["run", "--profile", profile]);
        let (mut logs, results): (Vec<&str>, Vec<&str>) =
            run.lines().partition(|line| line.starts_with("[log] "));
        logs.sort_unstable();
        assert_eq!(logs, LOGS, "{profile}: {run}");
        assert_eq!(results, RESULTS, "{profile}: {run}");
    }
}
