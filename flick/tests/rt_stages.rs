//! On Flick's own runtime a stage follows the classes of the operations of its
//! row, whichever way the class reaches the checker: from a module checked in
//! this run or from the interface of an earlier one, and the two modes of a
//! build do not read what the other one saved. Ignored by default: part of
//! `just e2e`, needs Node and `pnpm install`.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{APP, flick_in, project};
use flick_core::build::runner_path;

const LIB_CONFIG: &str = "schema-version = 1

[package]
name = \"ctl\"
version = \"1.0.0\"

[project]
kind = \"lib\"
";

const LIB: &str = "module Ctl exposing (Ask, twice, plain)

effect Ask =
  control ask : Int ->{} Int

twice : Unit ->{Ask;} Int
let twice () = Ask.ask 1 + Ask.ask 2

plain : Int ->{} Int
let plain x = x + 1
";

const LIB_MAIN: &str = "module App.Main exposing (main)

import Ctl exposing (Ask, twice)

use : Unit ->{Ask;} Int
let use () = twice () + Ask.ask 3

main : Unit ->{Async; AppError} Unit
let main () =
  let r =
    try use () with
    | Ask.ask x -> resume (x * 10)
  Platform.stdout_line (show r)
";

/// Without a handler of a `control` operation, which the default mode cannot compile.
const PLAIN_MAIN: &str = "module App.Main exposing (main, plain_too)

import Ctl exposing (plain)

plain_too : Int ->{} Int
let plain_too x = plain x

main : Unit ->{Async; AppError} Unit
let main () = Platform.stdout_line (show (plain_too 1))
";

const CTL: &str = "module App.Ctl exposing (Ask, twice)

effect Ask =
  control ask : Int ->{} Int

twice : Unit ->{Ask;} Int
let twice () = Ask.ask 1 + Ask.ask 2
";

const CTL_MAIN: &str = "module App.Main exposing (main)

import App.Ctl exposing (Ask, twice)

use : Unit ->{Ask;} Int
let use () = twice () + Ask.ask 3

main : Unit ->{Async; AppError} Unit
let main () =
  let r =
    try use () with
    | Ask.ask x -> resume (x * 10)
  Platform.stdout_line (show r)
";

/// `flick build args` in `dir`, with the cache directory beside it, so what one
/// build saves the next one reads.
fn build(dir: &Path, args: &[&str]) {
    let cache = dir.parent().expect("a parent").join("cache");
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("build")
        .args(args)
        .current_dir(dir)
        .env("XDG_CACHE_HOME", cache)
        .output()
        .expect("flick runs");
    assert!(out.status.success(), "{args:?}: {out:?}");
}

/// What the runner in `dist` of `dir` prints; it must exit cleanly.
fn run(dir: &Path) -> String {
    let out = Command::new("node")
        .arg(Path::new("dist").join(runner_path(false)))
        .current_dir(dir)
        .output()
        .expect("node runs");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

fn append(path: &Path, line: &str) {
    let text = fs::read_to_string(path).expect("read");
    fs::write(path, format!("{text}{line}\n")).expect("write");
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_new_project_builds_runs_and_tests_on_rt_without_effect() {
    let root = project("rt_stages/new_default", &[]);
    fs::create_dir_all(&root).expect("directory");
    let created = flick_in(&root, &["new", "hello"]);
    assert!(created.status.success(), "{created:?}");
    let app = root.join("hello");
    let manifest = fs::read_to_string(app.join("package.json")).expect("manifest");
    assert!(manifest.contains("@flick-lang/runtime"), "{manifest}");
    assert!(!manifest.contains("@flick-lang/effect-ts"), "{manifest}");
    assert!(!manifest.contains("\"effect\""), "{manifest}");

    build(&app, &[]);
    let js = fs::read_to_string(app.join("dist/App/Main.js")).expect("compiled module");
    assert!(js.contains("@flick-lang/runtime/rt"), "{js}");
    assert!(!js.contains("@flick-lang/effect-ts"), "{js}");
    assert_eq!(run(&app), "Hello, world!\n");

    let tested = flick_in(&app, &["test", "--reporter", "json"]);
    assert!(tested.status.success(), "{tested:?}");
    let report: serde_json::Value = serde_json::from_slice(&tested.stdout).expect("test report");
    assert!(
        report["tests"]
            .as_array()
            .is_some_and(|tests| !tests.is_empty())
    );

    let go = flick_in(&app, &["test", "--emit", "go", "--reporter", "json"]);
    assert!(
        go.status.success(),
        "the Go target still runs its own runtime: {go:?}"
    );

    let declarations = flick_in(&app, &["build", "--emit-dts", "--out-dir", "typed"]);
    assert!(declarations.status.success(), "{declarations:?}");
    assert!(app.join("typed/App/Main.d.ts").is_file());
    let declaration = fs::read_to_string(app.join("typed/App/Main.d.ts")).expect("declaration");
    assert!(
        !declaration.contains("@flick-lang/effect-ts"),
        "{declaration}"
    );
    let tsc = Command::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../node_modules/.bin/tsc"))
        .args([
            "--noEmit",
            "--module",
            "nodenext",
            "--moduleResolution",
            "nodenext",
            "--target",
            "es2022",
            "typed/App/Main.d.ts",
        ])
        .current_dir(&app)
        .output()
        .expect("TypeScript checker runs");
    assert!(tsc.status.success(), "RT declarations typecheck: {tsc:?}");
    let typescript = flick_in(&app, &["build", "--emit", "ts", "--out-dir", "ts"]);
    assert!(typescript.status.success(), "{typescript:?}");
    assert!(app.join("ts/App/Main.ts").is_file());
    let source = fs::read_to_string(app.join("ts/App/Main.ts")).expect("typescript");
    assert!(!source.contains("@flick-lang/effect-ts"), "{source}");
}

/// An app in `name/app` over the library `name/ctl` by path; the app's directory.
fn with_library(name: &str, build_section: &str, main: &str) -> PathBuf {
    let config = format!("{APP}{build_section}\n[dependencies]\nctl = {{ path = \"../ctl\" }}\n");
    project(
        name,
        &[
            ("app/flick.toml", &config),
            ("app/src/App/Main.fl", main),
            ("ctl/flick.toml", LIB_CONFIG),
            ("ctl/src/Ctl.fl", LIB),
        ],
    )
    .join("app")
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_control_effect_of_a_library_read_from_its_saved_interface() {
    let app = with_library("rt_stages/library", "", LIB_MAIN);
    build(&app, &["--rt"]);
    assert_eq!(run(&app), "60\n");
    // the app changes, the library is taken from what the first build saved
    append(&app.join("src/App/Main.fl"), "// edited");
    build(&app, &["--rt"]);
    assert_eq!(run(&app), "60\n");
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_project_on_the_runtime_by_its_configuration() {
    let app = with_library(
        "rt_stages/configured",
        "\n[build]\neffect = \"rt\"\n",
        LIB_MAIN,
    );
    build(&app, &[]);
    assert_eq!(run(&app), "60\n");
    append(&app.join("src/App/Main.fl"), "// edited");
    build(&app, &[]);
    assert_eq!(run(&app), "60\n");
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_project_of_two_modules_built_again_and_after_an_edit_of_each() {
    let dir = project(
        "rt_stages/modules/app",
        &[
            ("flick.toml", APP),
            ("src/App/Ctl.fl", CTL),
            ("src/App/Main.fl", CTL_MAIN),
        ],
    );
    for edited in [None, Some("src/App/Ctl.fl"), Some("src/App/Main.fl")] {
        if let Some(file) = edited {
            append(&dir.join(file), "// edited");
        }
        for _ in 0..2 {
            build(&dir, &["--rt"]);
            assert_eq!(run(&dir), "60\n", "after editing {edited:?}");
        }
    }
}

/// Whether the build in `out` under `app` is one for the kernel, by what its main
/// module imports, and whether its interface records a direct arrow.
fn on_the_kernel(app: &Path, out: &str) -> (bool, bool) {
    let read = |file: &str| fs::read_to_string(app.join(out).join(file)).expect(file);
    (
        read("App/Main.js").contains("@flick-lang/runtime/rt"),
        read("App/Main.iface").contains("\"direct\""),
    )
}

#[test]
fn the_two_modes_do_not_read_what_the_other_saved() {
    let app = with_library(
        "rt_stages/modes",
        "\n[build]\neffect = \"effect\"\n",
        PLAIN_MAIN,
    );
    let default = ["--out-dir", "default"];
    let kernel = ["--out-dir", "kernel", "--rt"];
    build(&app, &default);
    assert_eq!(on_the_kernel(&app, "default"), (false, false));
    assert!(
        fs::read_to_string(app.join("default/App/Main.js"))
            .expect("default output")
            .contains("@flick-lang/effect-ts"),
        "the Effect backend imports its adapter package"
    );
    build(&app, &kernel);
    assert_eq!(on_the_kernel(&app, "kernel"), (true, true));
    // a new text, for the builds to read again what the first ones saved
    append(&app.join("src/App/Main.fl"), "// edited");
    build(&app, &default);
    assert_eq!(on_the_kernel(&app, "default"), (false, false));
    build(&app, &kernel);
    assert_eq!(on_the_kernel(&app, "kernel"), (true, true));
}
