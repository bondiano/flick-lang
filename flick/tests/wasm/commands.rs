//! Commands a backend and a library plugin add: under flat and qualified
//! names, with their collision and the core name they may not take, plans
//! the host refuses before any write, help read from the cache alone, and a
//! session on stdio.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use flick_backend::sha256;
use serde_json::Value;

use super::{LIB, MAIN, app, component, flick};

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
        sha256(&bytes)
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

#[test]
fn a_command_gets_its_inputs_and_resumes_after_a_step_that_reads_in_place() {
    let root = project("commands_resume");
    let app = root.join("app");
    fs::create_dir_all(app.join("data")).expect("data");
    fs::write(app.join("data/in.txt"), "from data\n").expect("data file");
    fs::write(app.join("notes.txt"), "noted\n").expect("notes");
    let config = fs::read_to_string(app.join("flick.toml")).expect("config");
    let grant = "\n[tools]\n\"test/literal\" = [\"node\"]\n";
    fs::write(app.join("flick.toml"), config + grant).expect("grant");

    let out = flick(&root, &["literal-read"]);
    let (_, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let listed = fs::read_to_string(app.join("read.txt")).expect("read.txt");
    assert_eq!(
        listed,
        format!("notes.txt 6\nsrc/App/Main.fl {}\n", MAIN.len())
    );
    assert!(
        stderr.contains("literal points at its first module"),
        "{stderr}"
    );
    assert!(stderr.contains("src/App/Main.fl:1:1"), "{stderr}");
    assert!(stderr.contains("literal helps"), "{stderr}");

    let out = flick(&root, &["literal-resume"]);
    let (_, stderr) = text(&out);
    assert!(out.status.success(), "{stderr}");
    let resumed = fs::read_to_string(app.join("resumed.txt")).expect("resumed.txt");
    assert_eq!(resumed, "resumed r1 copy.txt: from data\n");
    // the step's output went to `resume`, not into the project
    assert!(!app.join("copy.txt").exists());

    let out = flick(&root, &["literal-sneak"]);
    let (_, stderr) = text(&out);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("FLICK-P055")
            && stderr.contains("reads `src`, which the command does not declare"),
        "{stderr}"
    );
}

fn serve_plugin_tarball(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().map_while(Result::ok) {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            if request.starts_with("GET /pairs-tpl.tgz ") {
                break;
            }
        }
    });
    format!("http://127.0.0.1:{port}/pairs-tpl.tgz")
}

#[test]
fn a_plugin_package_scaffolds_via_from_path_and_url_and_pins_in_lock() {
    let root = super::project("plugin_templates", &[]);
    let store = root.join("store");
    let pkg = root.join("pairs-pkg");
    fs::create_dir_all(&pkg).expect("pkg");
    fs::write(pkg.join("flick.toml"), LIB).expect("manifest");
    fs::copy(component(), pkg.join("pairs.wasm")).expect("component");
    let wasm_sha = sha256(&fs::read(pkg.join("pairs.wasm")).expect("bytes"));

    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(&root)
            .env("FLICK_STORE", &store)
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("NO_COLOR", "1")
            .output()
            .expect("flick runs")
    };

    // Omitting `--template` picks the plugin's sole template `pair-app`.
    let out = run(&["new", "from-plugin", "--from", pkg.to_str().expect("utf8")]);
    assert!(out.status.success(), "{:?}", text(&out));
    assert!(
        text(&out)
            .0
            .contains("Next: pairs template `from-plugin` ready")
    );
    let created = root.join("from-plugin");
    let notes = fs::read_to_string(created.join("NOTES.md")).expect("notes");
    assert!(notes.contains("pair-app from-plugin"), "{notes}");
    let lock = fs::read_to_string(created.join("flick.lock")).expect("lock");
    assert!(
        lock.contains("[[template]]")
            && lock.contains("template = \"pair-app\"")
            && lock.contains("package = \"pairs\"")
            && lock.contains("source = \"path+../pairs-pkg\"")
            && lock.contains(&format!("component = \"sha256:{wasm_sha}\"")),
        "{lock}"
    );
    let checked = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["check"])
        .current_dir(&created)
        .env("FLICK_STORE", &store)
        .output()
        .expect("check");
    assert!(checked.status.success(), "{:?}", text(&checked));

    // Tarball URL with `--sha256` works for a library plugin package too.
    let tgz = root.join("pairs-tpl.tgz");
    let packed = Command::new("tar")
        .args(["czf", "pairs-tpl.tgz", "pairs-pkg"])
        .env("COPYFILE_DISABLE", "1")
        .current_dir(&root)
        .status()
        .expect("tar");
    assert!(packed.success());
    let tgz_bytes = fs::read(&tgz).expect("tgz");
    let tgz_sha = sha256(&tgz_bytes);
    let url = serve_plugin_tarball(tgz_bytes);
    let out = run(&[
        "new",
        "from-plugin-url",
        "--from",
        &url,
        "--sha256",
        &tgz_sha,
    ]);
    assert!(out.status.success(), "{:?}", text(&out));
    let url_lock = fs::read_to_string(root.join("from-plugin-url/flick.lock")).expect("lock");
    assert!(
        url_lock.contains("[[template]]")
            && url_lock.contains("template = \"pair-app\"")
            && url_lock.contains(&format!("archive = \"sha256:{tgz_sha}\"")),
        "{url_lock}"
    );

    // Incompatible `[package] flick` constraint fails with FLICK-P033 and creates nothing.
    let bad_pkg = root.join("old-pkg");
    fs::create_dir_all(&bad_pkg).expect("bad pkg");
    fs::write(
        bad_pkg.join("flick.toml"),
        LIB.replace(
            "version = \"1.0.0\"",
            "version = \"1.0.0\"\nflick = \"<0.0.1\"",
        ),
    )
    .expect("manifest");
    fs::copy(component(), bad_pkg.join("pairs.wasm")).expect("component");
    let bad = run(&["new", "too-new", "--from", bad_pkg.to_str().expect("utf8")]);
    assert!(!bad.status.success());
    assert!(text(&bad).1.contains("FLICK-P033"), "{:?}", text(&bad));
    assert!(!root.join("too-new").exists());
}
