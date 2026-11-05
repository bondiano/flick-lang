//! Plugins of dependencies: a library names the WebAssembly component of
//! its plugin with `[package] plugin`, and a module that imports one of the
//! library's modules with `plugin head` gets its blocks expanded.

mod common;

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

/// The module of the library a program imports the plugin's heads with.
const PAIRS: &str = "module Pairs exposing (version)

version : Float
let version = 1.0
";

const MAIN: &str = "module App.Main exposing (main)

import Pairs exposing (plugin pairs)

point : { x : Float; blocks : Float }
let point = pairs { x = 1.0 }

main : Unit ->{; AppError} Unit
let main () = Platform.stdout_line (show point.x)
";

#[cfg(not(feature = "wasm-plugins"))]
mod without {
    use std::process::Command;

    use super::common::project;
    use super::{APP, LIB, MAIN, PAIRS};

    #[test]
    fn a_build_without_wasm_plugins_says_so() {
        let root = project(
            "plugins-without-wasm",
            &[
                ("app/flick.toml", APP),
                ("app/src/App/Main.fl", MAIN),
                ("lib/flick.toml", LIB),
                ("lib/src/Pairs.fl", PAIRS),
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
}

#[cfg(feature = "wasm-plugins")]
mod wasm {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use super::common::project;
    use super::{APP, LIB, MAIN, PAIRS};

    /// The test plugin as a component, which `just wasm-plugins` builds.
    fn component() -> PathBuf {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/wasm32-wasip2/release/flick_test_pairs.wasm");
        assert!(
            path.exists(),
            "no {}: run `just wasm-plugins`",
            path.display()
        );
        path
    }

    /// A project using the plugin, with `main` as its module.
    fn app(name: &str, main: &str) -> PathBuf {
        let root = project(
            name,
            &[
                ("app/flick.toml", APP),
                ("app/src/App/Main.fl", main),
                ("lib/flick.toml", LIB),
                ("lib/src/Pairs.fl", PAIRS),
            ],
        );
        fs::copy(component(), root.join("lib/pairs.wasm")).expect("copy the component");
        root
    }

    fn flick(root: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(root.join("app"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("FLICK_PLUGIN_TRACE", root.join("trace"))
            .env("NO_COLOR", "1")
            .output()
            .expect("flick runs")
    }

    fn trace(root: &Path) -> Vec<String> {
        let text = fs::read_to_string(root.join("trace")).unwrap_or_default();
        text.lines()
            .map(|l| {
                l.split(' ')
                    .filter(|w| w.len() != 64)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect()
    }

    #[test]
    fn a_wasm_plugin_expands_blocks_once() {
        let root = app("plugins-wasm", MAIN);
        let checked = flick(&root, &["check"]);
        assert!(checked.status.success(), "{checked:?}");
        assert_eq!(trace(&root), ["collect pairs", "expand pairs pairs"]);
        // again: every result comes from the cache
        let built = flick(&root, &["build", "--out-dir", "out"]);
        assert!(built.status.success(), "{built:?}");
        let checked = flick(&root, &["check"]);
        assert!(checked.status.success(), "{checked:?}");
        assert_eq!(trace(&root).len(), 2);
        // a changed block: one collect and one expand
        let main = MAIN.replace("x = 1.0", "x = 2.0");
        fs::write(root.join("app/src/App/Main.fl"), main).expect("write");
        let checked = flick(&root, &["check"]);
        assert!(checked.status.success(), "{checked:?}");
        assert_eq!(trace(&root)[2..], ["collect pairs", "expand pairs pairs"]);
    }

    #[test]
    fn a_plugin_runs_within_its_fuel_and_memory() {
        let main = "module App.Main exposing (main, a, b)\n\nimport Pairs exposing (plugin spin, plugin hog)\n\nlet a = spin {}\nlet b = hog {}\n\nmain : Unit ->{; AppError} Unit\nlet main () = ()\n";
        let root = app("plugins-limits", main);
        let checked = flick(&root, &["check"]);
        let stderr = String::from_utf8_lossy(&checked.stderr);
        assert!(
            stderr.contains("plugin `pairs` failed: ran out of its fuel"),
            "{stderr}"
        );
        assert!(
            stderr.contains("plugin `pairs` failed: ran out of its memory"),
            "{stderr}"
        );
    }

    #[test]
    fn a_component_of_another_version_is_reported_with_both() {
        let root = app("plugins-version", MAIN);
        let path = root.join("lib/pairs.wasm");
        let ours = format!("flick:plugin/compiler@{}", flick_plugin::VERSION);
        // an older version of the same length keeps the binary
        let version = format!("0.0.{}", "1".repeat(flick_plugin::VERSION.len() - 4));
        let old = format!("flick:plugin/compiler@{version}");
        assert_eq!(ours.len(), old.len());
        let mut bytes = fs::read(&path).expect("read the component");
        let mut found = false;
        while let Some(at) = bytes.windows(ours.len()).position(|w| w == ours.as_bytes()) {
            bytes[at..at + old.len()].copy_from_slice(old.as_bytes());
            found = true;
        }
        assert!(found, "the component names its interface");
        fs::write(&path, bytes).expect("write the component");
        let checked = flick(&root, &["check"]);
        let stderr = String::from_utf8_lossy(&checked.stderr);
        assert!(
            stderr.contains("FLICK-P035")
                && stderr.contains(&format!("built for `flick:plugin@{version}`"))
                && stderr.contains(&format!("speaks `{}`", flick_plugin::VERSION)),
            "{stderr}"
        );
    }

    #[test]
    fn flick_fmt_asks_the_plugins_of_the_projects_packages() {
        let main = MAIN.replace(
            "let point = pairs { x = 1.0 }",
            "let point =\n    pairs {\n      x = 1.0\n    }",
        );
        let root = app("plugins-fmt", &main);
        let written = flick(&root, &["fmt"]);
        assert!(written.status.success(), "{written:?}");
        // `pairs` gives no layout: the block keeps its lines, which move with its head
        assert_eq!(trace(&root), ["collect pairs", "format pairs pairs"]);
        let text = fs::read_to_string(root.join("app/src/App/Main.fl")).expect("read");
        assert!(text.contains("let point = pairs {\n  x = 1.0\n}"), "{text}");
    }

    #[test]
    fn a_wasm_plugin_derives_the_instances_of_its_class() {
        let main = "module App.Main exposing (main)\n\nimport Pairs\n\ntype Point = Point Float Float\n  deriving Pairs.Arbitrary\n\npoint : Point\nlet point = Pairs.arbitrary\n\nmain : Unit ->{; AppError} Unit\nlet main () = ()\n";
        let root = app("plugins-derive", main);
        let pairs = "module Pairs exposing (..)\n\n@derive\nclass Arbitrary a =\n  arbitrary : a\n\ninstance Arbitrary Float =\n  let arbitrary = 0.0\n";
        fs::write(root.join("lib/src/Pairs.fl"), pairs).expect("write");
        let checked = flick(&root, &["check"]);
        assert!(checked.status.success(), "{checked:?}");
        assert_eq!(
            trace(&root),
            ["collect pairs", "derive pairs Pairs.Arbitrary Point"]
        );
    }

    #[test]
    fn a_missing_component_is_reported_on_the_import() {
        let root = app("plugins-missing", MAIN);
        fs::remove_file(root.join("lib/pairs.wasm")).expect("remove");
        let checked = flick(&root, &["check"]);
        let stderr = String::from_utf8_lossy(&checked.stderr);
        assert!(
            stderr.contains("FLICK-P035") && stderr.contains("cannot read"),
            "{stderr}"
        );
    }
}
