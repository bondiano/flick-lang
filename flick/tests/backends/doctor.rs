//! `flick doctor` reports core checks (compiler, store, git, plugins, backend
//! load) and delegates target-ecosystem checks to the selected backend.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{self, Command, Output};

use super::common::{flick_in, project};
use super::{CONFIG, pinned};

fn doctor_with_path(dir: &Path, path_env: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("PATH", path_env)
        .output()
        .expect("flick doctor")
}

#[test]
fn doctor_in_go_project_checks_go_without_mentioning_node_or_pnpm() {
    let config = format!("{CONFIG}[build]\ntarget = \"go\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
    let root = project("doctor_go_project", &[("flick.toml", &config)]);
    let out = flick_in(&root, &["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    for expected in [
        "compiler: flick",
        "store:",
        "git:",
        "plugins:",
        "backend: flick/go@",
        "Go:",
    ] {
        assert!(text.contains(expected), "`{expected}` missing in:\n{text}");
    }
    for forbidden in [
        "Node.js",
        "pnpm",
        "package.json",
        "node_modules",
        "Content Mapper",
        "@flick-lang/runtime",
        "effect:",
    ] {
        assert!(
            !text.contains(forbidden),
            "Go doctor must not mention `{forbidden}`:\n{text}"
        );
    }

    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join("doctor_go_empty_path");
    fs::create_dir_all(&empty).expect("dir");
    let failed = doctor_with_path(&root, empty.to_str().expect("utf-8"), &["doctor"]);
    assert_eq!(failed.status.code(), Some(2), "{failed:?}");
    let failed_text = String::from_utf8_lossy(&failed.stdout);
    assert!(failed_text.contains("FAIL Go:"), "{failed_text}");
    assert!(!failed_text.contains("Node.js"), "{failed_text}");
    assert!(!failed_text.contains("pnpm"), "{failed_text}");
}

#[test]
fn doctor_in_js_project_checks_js_without_mentioning_go() {
    let root = project("doctor_js_project", &[("flick.toml", CONFIG)]);
    let out = flick_in(&root, &["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    for expected in [
        "compiler: flick",
        "store:",
        "git:",
        "plugins:",
        "backend: flick/js@",
        "Node.js:",
        "pnpm:",
        "@flick-lang/runtime:",
        "Content Mapper:",
    ] {
        assert!(text.contains(expected), "`{expected}` missing in:\n{text}");
    }
    assert!(
        !text.contains("Go:"),
        "JS doctor must not mention Go:\n{text}"
    );
}

#[test]
fn doctor_outside_project_runs_core_checks_and_respects_target_flag() {
    let outside = env::temp_dir().join(format!("flick-doctor-outside-{}", process::id()));
    let _ = fs::remove_dir_all(&outside);
    fs::create_dir_all(&outside).expect("dir");
    let empty = outside.join("empty-path");
    fs::create_dir_all(&empty).expect("dir");
    let empty_str = empty.to_str().expect("utf-8");

    let plain = doctor_with_path(&outside, empty_str, &["doctor"]);
    assert_eq!(plain.status.code(), Some(0), "{plain:?}");
    let plain_text = String::from_utf8_lossy(&plain.stdout);
    for expected in [
        "compiler: flick",
        "store:",
        "note git:",
        "note plugins:",
        "note backend:",
    ] {
        assert!(
            plain_text.contains(expected),
            "`{expected}` missing in:\n{plain_text}"
        );
    }
    for forbidden in ["Node.js", "pnpm", "Go:"] {
        assert!(
            !plain_text.contains(forbidden),
            "projectless doctor must not mention `{forbidden}`:\n{plain_text}"
        );
    }

    let go = doctor_with_path(&outside, empty_str, &["doctor", "--target", "go"]);
    assert_eq!(go.status.code(), Some(2), "{go:?}");
    let go_text = String::from_utf8_lossy(&go.stdout);
    assert!(go_text.contains("backend: flick/go@"), "{go_text}");
    assert!(go_text.contains("FAIL Go:"), "{go_text}");
    assert!(!go_text.contains("Node.js"), "{go_text}");
    assert!(!go_text.contains("pnpm"), "{go_text}");

    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn doctor_reports_plugin_lock_mismatch() {
    let root = project(
        "doctor_plugin_mismatch",
        &[
            (
                "flick.toml",
                concat!(
                    "schema-version = 1\n",
                    "[project]\nkind = \"lib\"\n",
                    "[build]\ntarget = \"go\"\n",
                    "[dependencies]\npairs = { path = \"pairs\" }\n",
                ),
            ),
            (
                "pairs/flick.toml",
                concat!(
                    "schema-version = 1\n",
                    "[project]\nkind = \"lib\"\n",
                    "[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\n",
                ),
            ),
            ("pairs/pairs.wasm", "not real wasm"),
            (
                "flick.lock",
                &format!(
                    "version = 1\n\n[[plugin]]\nname = \"pairs\"\ncomponent = \"pairs/pairs.wasm\"\nchecksum = \"sha256:{}\"\n",
                    "0".repeat(64)
                ),
            ),
        ],
    );
    let out = flick_in(&root, &["doctor"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("FAIL plugin pairs:"),
        "expected plugin lock mismatch failure:\n{text}"
    );
}

#[test]
#[ignore = "requires just wasm-backends"]
fn doctor_in_pinned_literal_backend_project() {
    let root = project("doctor_literal_backend", &[("flick.toml", CONFIG)]);
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds the fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    fs::write(
        root.join("flick.toml"),
        pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes)),
    )
    .expect("pin");
    let out = flick_in(&root, &["doctor"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("backend: test/literal@1.0.0"), "{text}");
    assert!(text.contains("ok   literal-backend:"), "{text}");
    for forbidden in ["Node.js", "pnpm", "Go:"] {
        assert!(!text.contains(forbidden), "{text}");
    }
}

#[test]
fn doctor_unknown_target_preserves_p059_and_suggestion() {
    let root = project("doctor_unknown_target", &[("flick.toml", CONFIG)]);
    let out = flick_in(
        &root,
        &["doctor", "--target", "goo", "--diagnostics", "json"],
    );
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("FAIL backend:"), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let diags: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).expect("json diagnostic line"))
        .collect();
    let p059 = diags
        .iter()
        .find(|d| d["code"] == "FLICK-P059")
        .unwrap_or_else(|| panic!("expected FLICK-P059 in {stderr}"));
    let fixes = p059["fixes"].as_array().expect("fixes array");
    assert!(
        fixes
            .iter()
            .any(|f| f["message"].as_str().unwrap_or("").contains("--target go")),
        "expected `--target go` suggestion in {p059}"
    );
}

#[test]
#[ignore = "requires just wasm-backends"]
fn doctor_refuses_non_read_only_plan_with_p055() {
    let root = project(
        "doctor_non_read_only",
        &[("flick.toml", CONFIG), ("doctor-write.txt", "trigger\n")],
    );
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds the fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    fs::write(
        root.join("flick.toml"),
        pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes)),
    )
    .expect("pin");
    let out = flick_in(&root, &["doctor", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("FAIL backend checks:"), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let diags: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).expect("json diagnostic line"))
        .collect();
    assert!(
        diags.iter().any(|d| d["code"] == "FLICK-P055"),
        "expected FLICK-P055 in {stderr}"
    );
    assert!(
        !root.join("doctor-side-effect.txt").exists(),
        "non-read-only doctor plan must not write files"
    );
}
