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
