//! Runs the built `flick` binary from the repository root.

#[allow(dead_code, reason = "not every test file uses it")]
pub mod postgres;

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

/// Writes the script `path` runs as, executable, by a `cp` of its own: a
/// child another test forks while this process writes the file holds it open
/// for writing until its `exec`, and running the script then fails with
/// "Text file busy".
#[cfg(unix)]
#[allow(dead_code, reason = "not every test file uses it")]
pub fn write_script(path: &path::Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    let draft = path.with_extension("draft");
    fs::write(&draft, contents).expect("write");
    let copied = Command::new("cp").arg(&draft).arg(path).status();
    assert!(copied.expect("cp runs").success());
    fs::remove_file(&draft).expect("remove the draft");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("executable");
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

/// A copy of `examples/<example>` at `name` in the target's temporary
/// directory, without its build output and installed packages: it resolves
/// `@flick-lang/runtime` from the workspace and keeps the example clean.
#[allow(dead_code, reason = "not every test file uses it")]
pub fn example(example: &str, name: &str) -> path::PathBuf {
    fn copy(from: &path::Path, to: &path::Path) {
        fs::create_dir_all(to).expect("dir");
        for entry in fs::read_dir(from).expect("read dir") {
            let entry = entry.expect("entry");
            let name = entry.file_name();
            if ["dist", ".flick", "node_modules"].contains(&name.to_string_lossy().as_ref()) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                copy(&path, &to.join(&name));
            } else {
                fs::copy(&path, to.join(&name)).expect("copy");
            }
        }
    }
    let source = path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples")
        .join(example);
    let dir = path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    copy(&source, &dir);
    dir
}

/// An app project's `flick.toml`.
#[allow(dead_code, reason = "not every test file uses it")]
pub const APP: &str = "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n";
