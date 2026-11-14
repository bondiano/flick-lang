use super::*;
use flick_core::project::cache::{MANIFEST, Manifest};
use std::path::PathBuf;

fn fixture(name: &str, case: &str) -> PathBuf {
    let root = project(
        name,
        &[
            ("flick.toml", CONFIG),
            (
                "src/App.fl",
                "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/wasm32-wasip2/release/flick_test_backend.wasm"),
    )
    .expect("fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    let config = pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes));
    fs::write(
        root.join("flick.toml"),
        config.replace("entry = \"App\"", &format!("entry = \"{case}\"")),
    )
    .expect("pin");
    let source = root.join(format!("src/{}.fl", case.replace('.', "/")));
    fs::create_dir_all(source.parent().expect("source parent")).expect("source directory");
    if source != root.join("src/App.fl") {
        fs::remove_file(root.join("src/App.fl")).expect("remove initial module");
    }
    fs::write(
        source,
        format!("module {case} exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n"),
    )
    .expect("case");
    root
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_component_cannot_enter_the_native_schema_aot_postprocessor() {
    let root = fixture("backend_schema_aot", "App");
    let result = flick_in(&root, &["build", "--schema-aot", "--diagnostics", "json"]);
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    let diagnostics = String::from_utf8_lossy(&result.stderr);
    assert!(
        diagnostics.contains("FLICK-P012") && diagnostics.contains("schema"),
        "{result:?}"
    );
    assert!(
        !root.join("dist/data.bin").exists(),
        "preflight precedes publication"
    );
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_warning_only_refusal_is_fatal_and_preserves_old_files() {
    let root = fixture("backend_warning_refusal", "Fixture.Refuse");
    fs::create_dir_all(root.join("dist")).expect("output");
    fs::write(root.join("dist/old.js"), "previous build").expect("old file");
    let mut manifest = Manifest::default();
    manifest.record("dist", vec!["old.js".into()]);
    fs::create_dir_all(root.join(".flick")).expect("manifest directory");
    fs::write(
        root.join(MANIFEST),
        serde_json::to_vec(&manifest).expect("JSON"),
    )
    .expect("manifest");
    let result = flick_in(&root, &["build", "--diagnostics", "json"]);
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("intentional warning-only refusal"),
        "{result:?}"
    );
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert_eq!(
        fs::read(root.join("dist/old.js")).expect("old remains"),
        b"previous build"
    );
    assert!(!root.join("dist/.flick-backend.cbor").exists());
}

#[test]
#[ignore = "requires just wasm-backends"]
fn successful_component_test_diagnostics_do_not_skip_assertions_or_enter_the_cache() {
    for case in ["Fixture.Warning", "Fixture.Note"] {
        let root = fixture(&format!("backend_test_{case}"), case);
        fs::create_dir_all(root.join("test")).expect("tests");
        fs::write(root.join("test/Suite.fl"), "module Suite exposing (tests)\ntests : List Test.Test\nlet tests = [Test.test \"executed\" (fun () -> Test.Pass)]\n").expect("suite");
        for _ in 0..2 {
            let result = flick_in(&root, &["test", "--reporter", "json"]);
            assert!(result.status.success(), "{result:?}");
            let json: serde_json::Value =
                serde_json::from_slice(&result.stdout).expect("assertions ran");
            assert_eq!(json["passed"], 1);
            assert!(
                String::from_utf8_lossy(&result.stderr).contains("successful backend diagnostic")
            );
            assert_eq!(
                fs::read_dir(root.join(".flick/cache")).map_or(0, Iterator::count),
                0,
                "diagnostic results stay uncached"
            );
        }
        let result = flick_in(
            &root,
            &["test", "--reporter", "json", "--warnings-as-errors"],
        );
        assert_eq!(
            result.status.code(),
            Some(i32::from(case == "Fixture.Warning")),
            "{result:?}"
        );
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_malformed_component_span_is_rejected_before_mapping_to_host_files() {
    let root = fixture("backend_bad_span", "Fixture.BadSpan");
    let result = flick_in(&root, &["build", "--diagnostics", "json"]);
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("FLICK-P013") && stderr.contains("unknown source file"),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"));
    assert!(!root.join("dist/data.bin").exists());
}

#[test]
#[ignore = "requires just wasm-backends"]
fn release_ir_export_is_the_program_supplied_to_the_component() {
    let root = fixture("backend_release_ir", "Fixture.Ir");
    fs::write(root.join("src/Fixture/Ir.fl"), "module Fixture.Ir exposing (main)\nmain : Unit -> Unit\nlet main () = if true then () else ()\n").expect("unoptimized body");
    let result = flick_in(
        &root,
        &["build", "--profile", "release", "--emit-ir", "json"],
    );
    assert!(result.status.success(), "{result:?}");
    let given = flick_ir::Program::from_cbor(
        &fs::read(root.join("dist/request.ir.cbor")).expect("actual request"),
    )
    .expect("request IR");
    let dump = flick_ir::Program::from_json(
        &fs::read_to_string(root.join("dist/program.ir.json")).expect("dump"),
    )
    .expect("dump IR");
    assert!(
        dump == given,
        "IR dump differs from component input ({} vs {} modules)",
        dump.modules.len(),
        given.modules.len(),
    );
}

#[test]
#[ignore = "requires just wasm-backends"]
fn test_publication_reconciles_previous_native_files_on_fresh_builds_and_cache_hits() {
    let root = fixture("backend_test_reconciliation", "Fixture.CommonJs");
    fs::create_dir_all(root.join("test")).expect("tests");
    fs::write(root.join("test/Suite.fl"), "module Suite exposing (tests)\ntests : List Test.Test\nlet tests = [Test.test \"executed\" (fun () -> Test.Pass)]\n").expect("suite");
    let pinned = fs::read_to_string(root.join("flick.toml")).expect("pinned config");
    let native = pinned
        .split("[build.backend]")
        .next()
        .expect("project config");
    for _ in 0..2 {
        fs::write(root.join("flick.toml"), native).expect("native selection");
        let result = flick_in(&root, &["test", "--reporter", "json"]);
        assert!(result.status.success(), "{result:?}");
        assert!(root.join(".flick/test/package.json").is_file());
        fs::write(root.join("flick.toml"), &pinned).expect("component selection");
        let result = flick_in(&root, &["test", "--reporter", "json"]);
        assert!(result.status.success(), "{result:?}");
        let report: serde_json::Value =
            serde_json::from_slice(&result.stdout).expect("assertions ran");
        assert_eq!(report["passed"], 1);
        assert!(
            !root.join(".flick/test/package.json").exists(),
            "stale native module mode is removed"
        );
        assert!(
            !root.join(".flick/test/test.js").exists(),
            "stale native runner is removed"
        );
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn successful_component_run_notes_do_not_count_as_warnings() {
    for (case, expected) in [("Fixture.Note", 0), ("Fixture.Warning", 1)] {
        let root = fixture(&format!("backend_run_diagnostic_{case}"), case);
        let result = flick_in(&root, &["run", "--warnings-as-errors"]);
        assert_eq!(result.status.code(), Some(expected), "{result:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("successful backend diagnostic"));
        assert_eq!(
            fs::read_dir(root.join(".flick/run"))
                .expect("run generations")
                .count(),
            0
        );
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn commonjs_emission_preserves_string_contents_and_test_filters() {
    let root = fixture("backend_commonjs_strings", "Fixture.CommonJs");
    fs::create_dir_all(root.join("test")).expect("tests");
    fs::write(root.join("test/Suite.fl"), "module Suite exposing (tests)\ntests : List Test.Test\nlet tests = [Test.test \"export const tests =\" (fun () -> Test.Pass)]\n").expect("suite");
    let result = flick_in(
        &root,
        &[
            "test",
            "--reporter",
            "json",
            "--filter",
            "export const tests =",
        ],
    );
    assert!(result.status.success(), "{result:?}");
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).expect("real report");
    assert_eq!(
        report["passed"], 1,
        "the literal filter selects the assertion"
    );
    assert_eq!(report["tests"][0]["name"], "Suite > export const tests =");
}
