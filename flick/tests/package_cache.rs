//! The modules of libraries saved between runs: a check or build takes a
//! library from the saved modules while its sources, the compiler and the
//! project's configuration stay the same, and checks it again otherwise.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use common::project;
use flick_core::build::go::executable;
use flick_core::build::runner_path;
use flick_core::project::config::MAIN_ENTRY;
use serde_json::{Value, json};

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

/// A point on the plane.
type alias Point = { x : Float; y : Float }

/// Where it starts.
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
fn cached_private_module_stays_private_to_a_dependent() {
    let (root, _) = fetched("package-cache-private");
    let shift = root.join(format!("store/{HASH}/lib/src/Geo/Shift.fl"));
    fs::write(
        &shift,
        SHIFT.replacen("module Geo.Shift", "private module Geo.Shift", 1),
    )
    .expect("private dependency module");
    assert_eq!(flick(&root, "cache", &["check"]), Some(0));

    let main = root.join("app/src/App/Main.fl");
    fs::write(
        &main,
        MAIN.replace(
            "import Geo exposing (origin, shifted)",
            "import Geo exposing (origin, shifted)\nimport Geo.Shift exposing (by)",
        ),
    )
    .expect("import private dependency module");
    for cache in ["cache", "fresh"] {
        let (status, stderr) = flick_output(&root, cache, &["check", "--diagnostics", "json"]);
        assert_eq!(status, Some(1), "{stderr}");
        let diagnostics: Vec<Value> = stderr
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSON diagnostic"))
            .filter(|event: &Value| event["severity"] == "error")
            .collect();
        assert_eq!(diagnostics.len(), 1, "{stderr}");
        assert_eq!(diagnostics[0]["code"], "FLICK-R017");
        assert!(
            Path::new(diagnostics[0]["path"].as_str().expect("source path"))
                .ends_with(Path::new("src").join("App").join("Main.fl")),
            "{stderr}"
        );
        // R017 points at the forbidden module path, not the import keyword
        // or its exposing list.
        let start = MAIN.find("\n\nstart").expect("end of imports") + 1 + "import ".len();
        assert_eq!(
            diagnostics[0]["range"],
            json!({ "start": start, "end": start + "Geo.Shift".len() })
        );
    }
}

#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn installed_private_module_public_facade_runs_on_js_and_go() {
    for emit in ["js", "go"] {
        for profile in ["debug", "release"] {
            let (root, geo) = fetched(&format!("package-private-facade-{emit}-{profile}"));
            fs::write(
                root.join(format!("store/{HASH}/lib/src/Geo/Shift.fl")),
                SHIFT.replacen("module Geo.Shift", "private module Geo.Shift", 1),
            )
            .expect("private implementation");
            // Re-export the private implementation, rather than wrapping it.
            fs::write(
                geo,
                "module Geo exposing (by)\nimport Geo.Shift exposing (by)\n",
            )
            .expect("public facade");
            let main = "module App.Main exposing (main)\nimport Geo exposing (by)\n\
                        main : Unit ->{; AppError} Unit\n\
                        let main () = Platform.stdout_line (show (by 41.0))\n";
            let main_path = root.join("app/src/App/Main.fl");
            for (cache, out) in [("cache", "cold"), ("cache", "saved"), ("fresh", "fresh")] {
                // Force a build-artifact miss: the saved run must lower the
                // package interface, not just copy the first executable.
                fs::write(&main_path, format!("{main}\n// {out}\n")).expect("app source");
                let (status, stderr) = flick_output(
                    &root,
                    cache,
                    &[
                        "build",
                        "--emit",
                        emit,
                        "--profile",
                        profile,
                        "--out-dir",
                        out,
                    ],
                );
                assert_eq!(status, Some(0), "{emit}/{profile}/{out}: {stderr}");
                let dist = root.join("app").join(out);
                let output = if emit == "js" {
                    Command::new("node")
                        .arg(dist.join(runner_path(false)))
                        .output()
                } else {
                    Command::new(dist.join(executable(MAIN_ENTRY))).output()
                }
                .expect("program runs");
                assert!(
                    output.status.success(),
                    "{emit}/{profile}/{out}: {output:?}"
                );
                assert_eq!(output.stdout, b"42\n", "{emit}/{profile}/{out}");
                assert!(output.stderr.is_empty(), "{output:?}");
            }
        }
    }
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
    // APFS can preserve the source timestamp when copying, while the cache
    // fingerprint distinguishes binaries by size and modification time.
    let original = fs::metadata(env!("CARGO_BIN_EXE_flick"))
        .expect("binary metadata")
        .modified()
        .expect("binary modified time");
    let earlier = original
        .checked_sub(Duration::from_secs(1))
        .expect("binary timestamp after epoch");
    fs::File::open(&copy)
        .expect("copied binary")
        .set_times(fs::FileTimes::new().set_modified(earlier))
        .expect("change copied binary timestamp");
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

#[test]
fn the_declarations_of_a_saved_library_keep_its_docs() {
    let (root, _) = fetched("package-cache-docs");
    assert_eq!(flick(&root, "cache", &["check"]), Some(0));
    // the build reads the docs from the saved interface, not the sources
    let cache = root.join("cache");
    let mut saved = 0;
    for (path, bytes) in files(&cache) {
        let text = String::from_utf8(bytes).expect("saved modules are JSON");
        if text.contains("Where it starts.") {
            let text = text.replace("Where it starts.", "Where it starts, as saved.");
            fs::write(cache.join(path), text).expect("write");
            saved += 1;
        }
    }
    assert_eq!(saved, 1);
    let build = ["build", "--emit-dts", "--out-dir", "out"];
    assert_eq!(flick(&root, "cache", &build), Some(0));
    let dts = fs::read_to_string(root.join("app/out/Geo.d.ts")).expect("declarations");
    assert!(
        dts.contains("/** A point on the plane. */\nexport interface Point {"),
        "{dts}"
    );
    assert!(
        dts.contains("/** Where it starts, as saved. */\nexport declare const origin:"),
        "{dts}"
    );
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
