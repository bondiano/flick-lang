//! Third-party backend package discovery, `[target.<name>.dependencies]`,
//! delegated dependency verbs, and `in-place` tool step validation through the
//! `test/literal` fixture component (issue #618 / epic #614 AC #3).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::common::{flick_in, project};
use super::{CONFIG, pinned};

const MAIN: &str = "module App exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n";

fn setup_literal(name: &str, extra_toml: &str, extra_files: &[(&str, &str)]) -> PathBuf {
    let mut files = vec![("flick.toml", CONFIG), ("src/App.fl", MAIN)];
    files.extend_from_slice(extra_files);
    let root = project(name, &files);
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds the fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    let mut config = pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes));
    if !extra_toml.is_empty() {
        config.push('\n');
        config.push_str(extra_toml);
    }
    fs::write(root.join("flick.toml"), config).expect("pin");
    root
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
#[ignore = "requires just wasm-backends"]
fn literal_backend_exercises_target_dependencies_and_verbs_end_to_end() {
    let lib_toml = "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[target.literal.dependencies]\nwidget = \"1.2.0\"\n";
    let helper_manifest = r#"{"name":"helper","version":"0.3.0","source-dir":"src"}"#;
    let helper_src = "module Helper exposing (value)\n\nvalue : Float\nlet value = 42\n";
    let main_using_helper = "module App exposing (main)\n\nimport Helper\n\nmain : Unit -> Unit\nlet main () =\n  let _ = Helper.value\n  ()\n";

    let root = setup_literal(
        "backend_literal_dependencies",
        "[dependencies]\nlib = { path = \"lib\" }\n",
        &[
            ("src/App.fl", main_using_helper),
            ("lib/flick.toml", lib_toml),
            ("literal_packages/helper/manifest.json", helper_manifest),
            ("literal_packages/helper/src/Helper.fl", helper_src),
        ],
    );

    // 1. Source package discovery via `test/literal`'s `packages()`
    let check = flick_in(&root, &["check", "--offline"]);
    assert!(
        check.status.success(),
        "check resolves literal_packages source root: {check:?}"
    );

    // 2. Uninstalled target dependency fails `build` with P034
    let unmet = flick_in(&root, &["build", "--offline"]);
    assert!(!unmet.status.success(), "{unmet:?}");
    let err = stderr(&unmet);
    assert!(
        err.contains("FLICK-P034") && err.contains("widget"),
        "{err}"
    );

    // 3. `install --locked` refuses to modify missing `literal-deps.json`
    let locked = flick_in(&root, &["install", "--locked", "--offline"]);
    assert!(!locked.status.success(), "{locked:?}");
    let err = stderr(&locked);
    assert!(err.contains("FLICK-P034"), "{err}");
    assert!(!root.join("literal-deps.json").exists());

    // 4. `install` writes `literal-deps.json` and `literal-deps.lock`, then `build` + `--locked` pass
    let install = flick_in(&root, &["install", "--offline"]);
    assert!(install.status.success(), "{install:?}");
    let manifest = fs::read_to_string(root.join("literal-deps.json")).expect("manifest");
    assert!(manifest.contains("\"widget\": \"1.2.0\""), "{manifest}");
    assert!(root.join("literal-deps.lock").is_file());

    let locked_ok = flick_in(&root, &["install", "--locked", "--offline"]);
    assert!(locked_ok.status.success(), "{locked_ok:?}");

    // Also install a minimal `src/App.fl` that satisfies literal's pure Unit main codegen check
    fs::write(root.join("src/App.fl"), MAIN).expect("simple main");
    let build = flick_in(&root, &["build", "--offline"]);
    assert!(build.status.success(), "{build:?}");

    // 5. `add`, `deps`, `deps --why`, `update`, `remove`
    let added = flick_in(&root, &["add", "extra@2.0.0"]);
    assert!(added.status.success(), "{added:?}");
    let manifest = fs::read_to_string(root.join("literal-deps.json")).expect("manifest");
    assert!(manifest.contains("\"extra\": \"2.0.0\""), "{manifest}");

    let listed = flick_in(&root, &["deps"]);
    assert!(listed.status.success(), "{listed:?}");
    let out = stdout(&listed);
    assert!(
        out.contains("widget 1.2.0") && out.contains("extra 2.0.0"),
        "{out}"
    );

    let why = flick_in(&root, &["deps", "--why", "widget"]);
    assert!(why.status.success(), "{why:?}");
    assert!(
        stdout(&why).contains("widget@1.2.0 (literal)"),
        "{}",
        stdout(&why)
    );

    let updated = flick_in(&root, &["update", "extra"]);
    assert!(updated.status.success(), "{updated:?}");

    let removed = flick_in(&root, &["remove", "extra"]);
    assert!(removed.status.success(), "{removed:?}");
    let manifest = fs::read_to_string(root.join("literal-deps.json")).expect("manifest");
    assert!(!manifest.contains("\"extra\""), "{manifest}");

    // 6. Conflicting version in `literal-deps.json` reports P040
    fs::write(
        root.join("literal-deps.json"),
        "{\n  \"dependencies\": {\n    \"widget\": \"9.9.9\"\n  },\n  \"installed\": [\"widget\"]\n}\n",
    )
    .expect("conflict");
    let conflict = flick_in(&root, &["check", "--offline"]);
    assert!(!conflict.status.success(), "{conflict:?}");
    assert!(
        stderr(&conflict).contains("FLICK-P040"),
        "{}",
        stderr(&conflict)
    );

    // 7. Invalid version in `[target.literal.dependencies]` reports P032
    fs::write(
        root.join("lib/flick.toml"),
        lib_toml.replace("1.2.0", "bad:spec"),
    )
    .expect("invalid spec");
    let invalid = flick_in(&root, &["check", "--offline"]);
    assert!(!invalid.status.success(), "{invalid:?}");
    assert!(
        stderr(&invalid).contains("FLICK-P032"),
        "{}",
        stderr(&invalid)
    );

    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(unix)]
#[test]
#[ignore = "requires just wasm-backends"]
fn literal_backend_enforces_in_place_step_contract_end_to_end() {
    use super::common::write_script;

    let root = setup_literal(
        "backend_literal_in_place",
        "[tools]\n\"test/literal\" = [\"node\"]\n",
        &[],
    );
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("bin");
    let log = root.join("node.log");
    write_script(
        &bin.join("node"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo v24.0.0; exit 0; fi\n\
             echo \"$*\" >> '{}'\n\
             cat staged.txt > after.txt\n",
            log.display()
        ),
    );
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let run = |mode: &str| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["literal-in-place", "--mode", mode])
            .current_dir(&root)
            .env("PATH", &path)
            .output()
            .expect("flick runs")
    };

    // Invalid in-place plans must be rejected with P055 before publishing or running the tool.
    for bad in ["bad-inputs", "bad-reads", "bad-outputs"] {
        let out = run(bad);
        assert!(!out.status.success(), "{bad}: {out:?}");
        assert!(
            stderr(&out).contains("FLICK-P055"),
            "{bad}: {}",
            stderr(&out)
        );
        assert!(
            !root.join("staged.txt").exists(),
            "{bad} must not publish before validation"
        );
        assert!(!log.exists(), "{bad} must not execute the tool");
    }

    // Valid in-place plan publishes its file first, then runs `node` in `project.root`.
    let ok = run("ok");
    assert!(ok.status.success(), "{ok:?}");
    assert_eq!(
        fs::read_to_string(root.join("staged.txt")).expect("marker"),
        "staged-content\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("after.txt")).expect("in-place output"),
        "staged-content\n"
    );
    assert!(log.is_file(), "tool executed in place");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn literal_backend_optional_pack_command_emits_ecosystem_artifact_alongside_source_tarball() {
    let root = setup_literal(
        "backend_literal_pack",
        "[package]\nname = \"litpkg\"\nversion = \"0.4.0\"\n\n[target.literal]\ndialect = \"beta\"\n",
        &[(
            "literal-pack.json",
            "{\"name\":\"litpkg\",\"version\":\"0.4.0\"}\n",
        )],
    );

    // 1. Both the Flick source tarball (stdout) and test/literal's `.literal-pkg` are written.
    let packed = flick_in(&root, &["pack", "--out-dir", "artifacts", "--offline"]);
    assert_eq!(packed.status.code(), Some(0), "{packed:?}");
    let out = stdout(&packed);
    let (sha, tarball) = out.trim().split_once("  ").expect("sha and tarball path");
    assert_eq!(sha.len(), 64, "{out}");
    assert!(
        Path::new(tarball).is_file() && tarball.ends_with("litpkg-0.4.0.tgz"),
        "{out}"
    );
    let pkg_path = root.join("artifacts/litpkg-0.4.0.literal-pkg");
    assert!(pkg_path.is_file(), "expected {}", pkg_path.display());
    let payload: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&pkg_path).expect("literal pkg")).expect("json");
    assert_eq!(payload["kind"], "app");
    assert_eq!(payload["has_package"], true);
    let entries = payload["entries"].as_array().expect("entries");
    assert!(entries.iter().any(|e| e == "main"), "{payload}");
    let runners = payload["runners"].as_array().expect("runners");
    assert!(
        runners.iter().any(|r| r == "main=invoke-main.mjs"),
        "expected backend-supplied launch path, got: {payload}"
    );
    let options = payload["options"].as_array().expect("options");
    assert!(
        options.iter().any(|o| o == "dialect=beta"),
        "expected resolved target option, got: {payload}"
    );

    // 2. Without `literal-pack.json`, `has-package = true` produces only the Flick source tarball.
    fs::remove_file(root.join("literal-pack.json")).expect("remove pack manifest");
    let source_only = flick_in(
        &root,
        &["pack", "--out-dir", "artifacts_source", "--offline"],
    );
    assert_eq!(source_only.status.code(), Some(0), "{source_only:?}");
    assert!(root.join("artifacts_source/litpkg-0.4.0.tgz").is_file());
    assert!(
        !root
            .join("artifacts_source/litpkg-0.4.0.literal-pkg")
            .exists()
    );

    // 3. A backend `pack` plan writing outside `.flick/pack` is rejected with FLICK-P055 before touching out_dir.
    fs::write(root.join("literal-pack.json"), "{\"escape\":true}\n").expect("escape manifest");
    let escaped = flick_in(
        &root,
        &["pack", "--out-dir", "artifacts_escape", "--offline"],
    );
    assert_eq!(escaped.status.code(), Some(1), "{escaped:?}");
    assert!(
        stderr(&escaped).contains("FLICK-P055"),
        "{}",
        stderr(&escaped)
    );
    assert!(!root.join("artifacts_escape").exists());
    assert!(!root.join("escaped.literal-pkg").exists());

    fs::remove_dir_all(root).expect("cleanup");
}
