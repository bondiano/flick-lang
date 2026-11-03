//! Dependencies fetched from git or a tarball: `flick.lock` pins them, the
//! package store holds their files, and a project compiles their sources
//! like those of a path dependency.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::project;

const APP: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[dependencies]
geo = { git = \"https://example.com/geo\", tag = \"v1.0.0\", subdir = \"lib\" }
";

const MAIN: &str = "module App.Main exposing (main, start)

import Geo exposing (origin)

start : Float
let start = origin.x

main : Unit -> Unit
let main () = ()
";

const LIB: &str = "schema-version = 1

[package]
name = \"geo\"
version = \"1.0.0\"
flick = \"REQUIREMENT\"

[project]
kind = \"lib\"
";

const GEO: &str = "module Geo exposing (Point, origin)

type alias Point = { x : Float; y : Float }

origin : Point
let origin = { x = 0.0; y = 0.0 }
";

const HASH: &str = "4f0e0bd5b9b8b1f2a0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a3b2c1d0e9f8a7";

fn lock(source: &str) -> String {
    format!(
        "version = 1\n\n[[package]]\nname = \"geo\"\nversion = \"1.0.0\"\nsource = \"{source}\"\nchecksum = \"sha256:{HASH}\"\n"
    )
}

/// `flick check` in `dir` with the package store `store`, as NDJSON codes.
fn check(dir: &Path, store: &Path) -> (Option<i32>, Vec<String>) {
    let out: Output = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["check", "--diagnostics", "json"])
        .current_dir(dir)
        .env("FLICK_STORE", store)
        .output()
        .expect("flick runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let codes = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|v| v["code"].as_str().map(str::to_owned))
        .collect();
    (out.status.code(), codes)
}

/// A store with the fetched repository of `geo`, which needs `requirement`.
fn store(name: &str, requirement: &str) -> PathBuf {
    let lib = LIB.replace("REQUIREMENT", requirement);
    let files = [
        (format!("{HASH}/lib/flick.toml"), lib),
        (format!("{HASH}/lib/src/Geo.fl"), GEO.to_owned()),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    project(name, &files)
}

#[test]
fn a_locked_git_dependency_compiles_from_the_store() {
    let store = store("fetched-store", "*");
    let source = "git+https://example.com/geo?tag=v1.0.0&subdir=lib#0123abcd";
    let app = project(
        "fetched-app",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("flick.lock", &lock(source)),
        ],
    );
    assert_eq!(check(&app, &store), (Some(0), Vec::new()));
}

#[test]
fn a_dependency_missing_from_the_lock_is_reported() {
    let store = store("fetched-store-other-tag", "*");
    let source = "git+https://example.com/geo?tag=v0.9.0&subdir=lib#0123abcd";
    let app = project(
        "fetched-app-other-tag",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("flick.lock", &lock(source)),
        ],
    );
    let (status, codes) = check(&app, &store);
    assert_eq!(status, Some(1));
    assert!(codes.contains(&"FLICK-P034".to_owned()), "{codes:?}");
}

#[test]
fn a_library_for_another_compiler_is_reported() {
    let store = store("fetched-store-old", "<0.0.1");
    let source = "git+https://example.com/geo?tag=v1.0.0&subdir=lib#0123abcd";
    let app = project(
        "fetched-app-old",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("flick.lock", &lock(source)),
        ],
    );
    let (status, codes) = check(&app, &store);
    assert_eq!(status, Some(1));
    assert!(codes.contains(&"FLICK-P033".to_owned()), "{codes:?}");
}
