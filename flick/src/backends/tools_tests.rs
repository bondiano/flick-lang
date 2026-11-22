use std::path::Path;
use std::process::{self, Command};
use std::{env, fs};

use flick_backend::{OptionValue, Toolchain, ToolchainStep, sha256};

use super::{ADAPTERS, Tool, authorize};

// A parallel test can fork while this process writes a fixture and retain its
// writable descriptor until exec, making execution fail with ETXTBSY. Like
// tests/common::write_script, let a separate process write the final inode.
pub(crate) fn script(path: &Path, contents: &[u8], mode: u32) {
    use std::os::unix::fs::PermissionsExt;

    let draft = path.with_extension("draft");
    fs::write(&draft, contents).expect("script draft");
    assert!(
        Command::new("cp")
            .arg(&draft)
            .arg(path)
            .status()
            .expect("cp runs")
            .success()
    );
    fs::remove_file(draft).expect("remove script draft");
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("script permissions");
}

fn adapter(id: &str) -> &'static super::Adapter {
    ADAPTERS.iter().find(|a| a.id == id).expect("an adapter")
}

#[test]
fn a_mismatched_pin_is_rejected_without_executing_the_candidate() {
    let root = env::temp_dir().join(format!("flick-tool-pin-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("directory");
    for (id, script_text, version) in [
        ("node", "printf 'v1.2.3\\n'", "1.2.3"),
        (
            "go",
            "printf 'go version go1.23.4 linux/amd64\\n'",
            "1.23.4",
        ),
    ] {
        let path = root.join(id);
        let text = format!("#!/bin/sh\nprintf x > \"$0.called\"\n{script_text}\n");
        script(&path, text.as_bytes(), 0o755);
        let mut pin = Toolchain {
            id: id.into(),
            version: version.into(),
            sha256: sha256(b"other executable"),
        };
        assert!(Tool::at(&path, adapter(id), Some(&pin)).is_err());
        assert!(
            !root.join(format!("{id}.called")).exists(),
            "an unapproved candidate must not execute"
        );
        pin.sha256 = sha256(text.as_bytes());
        let tool = Tool::at(&path, adapter(id), Some(&pin)).expect("matching pin");
        assert_eq!(tool.pin, pin);
        assert!(
            root.join(format!("{id}.called")).exists(),
            "the version query runs"
        );
    }
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn resolution_skips_nonexecutable_path_candidates() {
    let root = env::temp_dir().join(format!("flick-tool-path-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    for (directory, mode) in [("first", 0o644), ("second", 0o755)] {
        let directory = root.join(directory);
        fs::create_dir_all(&directory).expect("directory");
        script(
            &directory.join("node"),
            b"#!/bin/sh\nprintf 'v1.2.3\\n'\n",
            mode,
        );
    }
    let paths = env::join_paths([root.join("first"), root.join("second")]).expect("PATH");
    let node = Tool::on_path("node", &paths, None).expect("executable Node later on PATH");
    assert_eq!(
        node.path,
        fs::canonicalize(root.join("second/node")).expect("real node")
    );
    assert_eq!(node.pin.version, "1.2.3");
    assert!(Tool::on_path("rustc", &paths, None).is_err(), "no adapter");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn go_names_its_first_releases_without_a_patch() {
    assert_eq!(
        (adapter("go").parse)("go version go1.20 linux/amd64\n").as_deref(),
        Some("1.20.0")
    );
}

#[test]
fn a_step_sets_only_the_variables_of_its_tool() {
    let step = |id: &str, key: &str| ToolchainStep {
        id: "build".into(),
        dependencies: vec![],
        toolchain: Toolchain {
            id: id.into(),
            version: "1.0.0".into(),
            sha256: "0".repeat(64),
        },
        arguments: vec![],
        environment: vec![OptionValue {
            key: key.into(),
            value: "0".into(),
        }],
        inputs: vec![],
        outputs: vec![],
    };
    assert!(authorize(&step("go", "CGO_ENABLED")).is_ok());
    assert!(authorize(&step("node", "CGO_ENABLED")).is_err());
    assert!(authorize(&step("go", "LD_PRELOAD")).is_err());
}

#[test]
fn launch_authorization_checks_runtime_arguments_and_environment() {
    use super::authorize_launch_parts;

    let node = Toolchain {
        id: "node".into(),
        version: "24.0.0".into(),
        sha256: "0".repeat(64),
    };
    let env = |k: &str, v: &str| OptionValue {
        key: k.into(),
        value: v.into(),
    };
    assert!(authorize_launch_parts(None, &[], &[]).is_ok());
    assert!(authorize_launch_parts(None, &["--enable-source-maps".into()], &[]).is_err());
    assert!(authorize_launch_parts(None, &[], &[env("FLICK_DEVTOOLS", "ws://1")]).is_err());

    assert!(
        authorize_launch_parts(
            Some(&node),
            &[
                "--enable-source-maps".into(),
                "--import".into(),
                "@flick-lang/runtime/rt/runner/devtools".into(),
            ],
            &[env("FLICK_DEVTOOLS", "ws://127.0.0.1:34437")],
        )
        .is_ok()
    );
    assert!(
        authorize_launch_parts(
            Some(&node),
            &["--import".into(), "some-pkg/loader".into()],
            &[],
        )
        .is_ok()
    );
    assert!(authorize_launch_parts(Some(&node), &["--eval".into(), "1".into()], &[]).is_err());
    for bad in [
        "",
        "data:text/javascript,0",
        "DATA:text/javascript,console.log(\"BYPASS_OK\")",
        "Data:text/javascript,0",
        "file:///tmp/evil.mjs",
        "https://evil.example/hook.mjs",
        "node:fs",
        "/tmp/evil.mjs",
        "C:\\evil.mjs",
        "./evil.mjs",
        "../evil.mjs",
        "pkg/../evil.mjs",
        "pkg/./evil.mjs",
        "pkg//evil.mjs",
        "pkg/",
        "pkg/a?x=1",
        "pkg/a#frag",
        "jsdom/%2e%2e/%2e%2e/evil.mjs",
        "pkg/%2E%2E/evil.mjs",
        "pkg/%2f/evil.mjs",
        "pkg/%5c/evil.mjs",
        "pkg/a%20b",
        "pkg with space",
        "pkg\0evil",
        "pkg/\x1bevil",
        "pkg/\x7fevil",
        "pkg/\nevil",
        "-flag",
        "@",
        "@scope",
        "@/pkg",
        "@scope/",
        "@scope/.hidden",
        "@scope/-bad",
    ] {
        assert!(
            authorize_launch_parts(Some(&node), &["--import".into(), bad.into()], &[]).is_err(),
            "expected `--import {bad:?}` to be refused"
        );
    }
    assert!(
        authorize_launch_parts(
            Some(&node),
            &["--import".into(), "../escape.js".into()],
            &[]
        )
        .is_err()
    );
    assert!(authorize_launch_parts(Some(&node), &["--import".into()], &[]).is_err());
    assert!(authorize_launch_parts(Some(&node), &[], &[env("NODE_OPTIONS", "--inspect")]).is_err());
    assert!(authorize_launch_parts(Some(&node), &[], &[env("LD_PRELOAD", "/x")]).is_err());
}
