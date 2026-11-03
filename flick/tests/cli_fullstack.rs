//! `flick new --template fullstack`: a server entry and a browser page that
//! share the calls of a `server fn` module. Running it needs Node and the
//! workspace's runtime: that test is ignored by default and part of
//! `just e2e`.

mod common;

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, thread};

use common::{flick_in, project};

/// A fresh `fullstack` project `name` in the target's temporary directory.
fn fullstack_project(name: &str) -> PathBuf {
    let root = project(name, &[]);
    fs::create_dir_all(&root).expect("dir");
    let out = flick_in(&root, &["new", "todos", "--template", "fullstack"]);
    assert!(out.status.success(), "{out:?}");
    root.join("todos")
}

#[test]
fn the_fullstack_template_checks_and_is_formatted() {
    let dir = fullstack_project("fullstack_template_check");
    for args in [&["check", "--warnings-as-errors"][..], &["fmt", "--check"]] {
        let out = flick_in(&dir, args);
        assert!(out.status.success(), "{args:?} failed: {out:?}");
    }
    let config = fs::read_to_string(dir.join("flick.toml")).expect("flick.toml");
    assert!(config.contains("target = \"client\""), "{config}");
    let page = fs::read_to_string(dir.join("index.html")).expect("index.html");
    assert!(page.contains("<title>todos</title>"), "{page}");
}

/// The answer of a call of `tag` with `payload`, made with `cookie`.
fn call(port: u16, tag: &str, payload: &str, cookie: &str) -> String {
    let script = format!(
        "const response = await fetch(\"http://127.0.0.1:{port}/_flick/fn\", {{ method: \"POST\", headers: {{ \"content-type\": \"application/ndjson\", cookie: {cookie:?} }}, body: JSON.stringify({{ _tag: \"Request\", id: \"1\", tag: {tag:?}, payload: {payload}, headers: [] }}) + \"\\n\" }});\nprocess.stdout.write(await response.text());\n"
    );
    let out = Command::new("node")
        .args(["--input-type=module", "--eval", &script])
        .output()
        .expect("node");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn wait_for(rx: &mpsc::Receiver<String>, prefix: &str) {
    loop {
        match rx.recv_timeout(Duration::from_secs(60)) {
            Ok(line) if line.starts_with(prefix) => return,
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                panic!("no line `{prefix}…`")
            }
        }
    }
}

fn signal(pid: u32, name: &str) {
    Command::new("kill")
        .args(["-s", name, &pid.to_string()])
        .status()
        .expect("kill");
}

fn read_all(mut reader: impl Read) -> String {
    let mut text = String::new();
    let _ = reader.read_to_string(&mut text);
    text
}

/// The project's server as `flick dev` starts it, on `port`.
fn dev(dir: &Path, port: u16) -> (Child, mpsc::Receiver<String>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("dev")
        .current_dir(dir)
        .env("PORT", port.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    (child, rx)
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn dev_serves_the_shared_calls_and_refuses_an_unauthenticated_write() {
    let dir = fullstack_project("fullstack_template_dev");
    let port = 4817;
    let (mut child, rx) = dev(&dir, port);
    wait_for(&rx, "listening on http://127.0.0.1:4817");
    assert!(call(port, "App.Api.list", "null", "").contains("\"value\":[]"));
    let refused = call(port, "App.Api.add", "\"milk\"", "");
    assert!(refused.contains("Unauthorized"), "{refused}");
    let added = call(port, "App.Api.add", "\"milk\"", "session=ada");
    assert!(added.contains("\"title\":\"ada: milk\""), "{added}");
    assert!(call(port, "App.Api.list", "null", "").contains("ada: milk"));
    signal(child.id(), "TERM");
    let status = child.wait().expect("wait");
    let err = read_all(child.stderr.take().expect("stderr"));
    // without `pnpm install` there is no Vite: the server runs alone
    assert!(err.contains("the client entry needs Vite"), "{err}");
    assert_eq!(status.code(), Some(143), "{err}");
}
