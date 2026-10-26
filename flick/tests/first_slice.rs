//! The `examples/first-slice` project: services with a test and an external
//! implementation, layers provided in `main`, and two loads in parallel.
//! Each test works on a copy in the target directory (`common::example`).

mod common;

use std::path::{Path, PathBuf};

use common::flick_in;

/// The `[log]` lines of a run, sorted: the parallel loads may finish in either order.
const LOGS: [&str; 2] = ["[log] Loaded Ada Lovelace", "[log] Loaded Grace Hopper"];

/// The other lines of a run, in order.
const RESULTS: [&str; 2] = ["Ada Lovelace and Grace Hopper", "not found: \"u3\""];

fn example(name: &str) -> PathBuf {
    common::example("first-slice", &format!("first_slice/{name}"))
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
