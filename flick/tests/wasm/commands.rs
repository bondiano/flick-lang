//! Commands a backend and a library plugin add: under flat and qualified
//! names, with their collision and the core name they may not take, plans
//! the host refuses before any write, help read from the cache alone, and a
//! session on stdio.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{MAIN, app, flick};

/// The fixture backend, as `just wasm-backends` builds it.
fn backend() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    fs::read(&path).unwrap_or_else(|_| panic!("no {}: run `just wasm-backends`", path.display()))
}

/// The plugin project, its app pinning the fixture backend.
fn project(name: &str) -> PathBuf {
    let root = app(name, MAIN);
    let bytes = backend();
    fs::write(root.join("app/backend.wasm"), &bytes).expect("backend");
    let config = fs::read_to_string(root.join("app/flick.toml")).expect("config");
    let pin = format!(
        "\n[build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\nsha256 = \"{}\"\ncomponent = \"backend.wasm\"\n",
        flick_backend::sha256(&bytes)
    );
    fs::write(root.join("app/flick.toml"), config + &pin).expect("pin");
    root
}

fn text(out: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_backend_and_a_plugin_add_commands_under_flat_and_qualified_names() {
    let root = project("commands_names");
    let app = root.join("app");
    for args in [&["pairs-hello", "you"][..], &["pairs:pairs-hello", "you"]] {
        let out = flick(&root, args);
        let (stdout, stderr) = text(&out);
        assert!(out.status.success(), "{args:?}: {stderr}");
        assert_eq!(stdout, "pairs greets you\n");
        assert_eq!(
            fs::read_to_string(app.join("pairs.txt")).expect("written"),
            "pairs pairs-hello you\n"
        );
        // a core command a contributor declares is refused, and only warned of
        assert!(
            stderr.contains("FLICK-P054") && stderr.contains("`build`"),
            "{stderr}"
        );
    }
    for (args, dir) in [
        (&["literal-hello", "you", "--out", "flat"][..], "flat"),
        (
            &["literal:literal-hello", "you", "--out", "qualified"],
            "qualified",
        ),
    ] {
        let out = flick(&root, args);
        assert!(out.status.success(), "{args:?}: {:?}", text(&out));
        assert_eq!(text(&out).0, "literal greets you\n");
        let written = app.join(dir).join("hello.txt");
        assert_eq!(fs::read_to_string(written).expect("written"), "hello you\n");
    }

    let collision = flick(&root, &["greet", "you"]);
    let (_, stderr) = text(&collision);
    assert_eq!(collision.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("FLICK-P053"), "{stderr}");
    assert!(
        stderr.contains("literal:greet") && stderr.contains("pairs:greet"),
        "{stderr}"
    );
    let qualified = flick(&root, &["literal:greet", "you"]);
    assert!(qualified.status.success(), "{:?}", text(&qualified));
    assert_eq!(
        fs::read_to_string(app.join("greeted.txt")).expect("written"),
        "literal you\n"
    );

    let unknown = flick(&root, &["nothing-of-the-sort"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(
        text(&unknown).1.contains("FLICK-P052"),
        "{:?}",
        text(&unknown)
    );
}

#[test]
fn a_plan_that_escapes_or_runs_an_unpinned_tool_writes_nothing() {
    let root = project("commands_refused");
    let app = root.join("app");
    let before = fs::read_dir(&app).expect("app").count();
    for name in ["literal-escape", "literal-unpinned"] {
        let out = flick(&root, &[name]);
        let (_, stderr) = text(&out);
        assert_eq!(out.status.code(), Some(1), "{name}: {stderr}");
        assert!(stderr.contains("FLICK-P055"), "{name}: {stderr}");
    }
    assert!(!root.join("outside.txt").exists());
    assert!(!app.join("made.txt").exists());
    // the descriptor cache is outside the project, which gains nothing
    assert_eq!(fs::read_dir(&app).expect("app").count(), before);
}

#[test]
fn help_lists_contributed_commands_from_the_cache_without_loading_them() {
    let root = project("commands_help");
    // a first command caches what the contributors add
    assert!(flick(&root, &["literal:greet", "you"]).status.success());
    // the pin stays, so help cannot load these bytes; it reads the cache
    fs::write(root.join("app/backend.wasm"), "not a component").expect("break the component");
    let help = flick(&root, &["--help"]);
    let (stdout, stderr) = text(&help);
    assert!(help.status.success(), "{stderr}");
    assert!(stdout.contains("Commands of `test/literal`:"), "{stdout}");
    assert!(
        stdout.contains("literal-hello") && stdout.contains("echo"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Commands of `pairs`:") && stdout.contains("pairs-hello"),
        "{stdout}"
    );
    let literal = stdout
        .split("Commands of `test/literal`:")
        .nth(1)
        .expect("section");
    let literal = literal.split("\n\n").next().expect("its lines");
    assert!(
        !literal.contains("build"),
        "a core name is not listed again: {literal}"
    );
    // outside a project help lists what the store has seen
    let outside = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("--help")
        .current_dir(&root)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .output()
        .expect("flick runs");
    assert!(
        text(&outside).0.contains("Commands of `test/literal`:"),
        "{:?}",
        text(&outside)
    );
}

fn framed(message: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{message}", message.len())
}

/// The messages of `out`, each a JSON value.
fn messages(out: &[u8]) -> Vec<Value> {
    let text = String::from_utf8_lossy(out);
    text.split("Content-Length: ")
        .filter(|part| !part.is_empty())
        .map(|part| {
            let body = part.split_once("\r\n\r\n").expect("framed").1;
            serde_json::from_str(body).expect("JSON")
        })
        .collect()
}

#[test]
fn a_session_answers_traps_and_cancellations_and_ends_cleanly() {
    let root = project("commands_session");
    // compiling the component first keeps the session's timing its own
    assert!(flick(&root, &["literal:greet", "you"]).status.success());
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("echo")
        .current_dir(root.join("app"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("flick runs");
    let input = [
        r#"{"jsonrpc":"2.0","id":1,"method":"echo","params":"hi"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"trap"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"echo","params":"after"}"#,
        r#"{"jsonrpc":"2.0","id":4,"method":"spin"}"#,
        r#"{"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":4}}"#,
        r#"{"jsonrpc":"2.0","method":"exit"}"#,
    ];
    let mut stdin = child.stdin.take().expect("stdin");
    for message in &input[..4] {
        stdin.write_all(framed(message).as_bytes()).expect("write");
    }
    stdin.flush().expect("flush");
    // the cancellation reaches the spinning call while it runs
    thread::sleep(Duration::from_secs(2));
    for message in &input[4..] {
        stdin.write_all(framed(message).as_bytes()).expect("write");
    }
    drop(stdin);
    let started = Instant::now();
    let out = child.wait_with_output().expect("flick ends");
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    let (_, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let answers = messages(&out.stdout);
    assert_eq!(answers.len(), 4, "{answers:?}");
    assert_eq!(
        (answers[0]["id"].as_i64(), answers[0]["result"].as_str()),
        (Some(1), Some("hi"))
    );
    assert_eq!(
        answers[1]["error"]["code"], -32603,
        "a trap is a protocol error"
    );
    assert_eq!(answers[2]["result"], "after", "the session survives a trap");
    assert_eq!(answers[3]["error"]["code"], -32800, "a cancelled request");
    // the trap and the cancellation kept the state: two requests answered
    assert!(stderr.contains("echo answered 2"), "{stderr}");
}
