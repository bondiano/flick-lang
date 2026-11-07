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

const PLUGINS: &str = "schema-version = 1

[workspace]
members = [\"lib\", \"pairs\"]

[[workspace.plugins]]
package = \"pairs\"
source = { path = \"pairs\" }
";

#[test]
fn a_library_that_gets_a_workspace_plugin_is_not_packed_without_it() {
    let lib = "schema-version = 1\n[package]\nname = \"lib\"\nversion = \"1.0.0\"\n[project]\nkind = \"lib\"\n";
    let pairs = "schema-version = 1\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\n[project]\nkind = \"lib\"\n";
    let root = project(
        "workspace-plugin-pack",
        &[
            ("flick.toml", PLUGINS),
            ("lib/flick.toml", lib),
            ("lib/src/Shared/Answer.fl", &answer(1)),
            ("pairs/flick.toml", pairs),
            (
                "pairs/src/Pairs.fl",
                "module Pairs exposing (one)\n\none : Float\nlet one = 1\n",
            ),
        ],
    );
    let out = root.join("packed");
    let packed = flick_in(
        root.join("lib"),
        &["pack", "--out-dir", &out.to_string_lossy()],
    );
    assert!(!packed.status.success(), "{packed:?}");
    let stderr = text(&packed.stderr);
    assert!(
        stderr.contains("FLICK-P032") && stderr.contains("`pairs`"),
        "{stderr}"
    );
    // the plugin's own package gets nothing from it, and packs
    let own = flick_in(
        root.join("pairs"),
        &["pack", "--out-dir", &out.to_string_lossy()],
    );
    assert!(own.status.success(), "{own:?}");
}

#[test]
fn install_pins_the_build_of_a_local_plugin_and_locked_refuses_another() {
    let app = "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\npairs = { path = \"../pairs\" }\n";
    let pairs = "schema-version = 1\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\n[project]\nkind = \"lib\"\n";
    let root = project(
        "workspace-plugin-lock",
        &[
            ("app/flick.toml", app),
            ("app/src/Shared/Answer.fl", &answer(1)),
            ("pairs/flick.toml", pairs),
            (
                "pairs/src/Pairs.fl",
                "module Pairs exposing (one)\n\none : Float\nlet one = 1\n",
            ),
            ("pairs/pairs.wasm", "a first build"),
        ],
    );
    let app = root.join("app");
    let installed = flick_in(&app, &["install"]);
    assert!(installed.status.success(), "{installed:?}");
    let lock = fs::read_to_string(app.join("flick.lock")).expect("the lock");
    assert!(
        lock.contains("[[plugin]]\nname = \"pairs\"\ncomponent = \"../pairs/pairs.wasm\""),
        "{lock}"
    );
    assert!(flick_in(&app, &["install", "--locked"]).status.success());
    fs::write(root.join("pairs/pairs.wasm"), "another build").expect("rebuild");
    let refused = flick_in(&app, &["install", "--locked"]);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        text(&refused.stderr).contains("the component of the plugin of `pairs`"),
        "{}",
        text(&refused.stderr)
    );
    // a check is not held to the lock
    assert!(flick_in(&app, &["check"]).status.success());
    assert!(flick_in(&app, &["install"]).status.success());
    let lock = fs::read_to_string(app.join("flick.lock")).expect("the lock");
    assert!(
        flick_in(&app, &["install", "--locked"]).status.success(),
        "{lock}"
    );
}

#[test]
fn flick_plugins_shows_where_each_comes_from_and_its_build() {
    let root_manifest = PLUGINS.replace(
        "members = [\"lib\", \"pairs\"]",
        "members = [\"app\", \"pairs\"]",
    );
    let pairs = "schema-version = 1\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\n[project]\nkind = \"lib\"\n";
    let root = project(
        "workspace-plugins-list",
        &[
            ("flick.toml", &root_manifest),
            ("app/flick.toml", LIB),
            ("app/src/Shared/Answer.fl", &answer(1)),
            ("pairs/flick.toml", pairs),
            (
                "pairs/src/Pairs.fl",
                "module Pairs exposing (one)\n\none : Float\nlet one = 1\n",
            ),
        ],
    );
    let app = root.join("app");
    let listed = |dir: &PathBuf| {
        let out = flick_in(dir, &["plugins"]);
        assert!(out.status.success(), "{out:?}");
        text(&out.stdout)
    };
    let before = listed(&app);
    assert!(
        before.starts_with("std: linked into flick, blocks cli"),
        "{before}"
    );
    assert!(
        before.contains("pairs (from the workspace): ../pairs/pairs.wasm, missing"),
        "{before}"
    );
    fs::write(root.join("pairs/pairs.wasm"), "a build").expect("build");
    assert!(listed(&app).contains("not in `flick.lock`"));
    assert!(flick_in(&app, &["install"]).status.success());
    assert!(listed(&app).contains(", pinned"));
    fs::write(root.join("pairs/pairs.wasm"), "another build").expect("rebuild");
    assert!(listed(&app).contains("another build than `flick.lock` pins"));
}
