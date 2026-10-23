//! Runs the built `flick` binary from the repository root.

use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::{fs, path};

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

/// A fresh directory `name` of the target's temporary directory with `files`.
#[allow(dead_code, reason = "not every test file uses it")]
pub fn project(name: &str, files: &[(&str, &str)]) -> path::PathBuf {
    let dir = path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    for (file, contents) in files {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
        fs::write(path, contents).expect("write");
    }
    dir
}

/// An app project's `flick.toml`.
#[allow(dead_code, reason = "not every test file uses it")]
pub const APP: &str = "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n";
