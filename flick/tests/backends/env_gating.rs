//! End-to-end regression tests for environment gating on build, test, and inspect:
//! - pruning unavailable modules and dead static imports (`node:fs`, `@when "rt"`, `@when "go"`)
//! - filtering gated-out test suites on `flick test` (JS and Go)
//! - `flick inspect --emit iface` with an uncached third-party `[build.backend]`
//! - exit-code contract (`P065` -> exit 2, `P064`/`P066` -> exit 1)

use std::fs;
use std::process::Command;

use super::common::{flick_in, project};

#[test]
fn browser_build_prunes_dead_node_fs_helper_and_go_only_module() {
    let dir = project(
        "env_prune_browser_dead_node_and_go",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n\
                 [build]\nenvironment = \"js/browser\"\n",
            ),
            (
                "src/NodeFs.fl",
                "@when \"js && node\"\nmodule NodeFs exposing (exists)\n\
                 exists : String ->{} Bool\nforeign exists from \"node:fs\" \"existsSync\"\n  arity 1\n",
            ),
            (
                "src/GoOnly.fl",
                "@when \"go\"\nmodule GoOnly exposing (answer)\n\
                 answer : Int\nlet answer = 42\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main, dead_node)\n\
                 import GoOnly\nimport NodeFs\n\
                 dead_node : String ->{} Bool\nlet dead_node path = NodeFs.exists path\n\
                 main : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let build = flick_in(&dir, &["build", "--offline"]);
    assert!(build.status.success(), "browser build failed: {build:?}");

    // Gated-out `@when "js && node"` and `@when "go"` modules must not be emitted at all.
    assert!(!dir.join("dist/_flick/NodeFs.js").exists());
    assert!(!dir.join("dist/NodeFs.iface").exists());
    assert!(!dir.join("dist/_flick/GoOnly.js").exists());
    assert!(!dir.join("dist/_flick/GoOnly.js.map").exists());
    assert!(!dir.join("dist/GoOnly.iface").exists());

    // Surviving `App.js` must not statically import `NodeFs` or `node:fs` or emit `dead_node`.
    let app_js = fs::read_to_string(dir.join("dist/_flick/App.js")).expect("App.js");
    assert!(
        !app_js.contains("NodeFs") && !app_js.contains("node:fs") && !app_js.contains("dead_node"),
        "dead node helper leaked into App.js:\n{app_js}"
    );
    assert!(dir.join("dist/App.iface").is_file());

    // Loading and running the emitted runner succeeds cleanly.
    let run = Command::new("node")
        .arg(dir.join("dist/flick-main.js"))
        .output()
        .expect("node executes built runner");
    assert!(run.status.success(), "runner failed: {run:?}");
}

#[test]
fn effect_build_prunes_gated_rt_module_containing_control_handler() {
    let dir = project(
        "env_prune_effect_rt_control",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n\
                 [target.js]\nruntime = \"effect\"\n",
            ),
            (
                "package.json",
                "{\"name\":\"effect-prune\",\"type\":\"module\",\"dependencies\":{\"@flick-lang/effect-ts\":\"0.1.0\"}}\n",
            ),
            (
                "node_modules/@flick-lang/effect-ts/package.json",
                "{\"name\":\"@flick-lang/effect-ts\",\"version\":\"0.1.0\",\"dependencies\":{\"effect\":\"4.1.2\"}}\n",
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/effect/package.json",
                "{\"version\":\"4.1.2\"}\n",
            ),
            (
                "src/RtControl.fl",
                "@when \"rt\"\nmodule RtControl exposing (run_choice)\n\
                 effect Choice =\n  control flip : Unit ->{} Bool\n\
                 run_choice : Unit ->{} Bool\n\
                 let run_choice () =\n  try\n    Choice.flip ()\n  with\n    | Choice.flip () -> resume true\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main)\nimport RtControl\n\
                 main : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let build = flick_in(&dir, &["build", "--offline"]);
    assert!(
        build.status.success(),
        "effect build should prune gated @when \"rt\" control handler: {build:?}"
    );
    assert!(!dir.join("dist/_flick/RtControl.js").exists());
    assert!(!dir.join("dist/RtControl.iface").exists());
    assert!(dir.join("dist/_flick/App.js").is_file());
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn flick_test_on_js_skips_go_only_test_suite() {
    let dir = project(
        "env_test_filter_js",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            (
                "test/JsSuite.fl",
                "@when \"js\"\nmodule JsSuite exposing (tests)\nimport Ui\n\
                 tests : List Test.Test\n\
                 let tests = [Test.test \"js suite runs\" (fun () ->\n  let _ = Ui.text \"ok\"\n  Test.Pass)]\n",
            ),
            (
                "test/GoSuite.fl",
                "@when \"go\"\nmodule GoSuite exposing (tests)\n\
                 tests : List Test.Test\n\
                 let tests = [Test.test \"go suite must be skipped\" (fun () -> Test.Failed \"ran on js\")]\n",
            ),
        ],
    );
    let out = flick_in(&dir, &["test", "--reporter", "json", "--offline"]);
    assert!(out.status.success(), "js test failed: {out:?}");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json report");
    let tests = report["tests"].as_array().expect("tests array");
    assert_eq!(tests.len(), 1, "only JsSuite must run: {report}");
    assert_eq!(tests[0]["name"], "JsSuite > js suite runs");
    assert_eq!(tests[0]["status"], "passed");
}

#[test]
#[ignore = "requires Go"]
fn flick_test_on_go_skips_js_only_test_suite_using_ui() {
    let dir = project(
        "env_test_filter_go",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n\
                 [tools]\n\"flick/go\" = [\"go\"]\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            (
                "test/JsSuite.fl",
                "@when \"js\"\nmodule JsSuite exposing (tests)\nimport Ui\n\
                 tests : List Test.Test\n\
                 let tests = [Test.test \"js suite must be skipped\" (fun () ->\n  let _ = Ui.text \"no go\"\n  Test.Failed \"ran on go\")]\n",
            ),
            (
                "test/GoSuite.fl",
                "@when \"go\"\nmodule GoSuite exposing (tests)\n\
                 tests : List Test.Test\n\
                 let tests = [Test.test \"go suite runs\" (fun () -> Test.equal 42 (20 + 22))]\n",
            ),
        ],
    );
    let out = flick_in(
        &dir,
        &["test", "--target", "go", "--reporter", "json", "--offline"],
    );
    assert!(out.status.success(), "go test failed: {out:?}");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json report");
    let tests = report["tests"].as_array().expect("tests array");
    assert_eq!(tests.len(), 1, "only GoSuite must run: {report}");
    assert_eq!(tests[0]["name"], "GoSuite > go suite runs");
    assert_eq!(tests[0]["status"], "passed");
}

#[test]
fn inspect_emit_iface_succeeds_with_uncached_third_party_backend_and_host_requires() {
    let sha = "a".repeat(64);
    let dir = project(
        "env_inspect_uncached_custom_backend",
        &[
            (
                "flick.toml",
                &format!(
                    "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n\
                     [dependencies]\nhost-alpha = {{ path = \"hosts/alpha\" }}\n\
                     [build]\nenvironment = \"literal/alpha-host\"\n\
                     [build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
                     sha256 = \"{sha}\"\ncomponent = \"backend.wasm\"\n"
                ),
            ),
            (
                "hosts/alpha/flick.toml",
                "schema-version = 1\n[package]\nname = \"host-alpha\"\nversion = \"0.1.0\"\n\
                 [project]\nkind = \"lib\"\n[host]\nname = \"alpha-host\"\nbackend = \"literal\"\n\
                 requires = \"alpha\"\ntags = [\"console\"]\n",
            ),
            (
                "src/App.fl",
                "@when \"literal && alpha && console\"\nmodule App exposing (main)\n\
                 main : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let store = dir.join("empty-store");
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["inspect", "App", "--emit", "iface"])
        .current_dir(&dir)
        .env("FLICK_STORE", &store)
        .env("NO_COLOR", "1")
        .output()
        .expect("flick inspect runs");
    assert!(
        out.status.success(),
        "inspect --emit iface failed on uncached custom backend: {out:?}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"module\": \"App\""), "{stdout}");
    assert!(
        !store.join("descriptors").exists(),
        "inspect --emit iface must not load or cache backend descriptors"
    );
}

#[test]
fn exit_codes_match_contract_for_p064_p065_and_p066() {
    // FLICK-P065 (unknown host) -> exit code 2
    let p065 = project(
        "env_exit_code_p065",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\nenvironment = \"js/nod\"\n",
            ),
            (
                "src/Lib.fl",
                "module Lib exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let out = flick_in(&p065, &["check", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-P065"));

    // FLICK-P064 (unavailable reachable use) -> exit code 1
    let p064 = project(
        "env_exit_code_p064",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n\
                 [build]\nenvironment = \"js/browser\"\n",
            ),
            (
                "src/NodeOnly.fl",
                "@when \"js && node\"\nmodule NodeOnly exposing (v)\nv : Int\nlet v = 1\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main)\nimport NodeOnly\n\
                 main : Unit -> Unit\nlet main () =\n  let _ = NodeOnly.v\n  ()\n",
            ),
        ],
    );
    let out = flick_in(&p064, &["check", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-P064"));

    // FLICK-P066 (unknown tag typo) -> exit code 1
    let p066 = project(
        "env_exit_code_p066",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/Lib.fl",
                "@when \"js && nodde\"\nmodule Lib exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let out = flick_in(&p066, &["check", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-P066"));
}
