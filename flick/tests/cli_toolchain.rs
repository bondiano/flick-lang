//! Installed runtime and adapter versions guard the build and its cache.

mod common;

use std::fs;
use std::path::Path;
use std::process::Output;

use common::{APP, flick_in, project};

const MAIN: &str = "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"hi\"\n";

fn code(out: &Output) -> Option<i32> {
    out.status.code()
}
fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}
fn cached(dir: &Path) -> usize {
    fs::read_dir(dir.join(".flick/cache")).map_or(0, Iterator::count)
}

#[test]
fn an_incompatible_runtime_blocks_the_build() {
    let dir = project(
        "cli_runtime",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/@flick-lang/runtime/package.json",
                "{ \"name\": \"@flick-lang/runtime\", \"version\": \"0.0.9\" }",
            ),
        ],
    );
    let out = flick_in(&dir, &["build"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    assert!(stderr(&out).contains("FLICK-P015"));
    assert!(!dir.join("dist").exists());
}

#[test]
fn an_incompatible_effect_blocks_build_and_run() {
    let dir = project(
        "cli_effect_version",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/@flick-lang/runtime/package.json",
                "{ \"name\": \"@flick-lang/runtime\", \"version\": \"0.1.0\" }",
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/effect/package.json",
                "{ \"name\": \"effect\", \"version\": \"3.0.0\" }",
            ),
        ],
    );
    for command in ["build", "run"] {
        let out = flick_in(&dir, &[command]);
        assert_eq!(code(&out), Some(2), "{command}: {out:?}");
        assert!(stderr(&out).contains("FLICK-P015"), "{command}: {out:?}");
    }
    assert!(!dir.join("dist").exists());
    assert!(
        fs::read_dir(dir.join(".flick/run")).map_or(0, Iterator::count) == 0,
        "no generation was written"
    );
}

#[test]
fn the_installed_runtime_and_effect_are_in_the_cache_key() {
    let dir = project(
        "cli_cache_runtime",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let build = || assert_eq!(code(&flick_in(&dir, &["build"])), Some(0));
    build();
    let runtime = dir.join("node_modules/@flick-lang/runtime");
    let adapter = dir.join("node_modules/@flick-lang/effect-ts");
    fs::create_dir_all(&runtime).expect("dir");
    fs::create_dir_all(adapter.join("node_modules/effect")).expect("dir");
    fs::write(
        runtime.join("package.json"),
        "{ \"name\": \"@flick-lang/runtime\", \"version\": \"0.1.0\" }",
    )
    .expect("write");
    build();
    assert_eq!(cached(&dir), 2, "the runtime version is in the key");
    fs::write(
        adapter.join("package.json"),
        "{ \"name\": \"@flick-lang/effect-ts\", \"version\": \"0.1.0\" }",
    )
    .expect("write");
    build();
    assert_eq!(cached(&dir), 3, "the adapter version is in the key");
    fs::write(
        adapter.join("node_modules/effect/package.json"),
        "{ \"name\": \"effect\", \"version\": \"4.1.2\" }",
    )
    .expect("write");
    build();
    assert_eq!(cached(&dir), 4, "the Effect version is in the key");
}
