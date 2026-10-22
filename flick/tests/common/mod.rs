//! Runs the built `flick` binary from the repository root.

use std::io::Write;
use std::path;
use std::process::{Command, Output, Stdio};

pub fn flick(args: &[&str]) -> Output {
    flick_in(concat!(env!("CARGO_MANIFEST_DIR"), "/.."), args)
}

/// Runs `flick` with the working directory `dir`.
pub fn flick_in(dir: impl AsRef<path::Path>, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env_remove("NO_COLOR")
        .output()
        .expect("flick runs")
}

#[allow(dead_code, reason = "not every test file uses it")]
pub fn stdout(args: &[&str]) -> String {
    let out = flick(args);
    assert!(out.status.success(), "{args:?} failed: {out:?}");
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

#[allow(dead_code, reason = "not every test file uses it")]
pub fn flick_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("flick runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("flick exits")
}
