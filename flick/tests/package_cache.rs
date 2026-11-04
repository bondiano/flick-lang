//! The modules of libraries saved between runs: a check or build takes a
//! library from the saved modules while its sources, the compiler and the
//! project's configuration stay the same, and checks it again otherwise.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::project;

const APP: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[dependencies]
geo = DEPENDENCY
";

const FETCHED: &str = "{ git = \"https://example.com/geo\", tag = \"v1.0.0\", subdir = \"lib\" }";

const MAIN: &str = "module App.Main exposing (main, start)

import Geo exposing (origin, shifted)

start : Float
let start = (shifted origin).x

main : Unit ->{; AppError} Unit
let main () = Platform.stdout_line (show start)
";

const LIB: &str = "schema-version = 1

[package]
name = \"geo\"
version = \"1.0.0\"

[project]
kind = \"lib\"
";

const GEO: &str = "module Geo exposing (Point, origin, shifted)

import Geo.Shift exposing (by)

type alias Point = { x : Float; y : Float }

origin : Point
let origin = { x = 0.0; y = 0.0 }

shifted : Point -> Point
let shifted p = { x = by p.x; y = p.y }
";

const SHIFT: &str = "module Geo.Shift exposing (by)

by : Float -> Float
let by x = x + 1.0
";

const BROKEN: &str = "module Geo exposing (origin)\n\nlet origin = \n";

const HASH: &str = "4f0e0bd5b9b8b1f2a0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a3b2c1d0e9f8a7";

/// A project using the fetched library `geo`, its package store and a cache
/// directory, all under `name`.
fn fetched(name: &str) -> (PathBuf, PathBuf) {
    let source = "git+https://example.com/geo?tag=v1.0.0&subdir=lib#0123abcd";
    let lock = format!(
        "version = 1\n\n[[package]]\nname = \"geo\"\nversion = \"1.0.0\"\nsource = \"{source}\"\nchecksum = \"sha256:{HASH}\"\n"
    );
    let root = project(
        name,
        &[
            ("app/flick.toml", &APP.replace("DEPENDENCY", FETCHED)),
            ("app/src/App/Main.fl", MAIN),
            ("app/flick.lock", &lock),
            (&format!("store/{HASH}/lib/flick.toml"), LIB),
            (&format!("store/{HASH}/lib/src/Geo.fl"), GEO),
            (&format!("store/{HASH}/lib/src/Geo/Shift.fl"), SHIFT),
        ],
    );
    let geo = root.join(format!("store/{HASH}/lib/src/Geo.fl"));
    (root, geo)
}

/// `flick args` in `root/app` with the store and cache of `root`, or the
/// cache `cache`; the exit code.
fn flick(root: &Path, cache: &str, args: &[&str]) -> Option<i32> {
    flick_with(Path::new(env!("CARGO_BIN_EXE_flick")), root, cache, args)
}

fn flick_with(binary: &Path, root: &Path, cache: &str, args: &[&str]) -> Option<i32> {
    run(binary, root, cache, args).0
}

/// [`flick`], also with its stderr.
fn flick_output(root: &Path, cache: &str, args: &[&str]) -> (Option<i32>, String) {
    run(Path::new(env!("CARGO_BIN_EXE_flick")), root, cache, args)
}

fn run(binary: &Path, root: &Path, cache: &str, args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(binary)
        .args(args)
        .current_dir(root.join("app"))
        .env("FLICK_STORE", root.join("store"))
        .env("XDG_CACHE_HOME", root.join(cache))
        .env("NO_COLOR", "1")
        .output()
        .expect("flick runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_saved_library_is_not_checked_again() {
    let (root, geo) = fetched("package-cache-hit");
    assert_eq!(flick(&root, "cache", &["check"]), Some(0));
    // the sources of a fetched library never change in the store: had the
    // runs below read them, these would fail
    fs::write(&geo, BROKEN).expect("write");
    assert_eq!(flick(&root, "other-cache", &["check"]), Some(1));
    assert_eq!(flick(&root, "cache", &["check"]), Some(0));
    // a build compiles the saved module in its file, which must be the text
    // it was checked from: here it says so instead of reading the change
    let (status, stderr) = flick_output(&root, "cache", &["build", "--out-dir", "out"]);
    assert_eq!(status, Some(1));
    assert!(
        stderr.contains("FLICK-P034") && stderr.contains("changed since"),
        "{stderr}"
    );
}

#[test]
fn a_build_from_saved_modules_is_the_build_from_sources() {
    let (root, _) = fetched("package-cache-same");
    let main = root.join("app/src/App/Main.fl");
    assert_eq!(
        flick(&root, "cache", &["build", "--out-dir", "first"]),
        Some(0)
    );
    // a changed module of the project misses the cache of build artifacts
    fs::write(&main, format!("{MAIN}\n// changed\n")).expect("write");
    assert_eq!(
        flick(&root, "cache", &["build", "--out-dir", "saved"]),
        Some(0)
    );
    fs::remove_dir_all(root.join("app/.flick")).expect("artifacts cached");
    assert_eq!(
        flick(&root, "fresh", &["build", "--out-dir", "checked"]),
        Some(0)
    );
    let (saved, checked) = (
        files(&root.join("app/saved")),
        files(&root.join("app/checked")),
    );
    assert!(!saved.is_empty());
    assert_eq!(saved, checked);
}

#[test]
fn a_changed_library_project_or_compiler_checks_again() {
    let (root, geo) = fetched("package-cache-miss");
    assert_eq!(flick(&root, "cache", &["check"]), Some(0));
    fs::write(&geo, BROKEN).expect("write");
    // another binary of the compiler
    let copy = root.join("flick-copy");
    fs::copy(env!("CARGO_BIN_EXE_flick"), &copy).expect("copy");
    assert_eq!(flick_with(&copy, &root, "cache", &["check"]), Some(1));
    // the project's configuration
    let config = root.join("app/flick.toml");
    let changed = APP
        .replace("DEPENDENCY", FETCHED)
        .replace("kind = \"app\"", "kind = \"app\"\ntest-dir = \"spec\"");
    fs::write(&config, changed).expect("write");
    assert_eq!(flick(&root, "cache", &["check"]), Some(1));

    // the sources of a path library
    let root = project(
        "package-cache-path",
        &[
            (
                "app/flick.toml",
                &APP.replace("DEPENDENCY", "{ path = \"../lib\" }"),
            ),
            ("app/src/App/Main.fl", MAIN),
            ("lib/flick.toml", LIB),
            ("lib/src/Geo.fl", GEO),
            ("lib/src/Geo/Shift.fl", SHIFT),
        ],
    );
    assert_eq!(flick(&root, "cache", &["check"]), Some(0));
    fs::write(root.join("lib/src/Geo.fl"), BROKEN).expect("write");
    assert_eq!(flick(&root, "cache", &["check"]), Some(1));
}

/// The files under `dir` with their contents, by relative path.
fn files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(at) = pending.pop() {
        for entry in fs::read_dir(&at).expect("readable").flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let rel = path.strip_prefix(dir).expect("under dir").to_path_buf();
                out.push((rel, fs::read(&path).expect("readable")));
            }
        }
    }
    out.sort();
    out
}
