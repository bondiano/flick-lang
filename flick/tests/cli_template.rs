//! `flick new --template cli`: a project whose `main` is a `cli` block.
//! Running it needs Node and the workspace's runtime: that test is ignored
//! by default and part of `just e2e`.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{flick_in, project};

/// A fresh `cli` project `name` in the target's temporary directory.
fn cli_project(name: &str) -> PathBuf {
    let root = project(name, &[]);
    fs::create_dir_all(&root).expect("dir");
    let out = flick_in(&root, &["new", "greeter", "--template", "cli"]);
    assert!(out.status.success(), "{out:?}");
    root.join("greeter")
}

#[test]
fn the_cli_template_checks_and_is_formatted() {
    let dir = cli_project("cli_template_check");
    for args in [&["check", "--warnings-as-errors"][..], &["fmt", "--check"]] {
        let out = flick_in(&dir, args);
        assert!(out.status.success(), "{args:?} failed: {out:?}");
    }
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_cli_template_tests_prints_its_help_and_runs_its_handler() {
    let dir = cli_project("cli_template_run");
    let tests = flick_in(&dir, &["test"]);
    assert!(tests.status.success(), "{tests:?}");
    let help = flick_in(&dir, &["run", "--", "--help"]);
    assert!(help.status.success(), "{help:?}");
    let text = String::from_utf8_lossy(&help.stdout);
    for needle in [
        "greeter [flags] [<name>]",
        "--loud",
        "Print in capitals",
        "Who to greet",
    ] {
        assert!(text.contains(needle), "the help lacks {needle}: {text}");
    }
    let out = flick_in(&dir, &["run", "--", "--loud", "Ada"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "HELLO, ADA!\n",
        "{out:?}"
    );
}
