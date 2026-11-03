//! Workspaces: commands at the root go through the members, `--package`
//! picks one, a path dependency is compiled from its sources and rebuilt
//! when they change, and `flick run` runs the steps of `[scripts]`.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{flick_in, project};

const ROOT: &str = "schema-version = 1

[workspace]
members = [\"app\", \"lib\"]

[scripts]
hello = \"echo hello \\\"$@\\\"\"
both = [\"@hello\", \"echo second\"]
fail = [\"exit 3\", \"echo never\"]
spin = [\"@spin\"]
";

const LIB: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n";

const APP: &str = "schema-version = 1
[project]
kind = \"app\"
entries = { main = \"Main\", other = \"Other\" }
[dependencies]
lib = { path = \"../lib\" }
";

fn answer(n: u32) -> String {
    format!("module Shared.Answer exposing (answer)\n\nanswer : Float\nlet answer = {n}\n")
}

const MAIN: &str = "module Main exposing (main)

import Shared.Answer

main : Unit ->{} Unit
let main () = Console.log (show Shared.Answer.answer)
";

const OTHER: &str = "module Other exposing (main)

main : Unit ->{} Unit
let main () = Console.log \"other\"
";

fn workspace(name: &str) -> PathBuf {
    project(
        name,
        &[
            ("flick.toml", ROOT),
            ("lib/flick.toml", LIB),
            ("lib/src/Shared/Answer.fl", &answer(42)),
            ("app/flick.toml", APP),
            ("app/src/Main.fl", MAIN),
            ("app/src/Other.fl", OTHER),
        ],
    )
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn the_root_goes_through_the_members_dependencies_first() {
    let dir = workspace("workspace_check");
    let out = flick_in(&dir, &["check"]);
    assert!(out.status.success(), "{out:?}");
    let stderr = text(&out.stderr);
    let (lib, app) = (stderr.find("in `lib`"), stderr.find("in `app`"));
    assert!(lib.is_some() && lib < app, "{stderr}");
    let out = flick_in(&dir, &["check", "--package", "nope"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("FLICK-P031"), "{out:?}");
    let out = flick_in(&dir, &["run"]);
    assert!(text(&out.stderr).contains("--package"), "{out:?}");
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn a_path_dependency_is_rebuilt_when_its_sources_change() {
    let dir = workspace("workspace_run");
    let out = flick_in(&dir, &["run", "-p", "app", "main"]);
    assert_eq!(text(&out.stdout).trim(), "42", "{out:?}");
    fs::write(dir.join("lib/src/Shared/Answer.fl"), answer(43)).expect("write");
    let out = flick_in(dir.join("app"), &["run", "main"]);
    assert_eq!(text(&out.stdout).trim(), "43", "{out:?}");
    let out = flick_in(&dir, &["run", "-p", "app", "other"]);
    assert_eq!(text(&out.stdout).trim(), "other", "{out:?}");
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn scripts_run_their_steps_in_order_until_one_fails() {
    let dir = workspace("workspace_scripts");
    let out = flick_in(dir.join("app"), &["run", "both", "--", "a", "b"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(text(&out.stdout), "hello a b\nsecond\n");
    let out = flick_in(&dir, &["run", "fail"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(!text(&out.stdout).contains("never"));
    let out = flick_in(&dir, &["run", "spin"]);
    assert!(text(&out.stderr).contains("run each other"), "{out:?}");
    // an entry of the project wins over a script of the same name
    let out = flick_in(dir.join("app"), &["run", "other"]);
    assert_eq!(text(&out.stdout).trim(), "other", "{out:?}");
}
