//! Host tool steps of a component, through the CLI: they run only with the
//! project's consent, are cached with the build, and a failing, escaping or
//! cancelled step publishes nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};
use std::{env, fs, thread};

use super::common::{project, write_script};
use super::{CONFIG, pinned};

const MAIN: &str = "module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n";
const STEPS: &str = "module Fixture.Steps exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n";

/// A fake `node` whose `copy IN OUT` logs each call; `extra` replaces the copy.
fn node(root: &Path, extra: &str) {
    let log = root.join("node.log");
    let body = if extra.is_empty() {
        format!("echo called >> '{}'; cat \"$2\" > \"$3\"", log.display())
    } else {
        extra.to_owned()
    };
    let script =
        format!("#!/bin/sh\nif [ \"$1\" = --version ]; then echo v1.0.0; exit 0; fi\n{body}\n");
    write_script(&root.join("bin/node"), &script);
}

fn calls(root: &Path) -> usize {
    fs::read_to_string(root.join("node.log")).map_or(0, |log| log.lines().count())
}

fn setup(name: &str, tools: bool) -> PathBuf {
    let root = project(
        name,
        &[
            ("flick.toml", CONFIG),
            ("src/App.fl", MAIN),
            ("src/Fixture/Steps.fl", STEPS),
        ],
    );
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds the fixture");
    fs::write(root.join("backend.wasm"), &bytes).expect("component");
    let mut config = pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes));
    if tools {
        config.push_str("\n[tools]\n\"test/literal\" = [\"node\"]\n");
    }
    fs::write(root.join("flick.toml"), config).expect("pin");
    fs::create_dir_all(root.join("bin")).expect("bin");
    node(&root, "");
    root
}

fn command(root: &Path, path: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_flick"));
    command
        .args(["build", "--offline", "--diagnostics", "json"])
        .current_dir(root)
        .env("PATH", path);
    command
}

fn build(root: &Path) -> Output {
    let path = format!("{}:/usr/bin:/bin", root.join("bin").display());
    command(root, &path).output().expect("flick runs")
}

/// The files of the output, and that no work directory is left beside it.
fn published(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    for entry in fs::read_dir(root).expect("root").flatten() {
        let name = entry.file_name();
        assert!(
            !name.to_string_lossy().contains("flick-work"),
            "a work directory is left: {}",
            name.display()
        );
    }
    let mut files = BTreeMap::new();
    let mut stack = vec![root.join("dist")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.insert(path.clone(), fs::read(&path).expect("file"));
            }
        }
    }
    files
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_step_runs_only_with_consent_and_reruns_only_when_its_inputs_change() {
    let root = setup("backend_steps_consent", false);
    let refused = build(&root);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert!(
        stderr(&refused).contains("FLICK-P051"),
        "{}",
        stderr(&refused)
    );
    assert!(
        stderr(&refused).contains("\\\"test/literal\\\" = [\\\"node\\\"]"),
        "{}",
        stderr(&refused)
    );
    assert_eq!(calls(&root), 0, "a step without consent never runs");
    assert!(published(&root).is_empty());

    let config = fs::read_to_string(root.join("flick.toml")).expect("config");
    let config = format!("{config}\n[tools]\n\"test/literal\" = [\"node\"]\n");
    fs::write(root.join("flick.toml"), config).expect("consent");
    let built = build(&root);
    assert!(built.status.success(), "{built:?}");
    let copied = fs::read(root.join("dist/copied.txt")).expect("the step's output");
    assert!(
        published(&root).values().any(|bytes| *bytes == copied),
        "a copy of a module's output"
    );
    assert_eq!(calls(&root), 1);

    let again = build(&root);
    assert!(again.status.success(), "{again:?}");
    assert_eq!(
        calls(&root),
        1,
        "an identical build reuses the step's output"
    );

    node(
        &root,
        &format!(
            "echo called >> '{}'; cat \"$2\" > \"$3\" # v2",
            root.join("node.log").display()
        ),
    );
    assert!(build(&root).status.success());
    assert_eq!(calls(&root), 2, "another tool binary runs the step again");

    fs::write(root.join("src/App.fl"), format!("{MAIN}// changed\n")).expect("source");
    assert!(build(&root).status.success());
    assert_eq!(calls(&root), 3, "a changed input runs the step again");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn a_failing_escaping_or_unpinned_step_keeps_the_previous_build() {
    let root = setup("backend_steps_failure", true);
    let first = build(&root);
    assert!(first.status.success(), "{first:?}");
    let before = published(&root);

    fs::write(root.join("src/App.fl"), format!("{MAIN}// changed\n")).expect("source");
    node(&root, "echo partial > \"$3\"; echo boom >&2; exit 3");
    let failed = build(&root);
    assert_eq!(failed.status.code(), Some(1), "{failed:?}");
    assert!(
        stderr(&failed).contains("FLICK-P050"),
        "{}",
        stderr(&failed)
    );
    assert!(stderr(&failed).contains("boom"), "{}", stderr(&failed));
    assert_eq!(published(&root), before, "a failed step publishes nothing");

    node(&root, "");
    let missing = command(&root, "/nonexistent").output().expect("flick runs");
    assert_eq!(missing.status.code(), Some(1), "{missing:?}");
    assert!(
        stderr(&missing).contains("unapproved toolchain"),
        "{}",
        stderr(&missing)
    );
    assert_eq!(published(&root), before, "a tool missing from PATH");

    fs::rename(
        root.join("src/Fixture/Steps.fl"),
        root.join("src/Fixture/StepsEscape.fl"),
    )
    .expect("escape case");
    let escape = STEPS.replace("Fixture.Steps", "Fixture.StepsEscape");
    fs::write(root.join("src/Fixture/StepsEscape.fl"), escape).expect("module");
    let escaped = build(&root);
    assert_eq!(escaped.status.code(), Some(1), "{escaped:?}");
    assert!(
        stderr(&escaped).contains("FLICK-P013"),
        "{}",
        stderr(&escaped)
    );
    assert!(!root.join("escape.txt").exists());
    assert_eq!(published(&root), before, "an output outside the build");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn an_interrupt_cancels_a_running_step_and_keeps_the_previous_build() {
    let root = setup("backend_steps_interrupt", true);
    assert!(build(&root).status.success());
    let before = published(&root);

    let pid = root.join("tool.pid");
    fs::write(root.join("src/App.fl"), format!("{MAIN}// changed\n")).expect("source");
    node(
        &root,
        &format!("sleep 60 & echo $! > '{}'; wait", pid.display()),
    );
    let path = format!("{}:/usr/bin:/bin", root.join("bin").display());
    let flick = command(&root, &path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("flick runs");
    let deadline = Instant::now() + Duration::from_mins(1);
    while !fs::read_to_string(&pid).is_ok_and(|p| !p.trim().is_empty()) {
        assert!(Instant::now() < deadline, "the step started");
        thread::sleep(Duration::from_millis(20));
    }
    let interrupt = Command::new("kill")
        .args(["-INT", &flick.id().to_string()])
        .status();
    assert!(interrupt.expect("kill runs").success());
    let started = Instant::now();
    let output = flick.wait_with_output().expect("flick ends");
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("cancelled"), "{}", stderr(&output));
    let tool = fs::read_to_string(&pid).expect("pid");
    let alive = Command::new("kill")
        .args(["-0", tool.trim()])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    assert!(!alive, "the tool's own child ends with the build");
    assert_eq!(
        published(&root),
        before,
        "a cancelled build publishes nothing"
    );
    fs::remove_dir_all(root).expect("cleanup");
}
