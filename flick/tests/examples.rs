//! The projects of `examples/` beyond the first slice, each on a copy in the
//! target directory (`common::example`); Conduit's suites run in
//! `flick/tests/conduit.rs`. `examples/observability` runs with
//! the local collector of the observability run tests in its environment;
//! `examples/cli-tool` with `NOTES_FILE` set.

mod common;

use flick_core::build::{go::executable, runner_path};
use std::path::Path;
use std::process::Command;
use std::{fs, str};

use common::flick_in;

fn success(dir: &Path, args: &[&str]) {
    let out = flick_in(dir, args);
    assert!(out.status.success(), "{args:?} failed: {out:?}");
}

/// Starts the collector, runs `flick run` in the project with
/// `OTEL_EXPORTER_OTLP_ENDPOINT` on it, and prints the run's stdout and
/// then everything the collector got.
const COLLECT: &str = r#"
import { spawn } from "node:child_process";
const [receiver, flick, dir] = process.argv.slice(1);
const { start, received, stop } = await import(receiver);
const port = await start();
const child = spawn(flick, ["run"], {
  cwd: dir,
  env: { ...process.env, OTEL_EXPORTER_OTLP_ENDPOINT: `http://127.0.0.1:${port}` },
  stdio: ["ignore", "pipe", "inherit"],
});
let out = "";
child.stdout.on("data", (chunk) => (out += chunk));
const code = await new Promise((resolve) => child.on("close", resolve));
console.log(`exit ${code}\n${out}\n${received()}`);
await stop();
"#;

/// The contract, the server and the client of Conduit, under the tight
/// `[lint]` thresholds of each.
#[test]
fn the_conduit_workspace_checks_and_is_formatted() {
    let dir = common::example("conduit", "examples/conduit-check");
    success(&dir, &["check", "--warnings-as-errors"]);
    success(&dir, &["fmt", "--check"]);
}

#[test]
fn the_observability_example_checks_and_is_formatted() {
    let dir = common::example("observability", "examples/observability-check");
    success(&dir, &["check", "--warnings-as-errors"]);
    success(&dir, &["fmt", "--check"]);
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_observability_example_reports_to_a_local_collector() {
    let dir = common::example("observability", "examples/observability-run");
    let receiver = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/run/stdlib/observability/receiver.mjs")
        .canonicalize()
        .expect("the collector of the run tests");
    let out = Command::new("node")
        .args(["--input-type=module", "-e", COLLECT])
        .arg(&receiver)
        .arg(env!("CARGO_BIN_EXE_flick"))
        .arg(&dir)
        .output()
        .expect("`node` is on PATH");
    let got = String::from_utf8(out.stdout).expect("utf-8 stdout");
    assert!(got.starts_with("exit 0\n"), "{got}");
    assert!(got.contains("sent to http://127.0.0.1:"), "{got}");
    for needle in [
        "/v1/traces",
        "/v1/logs",
        "/v1/metrics",
        "\"checkout\"",
        "\"pay\"",
        "\"flick-shop\"",
        "paid by ada",
        "\"orders\"",
        "\"audit\"",
        "\"flick-audit\"",
    ] {
        assert!(got.contains(needle), "the collector lacks {needle}: {got}");
    }
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_mtl_planner_runs_pure_transformers_and_validates_cli_minutes() {
    let dir = common::example("mtl", "examples/mtl-run");
    let capabilities = concat!(
        "identity ok=5 state=15 log=begin,done\n",
        "identity error=negative state=10 log=begin\n",
        "eff ok=5 state=15 log=begin,done\n",
        "eff error=negative state=10 log=begin\n",
    );
    let cases: &[(&[&str], i32)] = &[
        (&["plan", "--budget", "60", "docs", "tests", "lint"], 0),
        (
            &["plan", "--budget", "36", "--overhead", "3", "docs", "lint"],
            0,
        ),
        (&["plan", "--budget", "25", "docs", "lint", "tests"], 1),
        (&["plan", "--budget", "60", "docs", "missing", "lint"], 1),
        (&["plan", "--budget", "0"], 0),
        (&["plan", "--budget", "9007199254740991"], 0),
        (&["plan", "--budget", "-1"], 1),
        (&["plan", "--budget", "1.5"], 1),
        (&["plan", "--budget", "NaN"], 1),
        (&["plan", "--budget", "Infinity"], 1),
        (&["plan", "--budget", "9007199254740992"], 1),
        (&["plan", "--budget", "60", "--overhead", "-1"], 1),
    ];
    for profile in ["debug", "release"] {
        success(
            &dir,
            &["test", "--profile", profile, "--seed", "7", "--jobs", "1"],
        );
        let run = ["run", "--entry", "App.Capabilities", "--profile", profile];
        let out = flick_in(&dir, &run);
        assert!(out.status.success(), "{run:?} failed: {out:?}");
        assert_eq!(
            String::from_utf8(out.stdout).expect("utf-8 stdout"),
            capabilities,
            "{run:?}"
        );
        for (args, expected) in cases {
            let mut run = vec!["run", "--profile", profile, "--"];
            run.extend_from_slice(args);
            let out = flick_in(&dir, &run);
            assert_eq!(out.status.code(), Some(*expected), "{run:?}: {out:?}");
        }
    }

    let ts_dir = common::example("mtl", "examples/mtl-capabilities-ts");
    let manifest = ts_dir.join("flick.toml");
    let contents = fs::read_to_string(&manifest).expect("example manifest");
    fs::write(
        &manifest,
        contents.replace("entry = \"App.Main\"", "entry = \"App.Capabilities\""),
    )
    .expect("TypeScript copy's entry");
    success(
        &ts_dir,
        &[
            "build",
            "--emit",
            "ts",
            "--profile",
            "debug",
            "--out-dir",
            "dist-ts",
        ],
    );
    let out = Command::new("node")
        .arg("--enable-source-maps")
        .arg(ts_dir.join("dist-ts").join(runner_path(true)))
        .current_dir(&ts_dir)
        .output()
        .expect("`node` is on PATH (Node.js >= 24.18.0)");
    assert!(
        out.status.success(),
        "TypeScript capabilities failed: {out:?}"
    );
    assert_eq!(
        String::from_utf8(out.stdout).expect("utf-8 stdout"),
        capabilities
    );
}

#[test]
fn the_cli_tool_example_checks_and_is_formatted() {
    let dir = common::example("cli-tool", "examples/cli-tool-check");
    success(&dir, &["check", "--warnings-as-errors"]);
    success(&dir, &["fmt", "--check"]);
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_cli_tool_example_keeps_notes_in_the_file_of_its_environment() {
    cli_tool_keeps_notes("js");
}

#[test]
#[ignore = "needs Go"]
fn the_cli_tool_example_keeps_notes_on_go() {
    cli_tool_keeps_notes("go");
}

fn cli_tool_keeps_notes(emit: &str) {
    let dir = common::example("cli-tool", &format!("examples/cli-tool-{emit}"));
    success(&dir, &["test", "--emit", emit, "--seed", "7"]);
    success(&dir, &["build", "--emit", emit]);
    let command = || {
        let mut command = if emit == "go" {
            Command::new(dir.join("dist").join(executable("main")))
        } else {
            let mut node = Command::new("node");
            node.arg(dir.join("dist").join(runner_path(false)));
            node
        };
        command.current_dir(&dir);
        command
    };
    let notes = dir.join("elsewhere.txt");
    let notes_cli = |args: &[&str]| {
        command()
            .args(args)
            .env("NOTES_FILE", &notes)
            .output()
            .expect("the notes executable runs")
    };
    for args in [
        &["add", "buy", "milk"][..],
        &["add", "call", "mom"],
        &["done", "1"],
    ] {
        let out = notes_cli(args);
        assert!(out.status.success(), "{args:?} failed: {out:?}");
    }
    let list = notes_cli(&["list"]);
    assert!(list.status.success(), "{list:?}");
    assert_eq!(String::from_utf8_lossy(&list.stdout), "1. call mom\n");
    assert_eq!(
        fs::read_to_string(&notes).expect("the notes file"),
        "call mom\n"
    );
    assert!(
        !dir.join("notes.txt").exists(),
        "NOTES_FILE chooses the file"
    );
    let missing = notes_cli(&["done", "9"]);
    assert_eq!(missing.status.code(), Some(1), "{missing:?}");
    assert!(String::from_utf8_lossy(&missing.stderr).contains("there is no note 9"));
    for args in [
        &["add"][..],
        &["done", "not-an-integer"],
        &["done", "1", "extra"],
        &["list", "extra"],
        &["missing"],
        &["add", "--unknown"],
    ] {
        let out = notes_cli(args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        assert_eq!(
            fs::read_to_string(&notes).expect("unchanged notes"),
            "call mom\n"
        );
    }
    let help = notes_cli(&["--help"]);
    assert!(help.status.success(), "{help:?}");
    let help_text = String::from_utf8_lossy(&help.stdout);
    for name in ["init", "add", "list", "done"] {
        assert!(help_text.contains(name), "{help:?}");
    }
    let version = notes_cli(&["--version"]);
    assert!(version.status.success(), "{version:?}");
    assert_eq!(
        String::from_utf8_lossy(&version.stdout)
            .split_whitespace()
            .last(),
        Some("v0.1.0")
    );
    let completions = notes_cli(&["--completions", "bash"]);
    assert!(completions.status.success(), "{completions:?}");
    bash_completes_add(&completions.stdout);
    assert_eq!(
        fs::read_to_string(&notes).expect("unchanged notes"),
        "call mom\n"
    );
    let literal = notes_cli(&["add", "--", "--help"]);
    assert!(literal.status.success(), "{literal:?}");
    assert_eq!(
        fs::read_to_string(&notes).expect("literal note"),
        "call mom\n--help\n"
    );
    let default = command()
        .args(["add", "default"])
        .env_remove("NOTES_FILE")
        .output()
        .expect("default path");
    assert!(default.status.success(), "{default:?}");
    assert_eq!(
        fs::read_to_string(dir.join("notes.txt")).expect("default notes"),
        "default\n"
    );
}

fn bash_completes_add(script: &[u8]) {
    let script = format!(
        "{}\nCOMP_WORDS=(notes a)\nCOMP_CWORD=1\nCOMP_LINE='notes a'\nCOMP_POINT=7\ncompletion=$(complete -p notes)\ncompletion=${{completion#* -F }}\ncompletion=${{completion%% *}}\n\"$completion\" notes a notes || exit\nprintf '%s\\n' \"${{COMPREPLY[@]}}\"\n",
        str::from_utf8(script).expect("bash completions")
    );
    let completion = Command::new("bash")
        .args(["-c", &script])
        .output()
        .expect("bash");
    assert!(completion.status.success(), "{completion:?}");
    assert_eq!(String::from_utf8_lossy(&completion.stdout), "add\n");
}

#[test]
fn the_typed_interpreter_checks_and_is_formatted() {
    let dir = common::example("typed-interpreter", "examples/typed-interpreter-check");
    success(&dir, &["check", "--warnings-as-errors"]);
    success(&dir, &["fmt", "--check"]);
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_typed_interpreter_evaluates_well_typed_terms() {
    typed_interpreter_runs("js");
}

#[test]
#[ignore = "needs Go"]
fn the_typed_interpreter_evaluates_well_typed_terms_on_go() {
    typed_interpreter_runs("go");
}

fn typed_interpreter_runs(emit: &str) {
    let dir = common::example(
        "typed-interpreter",
        &format!("examples/typed-interpreter-{emit}"),
    );
    success(&dir, &["test", "--emit", emit, "--seed", "7"]);
    success(&dir, &["build", "--emit", emit]);
    let mut command = if emit == "go" {
        Command::new(dir.join("dist").join(executable("main")))
    } else {
        let mut node = Command::new("node");
        node.arg(dir.join("dist").join(runner_path(false)));
        node
    };
    let out = command
        .current_dir(&dir)
        .output()
        .expect("the interpreter runs");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "3\n10\ntrue\ntype error: expected Int, found Bool\ntype error: expected Bool, found Int\n"
    );
}
