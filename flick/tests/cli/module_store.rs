//! The project's own modules saved between processes: a check or build
//! takes a module from the saved ones while everything its checking reads
//! stays the same, and checks it again otherwise.

use crate::common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::project;

const APP: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"
";

// what the compiler's own plugin derives is part of the text a key holds
const BASE: &str = "module App.Base exposing (x, Point(..))\n\ntype Point = Point Float Float\n  deriving Eq, Show, Schema\n\nx : Float\nlet x = 1.0\n";

const MAIN: &str = "module App.Main exposing (main)

import App.Base exposing (x)

main : Unit ->{Host.Console; AppError} Unit
let main () = Platform.stdout_line (show (x + 1.0))
";

/// `flick args` in `root/app` with the cache directory of `root`: the exit
/// code, the output, and how many modules the run checked.
fn flick(root: &Path, args: &[&str]) -> (Option<i32>, String, usize) {
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("--timings")
        .args(args)
        .current_dir(root.join("app"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("NO_COLOR", "1")
        .output()
        .expect("flick runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let checked = (stderr.lines())
        .filter_map(|l| {
            l.strip_prefix("Slowest of ")?
                .strip_suffix(" modules checked")
        })
        .map(|n| n.parse::<usize>().expect("a count"))
        .sum();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    (out.status.code(), format!("{stdout}{stderr}"), checked)
}

/// A project of two modules in a directory of its own, `name`.
fn app(name: &str) -> PathBuf {
    project(
        name,
        &[
            ("app/flick.toml", APP),
            ("app/src/App/Base.fl", BASE),
            ("app/src/App/Main.fl", MAIN),
        ],
    )
}

#[test]
fn a_saved_module_serves_the_next_process_until_what_it_read_changes() {
    let root = app("module-store");
    let base = root.join("app/src/App/Base.fl");
    let check = |what: &str, status: i32, checked: usize| {
        let (code, out, count) = flick(&root, &["check"]);
        assert_eq!((code, count), (Some(status), checked), "{what}: {out}");
        out
    };
    check("the first", 0, 2);
    check("nothing changed", 0, 0);
    fs::write(&base, BASE.replace("x = 1.0", "x = 2.0")).expect("write");
    check("a body, the interface as it was", 0, 1);
    let string = BASE.replace("x : Float", "x : String");
    fs::write(&base, string.replace("x = 1.0", "x = \"one\"")).expect("write");
    let out = check("another type of an export", 1, 2);
    assert!(out.contains("Main.fl"), "the importer reports it: {out}");
    fs::write(&base, BASE).expect("write");
    check("the text saved before", 0, 0);

    // what cannot be read back is checked again
    let saved = root.join("cache/flick/modules");
    for entry in fs::read_dir(&saved).expect("saved modules") {
        fs::write(entry.expect("an entry").path(), "{").expect("write");
    }
    check("every saved module unreadable", 0, 2);
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn a_build_takes_the_modules_saved_with_their_ir() {
    let root = app("module-store-run");
    assert_eq!(flick(&root, &["check"]).2, 2);
    // a check lowers nothing, so a build takes none of its modules; a later
    // build, after an edit, takes the others with their IR
    let (code, out, count) = flick(&root, &["run"]);
    assert_eq!((code, count), (Some(0), 2), "{out}");
    assert!(out.contains("2\n"), "{out}");
    let main = root.join("app/src/App/Main.fl");
    fs::write(&main, MAIN.replace("x + 1.0", "x + 2.0")).expect("write");
    let (code, out, count) = flick(&root, &["run"]);
    assert_eq!((code, count), (Some(0), 1), "{out}");
    assert!(out.contains("3\n"), "{out}");
}

/// What `--timings` says the module cache did with each module of a check,
/// as `module: reason`, in the order of their names.
fn reasons(root: &Path) -> Vec<String> {
    let (_, out, _) = flick(root, &["check", "--diagnostics", "json"]);
    let timings: serde_json::Value = (out.lines())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| event["type"] == "timings")
        .expect("a timings event");
    (timings["module-cache"]
        .as_array()
        .expect("the module cache"))
    .iter()
    .map(|m| {
        format!(
            "{}: {}",
            m["module"].as_str().unwrap_or(""),
            m["reason"].as_str().unwrap_or("")
        )
    })
    .collect()
}

#[test]
fn timings_say_why_each_module_was_checked_again() {
    let root = app("module-store-reasons");
    let base = root.join("app/src/App/Base.fl");
    assert_eq!(
        reasons(&root),
        ["App.Base: no earlier entry", "App.Main: no earlier entry"]
    );
    assert_eq!(
        reasons(&root),
        [
            "App.Base: taken from the store",
            "App.Main: taken from the store"
        ]
    );
    fs::write(&base, BASE.replace("x = 1.0", "x = 2.0")).expect("write");
    assert_eq!(
        reasons(&root),
        ["App.Base: source", "App.Main: taken from the store"]
    );
    let exported = BASE.replace("exposing (x,", "exposing (x, y,");
    fs::write(&base, format!("{exported}\ny : Float\nlet y = 2.0\n")).expect("write");
    assert_eq!(
        reasons(&root),
        ["App.Base: source", "App.Main: import App.Base ← source"]
    );
}
