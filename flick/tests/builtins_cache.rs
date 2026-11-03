//! The saved interfaces of the built-in modules follow the sources of
//! `std`: another `std` is checked again, not read from the cache.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::project;

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("dir");
    for entry in fs::read_dir(from).expect("dir") {
        let path = entry.expect("entry").path();
        let target = to.join(path.file_name().expect("name"));
        if path.is_dir() {
            copy(&path, &target);
        } else {
            fs::copy(&path, &target).expect("copy");
        }
    }
}

fn check(dir: &Path, std: &Path, cache: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["check", "src/App/Main.fl"])
        .current_dir(dir)
        .env("FLICK_STD_PATH", std)
        .env("XDG_CACHE_HOME", cache)
        .output()
        .expect("flick runs")
}

#[test]
fn a_changed_std_is_checked_again() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("builtins_cache");
    let _ = fs::remove_dir_all(&root);
    let std = root.join("std");
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std"),
        &std,
    );
    let cache = root.join("cache");
    let main = "module App.Main exposing (x)\n\nx : String\nlet x = Markdown.shout \"hi\"\n";
    let app = project(
        "builtins_cache/app",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
            ),
            ("src/App/Main.fl", main),
        ],
    );
    let before = check(&app, &std, &cache);
    assert!(
        !before.status.success(),
        "`Markdown.shout` does not exist yet"
    );
    let markdown = std.join("src/Markdown.fl");
    let text = fs::read_to_string(&markdown).expect("Markdown.fl");
    let text = text.replacen("exposing (", "exposing (shout, ", 1)
        + "\nshout : String -> String\nlet shout s = s ++ \"!\"\n";
    fs::write(&markdown, text).expect("write");
    let after = check(&app, &std, &cache);
    assert!(after.status.success(), "{after:?}");
}
