//! A backend that implements only the primitive kernel (#672): the
//! `test/literal` fixture's `kernel` case runs the conformance suite of
//! `tests/kernel`, and the kernel-only tests of the `tests/language`
//! modules `tests/kernel/language.txt` lists, in the portable evaluator with
//! the kernel's primitives alone. A test of the suite that reaches a binding
//! outside the kernel fails the build.

use std::fs;
use std::path::{Path, PathBuf};

use flick_backend_support::kernel;
use serde_json::Value;

use super::common::{flick_in, project};
use super::pinned;

const CONFIG: &str = "\n[target.literal]\ncase = \"kernel\"\n";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The `.fl` files under `dir`, by their path below it.
fn sources(dir: &Path, below: &str, out: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).expect("a source directory") {
        let path = entry.expect("an entry").path();
        let name = path.file_name().expect("a name").to_string_lossy();
        let name = format!("{below}/{name}");
        if path.is_dir() {
            sources(&path, &name, out);
        } else if path.extension().is_some_and(|e| e == "fl") {
            out.push((name, fs::read_to_string(&path).expect("a source")));
        }
    }
}

/// A project of the fixture backend's `kernel` case with `files`.
fn kernel_project(name: &str, files: &[(String, String)]) -> PathBuf {
    let component = repo().join("target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm");
    let config = format!(
        "{}{CONFIG}",
        pinned()
            .replace(&"a".repeat(64), &flick_backend::sha256(&bytes))
            .replace("entry = \"App\"", "entry = \"Kernel.Main\"")
    );
    let mut all: Vec<(&str, &str)> = vec![("flick.toml", &config)];
    all.extend(files.iter().map(|(p, s)| (p.as_str(), s.as_str())));
    let root = project(name, &all);
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    root
}

fn report(root: &Path) -> Vec<Value> {
    let text = fs::read_to_string(root.join(".flick/test/kernel-report.json")).expect("report");
    let json: Value = serde_json::from_str(&text).expect("report JSON");
    assert_eq!(json["kernel"], kernel::VERSION);
    json["tests"].as_array().expect("tests").clone()
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_kernel_only_backend_passes_the_kernel_suite() {
    let mut files = Vec::new();
    sources(&repo().join("tests/kernel/src"), "src", &mut files);
    sources(&repo().join("tests/kernel/test"), "test", &mut files);
    let listed = fs::read_to_string(repo().join("tests/kernel/language.txt")).expect("list");
    let reused: Vec<&str> = (listed.lines())
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();
    for module in &reused {
        let path = repo().join(format!("tests/language/test/Language/{module}.fl"));
        let text = fs::read_to_string(path).expect("a listed module");
        files.push((format!("test/Language/{module}.fl"), text));
    }
    let root = kernel_project("backend_kernel_suite", &files);
    for profile in ["debug", "release"] {
        let run = flick_in(
            &root,
            &[
                "test",
                "--profile",
                profile,
                "--reporter",
                "json",
                "--offline",
            ],
        );
        assert!(run.status.success(), "{profile}: {run:?}");
        let tests = report(&root);
        let failed: Vec<&Value> = tests.iter().filter(|t| t["status"] == "failed").collect();
        assert!(failed.is_empty(), "{profile}: {failed:#?}");
        let status = |prefix: &str, wanted: &str| {
            (tests.iter())
                .filter(|t| t["test"].as_str().is_some_and(|n| n.starts_with(prefix)))
                .filter(|t| t["status"] == wanted)
                .count()
        };
        // what the evaluator does not run is named, never approximated
        for t in tests.iter().filter(|t| t["status"] == "skipped") {
            let (name, why) = (
                t["test"].as_str().expect("name"),
                t["reason"].as_str().expect("why"),
            );
            let kernel = name.starts_with("Kernel.");
            let named = why == "an effect test needs the control kernel"
                || why.ends_with(", which the evaluator does not run");
            assert!(
                named
                    || !kernel
                        && (why.starts_with("outside the kernel: ")
                            || why.starts_with("the evaluator stopped: ")),
                "{profile}: {name}: {why}"
            );
        }
        for module in [
            "Kernel.Numbers",
            "Kernel.Strings",
            "Kernel.Hooks",
            "Kernel.Arrays",
        ] {
            assert!(status(module, "passed") > 0, "{profile}: {module}");
        }
        assert_eq!(status("Kernel.Numbers", "skipped"), 0, "{profile}");
        assert_eq!(status("Kernel.Strings", "skipped"), 0, "{profile}");
        assert_eq!(status("Kernel.Hooks", "skipped"), 0, "{profile}");
        assert_eq!(
            status("Kernel.Cells", "passed"),
            0,
            "{profile}: cells need the runtime"
        );
        // the Int boundaries tests/language pins run on the kernel alone
        for group in ["Language.Ints > wrapping", "Language.Ints > division"] {
            assert!(status(group, "passed") > 5, "{profile}: {group}");
            assert_eq!(status(group, "skipped"), 0, "{profile}: {group}");
        }
        let json: Value = serde_json::from_slice(&run.stdout).expect("runner report");
        let reported = json["tests"].as_array().expect("runner tests");
        assert_eq!(
            reported.len(),
            tests.len(),
            "{profile}: the runner reports each test"
        );
    }
    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_kernel_test_reaching_beyond_the_kernel_fails_the_build() {
    let suite = "\
module Kernel.Beyond exposing (tests)
// π keeps UTF-8 offsets honest

tests : List Test.Test
let tests = [
  Test.test \"length\" (fun () -> Test.equal 2 (String.length \"ab\")),
  Test.test \"trim\" (fun () -> Test.equal \"a\" (String.trim \" a \")),
]
";
    let main = "module Kernel.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n";
    let root = kernel_project(
        "backend_kernel_beyond",
        &[
            ("src/Kernel/Main.fl".into(), main.into()),
            ("test/Kernel/Beyond.fl".into(), suite.into()),
        ],
    );
    let run = flick_in(&root, &["test", "--offline", "--diagnostics", "json"]);
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    let stderr = String::from_utf8_lossy(&run.stderr);
    let diagnostic = (stderr.lines())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|d| {
            d["message"]
                .as_str()
                .is_some_and(|m| m.contains("outside the primitive kernel"))
        })
        .unwrap_or_else(|| panic!("no kernel diagnostic in {stderr}"));
    let message = diagnostic["message"].as_str().expect("message");
    assert!(message.contains("Kernel.Beyond > trim"), "{message}");
    assert!(message.contains("flick:String trim"), "{message}");
    assert!(!message.contains("length"), "{message}");
    let start =
        usize::try_from(diagnostic["range"]["start"].as_u64().expect("start")).expect("fits");
    let end = usize::try_from(diagnostic["range"]["end"].as_u64().expect("end")).expect("fits");
    assert_eq!(&suite[start..end], "Test.equal \"a\" (String.trim \" a \")");
    assert!(!root.join(".flick/test/kernel-report.json").exists());
    fs::remove_dir_all(&root).expect("cleanup");
}
