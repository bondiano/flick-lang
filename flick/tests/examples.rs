//! The projects of `examples/` beyond the first slice, each on a copy in the
//! target directory (`common::example`). `examples/observability` runs with
//! the local collector of the observability run tests in its environment.

mod common;

use flick_core::build::runner_path;
use std::fs;
use std::path::Path;
use std::process::Command;

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
