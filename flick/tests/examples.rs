//! The projects of `examples/` beyond the first slice, each on a copy in the
//! target directory (`common::example`). `examples/observability` runs with
//! the local collector of the observability run tests in its environment.

mod common;

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
