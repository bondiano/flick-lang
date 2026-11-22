//! `flick new --template cli`: a project whose `main` is a `cli` block,
//! `--template plugin`: a library with a plugin, and the `AGENTS.md` of
//! every template.
//! Running it needs Node and the workspace's runtime: that test is ignored
//! by default and part of `just e2e`.

use crate::common;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{flick_in, project};

/// Recursively collects every file under `dir` as sorted `(relative_path, contents)`.
fn collect_tree(dir: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).expect("read dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("inside root")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let contents = fs::read_to_string(&path).expect("utf-8 template file");
                out.push((rel, contents));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn snapshot_tree(dir: &Path) -> String {
    let mut out = String::new();
    for (path, contents) in collect_tree(dir) {
        let _ = writeln!(out, "=== {path} ===");
        out.push_str(&contents);
        if !contents.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

#[test]
fn js_templates_match_their_snapshots() {
    for template in ["app", "cli", "fullstack", "lib", "plugin"] {
        let root = project(&format!("js_template_snapshot_{template}"), &[]);
        fs::create_dir_all(&root).expect("dir");
        let out = flick_in(&root, &["new", "demo", "--template", template]);
        assert!(out.status.success(), "{template}: {out:?}");
        let tree = snapshot_tree(&root.join("demo"));
        insta::assert_snapshot!(format!("js_template_{template}"), tree);
    }
}

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

#[test]
fn go_templates_check_format_and_reject_unsupported() {
    for template in ["app", "cli", "lib", "plugin"] {
        let root = project(&format!("go_template_{template}"), &[]);
        fs::create_dir_all(&root).expect("dir");
        let out = flick_in(
            &root,
            &["new", "go-demo", "--target", "go", "--template", template],
        );
        assert!(out.status.success(), "{template}: {out:?}");
        let dir = root.join("go-demo");
        assert!(
            !dir.join("package.json").exists(),
            "{template}: Go project must not write package.json"
        );
        let config = fs::read_to_string(dir.join("flick.toml")).expect("flick.toml");
        assert!(config.contains("target = \"go\""), "{template}: {config}");
        assert!(
            config.contains("\"flick/go\" = [\"go\"]"),
            "{template}: {config}"
        );
        let ignore = fs::read_to_string(dir.join(".gitignore")).expect(".gitignore");
        assert!(
            !ignore.contains("node_modules"),
            "{template}: Go .gitignore should not list node_modules: {ignore}"
        );
        for args in [&["check", "--warnings-as-errors"][..], &["fmt", "--check"]] {
            let checked = flick_in(&dir, args);
            assert!(checked.status.success(), "{template} {args:?}: {checked:?}");
        }
    }

    // Default template on `--target go` selects `app`.
    let root = project("go_template_default", &[]);
    fs::create_dir_all(&root).expect("dir");
    let out = flick_in(&root, &["new", "default-go", "--target", "go"]);
    assert!(out.status.success(), "{out:?}");
    assert!(root.join("default-go/src/App/Main.fl").is_file());
    assert!(!root.join("default-go/package.json").exists());

    // Unsupported Go templates (including `fullstack`) report FLICK-P060 listing available ones.
    for bad in ["fullstack", "unknown-template"] {
        let out = flick_in(&root, &["new", "nope", "--target", "go", "--template", bad]);
        assert_eq!(out.status.code(), Some(2), "{bad}: {out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("FLICK-P060"), "{bad}: {stderr}");
        for available in ["app", "cli", "lib", "plugin"] {
            assert!(
                stderr.contains(available),
                "{bad}: missing `{available}` in {stderr}"
            );
        }
        assert!(
            !root.join("nope").exists(),
            "{bad}: failed scaffold must create no directory"
        );
    }

    // Unknown `--target` reports FLICK-P059.
    let bad_target = flick_in(&root, &["new", "bad-target", "--target", "cobol"]);
    assert_eq!(bad_target.status.code(), Some(2), "{bad_target:?}");
    let stderr = String::from_utf8_lossy(&bad_target.stderr);
    assert!(stderr.contains("FLICK-P059"), "{stderr}");
    assert!(!root.join("bad-target").exists());
}

#[test]
fn init_in_non_empty_directory_keeps_existing_files_and_refuses_collisions() {
    // Non-empty directory with unrelated files + AGENTS.md (default JS target).
    let js_repo = project(
        "init_non_empty_js",
        &[
            ("README.md", "# existing readme\n"),
            ("notes/keep.txt", "nested note\n"),
            ("AGENTS.md", "existing agents\n"),
        ],
    );
    let out = flick_in(&js_repo, &["init"]);
    assert!(out.status.success(), "{out:?}");
    assert!(js_repo.join("flick.toml").is_file());
    assert!(js_repo.join("package.json").is_file());
    assert!(js_repo.join("src/App/Main.fl").is_file());
    assert_eq!(
        fs::read_to_string(js_repo.join("README.md")).expect("readme"),
        "# existing readme\n"
    );
    assert_eq!(
        fs::read_to_string(js_repo.join("notes/keep.txt")).expect("nested"),
        "nested note\n"
    );
    assert_eq!(
        fs::read_to_string(js_repo.join("AGENTS.md")).expect("agents"),
        "existing agents\n"
    );

    // Non-empty directory with `--target go --template lib`.
    let go_repo = project(
        "init_non_empty_go",
        &[("README.md", "go readme\n"), ("AGENTS.md", "go agents\n")],
    );
    let out = flick_in(&go_repo, &["init", "--target", "go", "--template", "lib"]);
    assert!(out.status.success(), "{out:?}");
    assert!(go_repo.join("flick.toml").is_file());
    assert!(go_repo.join("src/Lib/Greeting.fl").is_file());
    assert!(!go_repo.join("package.json").exists());
    assert_eq!(
        fs::read_to_string(go_repo.join("README.md")).expect("readme"),
        "go readme\n"
    );
    assert_eq!(
        fs::read_to_string(go_repo.join("AGENTS.md")).expect("agents"),
        "go agents\n"
    );

    // Collision check reports all existing template files before writing anything.
    let colliding = project(
        "init_collision_go",
        &[
            ("README.md", "keep\n"),
            ("flick.toml", "existing manifest\n"),
            ("src/App/Main.fl", "existing main\n"),
        ],
    );
    let out = flick_in(&colliding, &["init", "--target", "go"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FLICK-P019")
            && stderr.contains("`flick.toml` exists")
            && stderr.contains("`src/App/Main.fl` exists"),
        "{stderr}"
    );
    assert!(!colliding.join("src/App/Greeting.fl").exists());
    assert_eq!(
        fs::read_to_string(colliding.join("flick.toml")).expect("kept"),
        "existing manifest\n"
    );
    assert_eq!(
        fs::read_to_string(colliding.join("README.md")).expect("kept"),
        "keep\n"
    );
}

#[test]
fn from_validation_and_empty_path_scaffolding() {
    let root = project("from_validation", &[("empty_pkg/README.md", "nothing\n")]);
    fs::create_dir_all(&root).expect("dir");

    for (name, args, expected_code) in [
        (
            "url_without_sha",
            &["new", "a", "--from", "https://example.invalid/tpl.tgz"][..],
            "FLICK-P032",
        ),
        (
            "url_bad_sha",
            &[
                "new",
                "b",
                "--from",
                "https://example.invalid/tpl.tgz",
                "--sha256",
                "short",
            ],
            "FLICK-P032",
        ),
        (
            "git_without_tag",
            &["new", "c", "--from", "git+https://example.invalid/repo"],
            "FLICK-P032",
        ),
        (
            "missing_path",
            &["new", "d", "--from", "./does-not-exist"],
            "FLICK-P032",
        ),
        (
            "empty_pkg",
            &["new", "e", "--from", "./empty_pkg"],
            "FLICK-P032",
        ),
    ] {
        let out = flick_in(&root, args);
        assert!(!out.status.success(), "{name}: {out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(expected_code), "{name}: {stderr}");
        assert!(!root.join(args[1]).exists(), "{name}: no dir created");
    }

    // `--target` and `--from` conflict at argument parse time (exit code 2).
    let conflict = flick_in(
        &root,
        &["new", "x", "--target", "js", "--from", "./empty_pkg"],
    );
    assert_eq!(conflict.status.code(), Some(2), "{conflict:?}");

    // Scaffolding works with an empty PATH (no node, pnpm, or go required).
    let empty_path = root.join("empty-bin");
    fs::create_dir_all(&empty_path).expect("dir");
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["new", "no-node-app", "--target", "go"])
        .current_dir(&root)
        .env("PATH", &empty_path)
        .env("NO_COLOR", "1")
        .output()
        .expect("flick runs");
    assert!(out.status.success(), "{out:?}");
    assert!(root.join("no-node-app/flick.toml").is_file());
}

#[test]
fn malformed_and_workspace_only_from_manifests_render_cleanly_without_panicking() {
    let root = project(
        "from_invalid_manifests",
        &[
            ("broken_toml/flick.toml", "schema-version = [\n"),
            (
                "workspace_only/flick.toml",
                "schema-version = 1\n\n[workspace]\nmembers = [\"a\"]\n",
            ),
        ],
    );

    for (name, pkg) in [
        ("broken_toml", "./broken_toml"),
        ("workspace_only", "./workspace_only"),
    ] {
        let human = flick_in(&root, &["new", "out-human", "--from", pkg]);
        assert!(!human.status.success(), "{name} human: {human:?}");
        let stderr = String::from_utf8_lossy(&human.stderr);
        assert!(
            !stderr.contains("panicked") && stderr.contains("FLICK-"),
            "{name} human: {stderr}"
        );
        assert!(!root.join("out-human").exists());

        let json = flick_in(
            &root,
            &["--diagnostics", "json", "new", "out-json", "--from", pkg],
        );
        assert!(!json.status.success(), "{name} json: {json:?}");
        let json_stderr = String::from_utf8_lossy(&json.stderr);
        assert!(!json_stderr.contains("panicked"), "{name}: {json_stderr}");
        let events: Vec<serde_json::Value> = json_stderr
            .lines()
            .map(|line| {
                serde_json::from_str(line).unwrap_or_else(|e| panic!("{name}: {e}: {line}"))
            })
            .collect();
        assert!(
            events.iter().any(|e| {
                e["code"]
                    .as_str()
                    .is_some_and(|code| code.starts_with("FLICK-"))
            }),
            "{name}: {events:?}"
        );
        assert!(!root.join("out-json").exists());
    }
}

#[cfg(unix)]
#[test]
fn symlinked_from_manifests_escaping_package_dir_are_refused_without_disclosure() {
    use std::os::unix::fs::symlink;

    let root = project(
        "from_symlink_manifests",
        &[
            ("outside/secret.toml", "SECRET_TOKEN = [\n"),
            ("outside/secret.json", "{\"secret\": true}\n"),
        ],
    );
    let toml_pkg = root.join("toml-pkg");
    fs::create_dir_all(&toml_pkg).expect("toml-pkg");
    symlink(
        root.join("outside/secret.toml"),
        toml_pkg.join("flick.toml"),
    )
    .expect("symlink toml");

    let out_toml = flick_in(&root, &["new", "a", "--from", "./toml-pkg"]);
    assert!(!out_toml.status.success(), "{out_toml:?}");
    let stderr = String::from_utf8_lossy(&out_toml.stderr);
    assert!(
        stderr.contains("FLICK-P055") && !stderr.contains("SECRET_TOKEN"),
        "{stderr}"
    );
    assert!(!root.join("a").exists());

    let backend_pkg = root.join("backend-pkg");
    fs::create_dir_all(&backend_pkg).expect("backend-pkg");
    symlink(
        root.join("outside/secret.json"),
        backend_pkg.join("backend.json"),
    )
    .expect("symlink backend.json");

    let out_backend = flick_in(&root, &["new", "b", "--from", "./backend-pkg"]);
    assert!(!out_backend.status.success(), "{out_backend:?}");
    let stderr = String::from_utf8_lossy(&out_backend.stderr);
    assert!(
        stderr.contains("FLICK-P055") && !stderr.contains("secret"),
        "{stderr}"
    );
    assert!(!root.join("b").exists());
}
