//! Plugins of dependencies: a library names the WebAssembly component of
//! its plugin with `[package] plugin`, and a module that imports it gets
//! its blocks expanded.

mod common;

use std::process::Command;

use common::project;

const APP: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[dependencies]
pairs = { path = \"../lib\" }
";

const LIB: &str = "schema-version = 1

[package]
name = \"pairs\"
version = \"1.0.0\"
plugin = \"pairs.wasm\"

[project]
kind = \"lib\"
";

const MAIN: &str = "module App.Main exposing (main)

import plugin \"pairs\" exposing (pairs)

point : { x : Float; blocks : Float }
let point = pairs { x = 1.0 }

main : Unit ->{; AppError} Unit
let main () = Platform.stdout_line (show point.x)
";

#[cfg(not(feature = "wasm-plugins"))]
#[test]
fn a_build_without_wasm_plugins_says_so() {
    let root = project(
        "plugins-without-wasm",
        &[
            ("app/flick.toml", APP),
            ("app/src/App/Main.fl", MAIN),
            ("lib/flick.toml", LIB),
        ],
    );
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["check", "--diagnostics", "json"])
        .current_dir(root.join("app"))
        .output()
        .expect("flick runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(stderr.contains("FLICK-P035"), "{stderr}");
    assert!(stderr.contains("built without wasm plugins"), "{stderr}");
}
