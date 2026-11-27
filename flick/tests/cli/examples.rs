//! The projects of `examples/` beyond the first slice, each on a copy in the
//! target directory (`common::example`); Conduit's suites run in
//! `flick/tests/conduit.rs`. `examples/observability` runs with
//! the local collector of the observability run tests in its environment;
//! `examples/cli-tool` with `NOTES_FILE` set.

use crate::common;

use flick_core::build::{go::executable, runner_path};
use flick_core::project::config;
use flick_core::source::FileId;
#[cfg(unix)]
use std::os::unix::fs as unix_fs;
use std::path::Path;
use std::process::Command;
use std::{fs, str};

use common::flick_in;

fn success(dir: &Path, args: &[&str]) {
    let out = flick_in(dir, args);
    assert!(out.status.success(), "{args:?} failed: {out:?}");
}

#[test]
fn the_ssr_load_server_checks() {
    let out = common::flick(&["--project", "tools/load/ssr", "check"]);
    assert!(out.status.success(), "{out:?}");
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

/// The page's script starts the client's `main` with the runner of the
/// runtime the client is built for; the runner of the other one leaves the
/// page empty without an error.
#[test]
fn the_conduit_client_starts_with_the_runner_of_its_runtime() {
    let client = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/conduit/client");
    let text = fs::read_to_string(client.join("flick.toml")).expect("the client's flick.toml");
    let config = config::parse(&text, FileId::default()).expect("a valid flick.toml");
    let is_rt = !matches!(
        config
            .targets
            .get("js")
            .and_then(|t| t.get("runtime"))
            .map(|o| &o.value),
        Some(config::TargetValue::String(v)) if v == "effect"
    );
    let runner = if is_rt {
        "@flick-lang/runtime/rt/runner/browser"
    } else {
        "@flick-lang/effect-ts/runner/browser"
    };
    let script = fs::read_to_string(client.join("src/main.js")).expect("the page's script");
    assert!(
        script.contains(&format!("from \"{runner}\"")),
        "the client builds for is_rt={is_rt}, so its script imports `run` from {runner}:\n{script}",
    );
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
            "--target",
            "js",
            "--typescript",
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
#[ignore = "needs Node and the installed runtime"]
fn the_mtl_planner_matches_js_on_vm_tier0_and_tier1() {
    let dir = common::example("mtl", "examples/mtl-vm-js-parity");
    #[cfg(unix)]
    {
        let repo_nm = Path::new(env!("CARGO_MANIFEST_DIR")).join("../node_modules");
        let dst_nm = dir.join("node_modules");
        if repo_nm.is_dir() && !dst_nm.exists() {
            let _ = unix_fs::symlink(&repo_nm, &dst_nm);
        }
    }
    success(&dir, &["build", "--target", "vm", "--out-dir", "dist-vm"]);

    let vm_image = dir.join("dist-vm/main.fvm");
    let vm_image_str = vm_image.to_string_lossy().into_owned();

    let cases: &[&[&str]] = &[
        &["catalog"],
        &["plan", "--budget", "60", "docs", "lint"],
        &["plan", "--budget", "36", "--overhead", "3", "docs", "lint"],
        &["plan", "--budget", "25", "docs", "lint", "tests"],
        &["--help"],
        &["plan", "--help"],
        &["--version"],
        &["plan", "--budget", "abc", "docs"],
        &["plan", "--budget", "-1", "docs"],
        &["--completions", "bash"],
        &["--completions", "zsh"],
        &["--completions", "fish"],
        &["--completions", "powershell"],
    ];

    for args in cases {
        let mut js_args = vec!["run", "--target", "js", "--"];
        js_args.extend_from_slice(args);
        let js = flick_in(&dir, &js_args);
        for tier in ["0", "1"] {
            let mut vm_args = vec![
                "run",
                "--target",
                "vm",
                "--vm-tier",
                tier,
                &vm_image_str,
                "--",
            ];
            vm_args.extend_from_slice(args);
            let vm = flick_in(&dir, &vm_args);
            assert_eq!(
                vm.status.code(),
                js.status.code(),
                "exit code mismatch for {args:?} (tier {tier}): vm={:?} js={:?}\nvm_err={}\njs_err={}",
                vm.status,
                js.status,
                String::from_utf8_lossy(&vm.stderr),
                String::from_utf8_lossy(&js.stderr),
            );
            assert_eq!(
                String::from_utf8_lossy(&vm.stdout),
                String::from_utf8_lossy(&js.stdout),
                "stdout mismatch for {args:?} (tier {tier})"
            );
            assert_eq!(
                String::from_utf8_lossy(&vm.stderr),
                String::from_utf8_lossy(&js.stderr),
                "stderr mismatch for {args:?} (tier {tier})"
            );
        }
    }
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
    success(&dir, &["test", "--target", emit, "--seed", "7"]);
    success(&dir, &["build", "--target", emit]);
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
    success(&dir, &["test", "--target", emit, "--seed", "7"]);
    success(&dir, &["build", "--target", emit]);
    let out = built_main(&dir, emit)
        .output()
        .expect("the interpreter runs");
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "3\n10\ntrue\ntype error: expected Int, found Bool\ntype error: expected Bool, found Int\n"
    );
}

/// The `main` that `flick build` wrote to the project's `dist`, run in the
/// project.
fn built_main(dir: &Path, emit: &str) -> Command {
    let mut command = if emit == "go" {
        Command::new(dir.join("dist").join(executable("main")))
    } else {
        let mut node = Command::new("node");
        node.arg(dir.join("dist").join(runner_path(false)));
        node
    };
    command.current_dir(dir);
    command
}

#[test]
fn the_exchange_checks_and_is_formatted() {
    let dir = common::example("exchange", "examples/exchange-check");
    success(&dir, &["check", "--warnings-as-errors"]);
    success(&dir, &["fmt", "--check"]);
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_exchange_session_prints_its_expected_report() {
    exchange_runs("js");
}

#[test]
#[ignore = "needs Go"]
fn the_exchange_session_prints_its_expected_report_on_go() {
    exchange_runs("go");
}

/// The tests, then a session of 5 000 commands: traders send them at once,
/// so the same report on both targets shows that the sequencer restores
/// their order.
fn exchange_runs(emit: &str) {
    let dir = common::example("exchange", &format!("examples/exchange-{emit}"));
    success(&dir, &["test", "--target", emit, "--seed", "7"]);
    success(&dir, &["build", "--target", emit]);
    let out = built_main(&dir, emit)
        .args(["run", "--orders", "5000"])
        .output()
        .expect("the session runs");
    assert!(out.status.success(), "{out:?}");
    let expected = fs::read_to_string(dir.join("expected.stdout")).expect("expected.stdout");
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    let refused = built_main(&dir, emit)
        .args(["run", "--traders", "0"])
        .output()
        .expect("the session starts");
    assert!(!refused.status.success(), "{refused:?}");
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--traders must be at least 1"),
        "{refused:?}"
    );
}
