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
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

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
            .env_remove("CI")
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
    fn a_wasm_plugin_checks_its_code_with_the_types_found() {
        let main = "module App.Main exposing (main, x)\n\nimport Pairs exposing (plugin typed)\n\nx : Float\nlet x = typed { 1.0 + 2.0 }\n\nmain : Unit ->{; AppError} Unit\nlet main () = ()\n";
        let root = app("plugins-check", main);
        let checked = flick(&root, &["check"]);
        let stderr = String::from_utf8_lossy(&checked.stderr);
        assert!(!checked.status.success(), "{stderr}");
        assert!(stderr.contains("pairs/X002"), "{stderr}");
        assert!(stderr.contains("`_` is `Float`"), "{stderr}");
        assert!(
            trace(&root).iter().any(|l| l == "check pairs typed"),
            "{:?}",
            trace(&root)
        );
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
        // the version before, of the same length, keeps the binary
        let version = "0.13.0";
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

    /// A project whose `note` is the block `value`, granted `grants`.
    fn reaching(name: &str, value: &str, grants: &str) -> PathBuf {
        let head = value.split(' ').next().unwrap_or_default();
        let main = format!(
            "module App.Main exposing (main, note)\n\nimport Pairs exposing (plugin {head})\n\nnote : String\nlet note = {value}\n\nmain : Unit ->{{; AppError}} Unit\nlet main () = ()\n"
        );
        let root = app(name, &main);
        let manifest = format!("{APP}\n[plugins.pairs]\n{grants}\n");
        fs::write(root.join("app/flick.toml"), manifest).expect("write");
        root
    }

    #[test]
    fn a_plugin_reaches_only_what_the_project_grants() {
        let root = reaching("plugins-denied", "read_file \"notes.txt\" {}", "");
        fs::write(root.join("app/notes.txt"), "hello").expect("write");
        let checked = flick(&root, &["check"]);
        let stderr = String::from_utf8_lossy(&checked.stderr);
        assert!(!checked.status.success(), "{stderr}");
        assert!(stderr.contains("FLICK-P038"), "{stderr}");
        assert!(
            stderr.contains("add `fs-read = [\"notes.txt\"]` to `[plugins.pairs]`"),
            "{stderr}"
        );
        let manifest = fs::read_to_string(root.join("app/flick.toml")).expect("read");
        let granted = manifest.replace(
            "[plugins.pairs]\n",
            "[plugins.pairs]\nfs-read = [\"notes.txt\"]\n",
        );
        fs::write(root.join("app/flick.toml"), granted).expect("write");
        let expanded = flick(&root, &["expand"]);
        assert!(expanded.status.success(), "{expanded:?}");
        assert!(
            String::from_utf8_lossy(&expanded.stdout).contains("\"hello\""),
            "{expanded:?}"
        );
        let built = |root: &Path| {
            let out = flick(root, &["build", "--out-dir", "out"]);
            assert!(out.status.success(), "{out:?}");
            fs::read_to_string(root.join("app/out/App/Main.js")).expect("built")
        };
        assert!(built(&root).contains("\"hello\""));
        // a changed file expands the block again, and builds anew
        fs::write(root.join("app/notes.txt"), "again").expect("write");
        let expanded = flick(&root, &["expand"]);
        assert!(
            String::from_utf8_lossy(&expanded.stdout).contains("\"again\""),
            "{expanded:?}"
        );
        assert!(built(&root).contains("\"again\""));
    }

    /// A server at a port of 127.0.0.1 that answers each request with
    /// `body`, its URL, and the number of requests for that URL: a probe of
    /// the new port, as WSL makes, asks for another.
    fn serve(body: &'static str) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("bound").port();
        let asked = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&asked);
        thread::spawn(move || {
            for mut stream in listener.incoming().map_while(Result::ok) {
                let mut reader = BufReader::new(stream.try_clone().expect("stream"));
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                if line.contains(" /tables ") {
                    counted.fetch_add(1, Ordering::Relaxed);
                }
                while reader.read_line(&mut line).is_ok_and(|n| n > 2) {}
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(answer.as_bytes());
            }
        });
        (format!("http://127.0.0.1:{port}/tables"), asked)
    }

    #[test]
    fn an_answer_is_kept_as_a_snapshot_an_offline_run_replays() {
        let (url, asked) = serve("users");
        let root = reaching(
            "plugins-snapshots",
            &format!("fetch_text \"{url}\" {{}}"),
            "net = [\"127.0.0.1\"]",
        );
        let expanded = flick(&root, &["expand"]);
        assert!(
            String::from_utf8_lossy(&expanded.stdout).contains("\"200 users\""),
            "{expanded:?}"
        );
        let snapshots = root.join("app/.flick/plugin-snapshots/pairs");
        let kept: Vec<_> = fs::read_dir(&snapshots).expect("snapshots").collect();
        assert_eq!(kept.len(), 1);
        // without the cache, offline: the snapshot answers
        fs::remove_dir_all(root.join("cache")).expect("drop the cache");
        let offline = flick(&root, &["--offline", "expand"]);
        assert!(
            String::from_utf8_lossy(&offline.stdout).contains("\"200 users\""),
            "{offline:?}"
        );
        assert_eq!(asked.load(Ordering::Relaxed), 1);
        fs::remove_dir_all(&snapshots).expect("drop the snapshots");
        // `--offline`, or `CI` set, reaches no network
        let ci = |args: &[&str]| {
            Command::new(env!("CARGO_BIN_EXE_flick"))
                .args(args)
                .current_dir(root.join("app"))
                .env("XDG_CACHE_HOME", root.join("cache"))
                .env("NO_COLOR", "1")
                .env("CI", "true")
                .output()
                .expect("flick runs")
        };
        for (args, checked) in [
            (
                &["--offline", "check"][..],
                flick(&root, &["--offline", "check"]),
            ),
            (&["check"], ci(&["check"])),
        ] {
            let stderr = String::from_utf8_lossy(&checked.stderr);
            assert!(stderr.contains("FLICK-P039"), "{args:?}: {stderr}");
        }
    }
}
