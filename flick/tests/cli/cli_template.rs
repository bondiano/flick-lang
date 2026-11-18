//! `flick new --template cli`: a project whose `main` is a `cli` block,
//! `--template plugin`: a library with a plugin, and the `AGENTS.md` of
//! every template.
//! Running it needs Node and the workspace's runtime: that test is ignored
//! by default and part of `just e2e`.

use crate::common;

use std::fs;
use std::path::PathBuf;

use common::{flick_in, project};

/// A fresh `cli` project `name` in the target's temporary directory.
fn cli_project(name: &str) -> PathBuf {
    let root = project(name, &[]);
    fs::create_dir_all(&root).expect("dir");
    let out = flick_in(&root, &["new", "greeter", "--template", "cli"]);
    assert!(out.status.success(), "{out:?}");
    root.join("greeter")
}

#[test]
fn the_cli_template_checks_and_is_formatted() {
    let dir = cli_project("cli_template_check");
    for args in [&["check", "--warnings-as-errors"][..], &["fmt", "--check"]] {
        let out = flick_in(&dir, args);
        assert!(out.status.success(), "{args:?} failed: {out:?}");
    }
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_cli_template_tests_prints_its_help_and_runs_its_handler() {
    let dir = cli_project("cli_template_run");
    let tests = flick_in(&dir, &["test"]);
    assert!(tests.status.success(), "{tests:?}");
    let help = flick_in(&dir, &["run", "--", "--help"]);
    assert!(help.status.success(), "{help:?}");
    let text = String::from_utf8_lossy(&help.stdout);
    for needle in [
        "greeter [flags] [<name>]",
        "--loud",
        "Print in capitals",
        "Who to greet",
    ] {
        assert!(text.contains(needle), "the help lacks {needle}: {text}");
    }
    let out = flick_in(&dir, &["run", "--", "--loud", "Ada"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "HELLO, ADA!\n",
        "{out:?}"
    );
}

/// `flick new --template plugin`: a library and the crate of its plugin,
/// which builds against the interface of this compiler's release.
#[test]
fn the_plugin_template_checks_and_pins_this_compiler() {
    let root = project("plugin_template", &[]);
    fs::create_dir_all(&root).expect("dir");
    let out = flick_in(&root, &["new", "tagged", "--template", "plugin"]);
    assert!(out.status.success(), "{out:?}");
    let dir = root.join("tagged");
    for args in [&["check", "--warnings-as-errors"][..], &["fmt", "--check"]] {
        let out = flick_in(&dir, args);
        assert!(out.status.success(), "{args:?} failed: {out:?}");
    }
    let config = fs::read_to_string(dir.join("flick.toml")).expect("manifest");
    assert!(config.contains("plugin = \"plugin.wasm\""), "{config}");
    assert!(
        config.contains("release/tagged_plugin.wasm plugin.wasm"),
        "{config}"
    );
    let crate_manifest = fs::read_to_string(dir.join("plugin/Cargo.toml")).expect("crate");
    let tag = format!("tag = \"v{}\"", env!("CARGO_PKG_VERSION"));
    assert!(crate_manifest.contains(&tag), "{crate_manifest}");
    for file in [
        "plugin/src/lib.rs",
        "explain/T001.md",
        "rust-toolchain.toml",
    ] {
        assert!(dir.join(file).exists(), "no {file}");
    }
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents");
    assert!(agents.contains("flick run plugin"), "{agents}");
}

#[test]
fn agents_md_names_the_checks_and_init_keeps_a_repositorys_own() {
    let dir = cli_project("agents_md_new");
    let agents = fs::read_to_string(dir.join("AGENTS.md")).expect("agents");
    for check in [
        "flick fmt",
        "flick check --warnings-as-errors",
        "flick test",
        "flick doc --std",
    ] {
        assert!(agents.contains(check), "no `{check}` in {agents}");
    }
    assert!(!agents.contains("flick run plugin"), "{agents}");

    let repo = project("agents_md_init", &[("AGENTS.md", "ours\n")]);
    let out = flick_in(&repo, &["init", "--template", "lib"]);
    assert!(out.status.success(), "{out:?}");
    assert!(repo.join("flick.toml").exists(), "the project is written");
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).expect("kept"),
        "ours\n"
    );
}
