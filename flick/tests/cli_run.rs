//! `run`, `dev` and the exit codes, on Node with the workspace's runtime
//! (projects live in the target directory inside the repository). Ignored
//! by default: part of `just e2e`.

mod common;

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{fs, thread};

use common::{APP, project};
use flick_core::build::runner_path;

const FOREVER: &str =
    "import { Effect } from \"@flick-lang/runtime\";\nexport const forever = () => Effect.never;\n";

/// An app that prints `started <v>`, waits for a signal and prints
/// `cleanup <v>` when its scope closes, failing there when `broken`.
fn waiting(version: &str, broken: bool) -> String {
    let release = if broken {
        "crash \"{\"".to_owned()
    } else {
        format!("Platform.stdout_line \"cleanup {version}\"")
    };
    format!(
        "module App.Main exposing (main)

forever : Unit ->{{}} Unit
foreign effect forever from \"./forever.mjs\" \"forever\"

crash : String ->{{}} Unit
foreign sync crash from global \"JSON.parse\"

main : Unit ->{{; AppError}} Unit
let main () =
  scope {{
    Resource.acquire_release (task {{ Platform.stdout_line \"started {version}\" }}) (fun _ -> {release})
    forever ()
  }}
"
    )
}

fn flick(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_flick"));
    command.args(args).current_dir(dir);
    command
}

fn run(dir: &Path, args: &[&str]) -> Output {
    flick(dir, args).output().expect("flick runs")
}

fn signal(child: &Child, name: &str) {
    Command::new("kill")
        .args(["-s", name, &child.id().to_string()])
        .status()
        .expect("kill");
}

/// Lines of the child's stdout as they come.
fn lines(child: &mut Child) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().expect("piped stdout");
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

fn expect_line(rx: &mpsc::Receiver<String>, wanted: &str) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(line) if line == wanted => return,
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {
                panic!("no line `{wanted}`")
            }
        }
    }
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn run_builds_javascript_even_for_a_typescript_project() {
    let config = format!("{APP}\n[build]\nemit = \"ts\"\n");
    let main = "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (show (Platform.args ()))\n";
    let dir = project(
        "run_ts",
        &[("flick.toml", &config), ("src/App/Main.fl", main)],
    );
    let out = run(&dir, &["run", "--", "a", "b"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "[\"a\", \"b\"]\n");
    assert!(
        fs::read_dir(dir.join(".flick/run"))
            .expect("generations")
            .count()
            == 0,
        "the generation goes after the run"
    );
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn exit_codes_follow_the_outcome_and_cleanup() {
    let fail = "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = App.fail \"nope\"\n";
    let cases: [(&str, String, Option<&str>, i32); 5] = [
        (
            "exit_ok",
            waiting("v", false).replace("forever ()", "()"),
            None,
            0,
        ),
        ("exit_fail", fail.to_owned(), None, 1),
        ("exit_int", waiting("v", false), Some("INT"), 130),
        ("exit_term", waiting("v", false), Some("TERM"), 143),
        ("exit_cleanup", waiting("v", true), Some("TERM"), 1),
    ];
    for (name, source, signal_name, expected) in cases {
        let dir = project(
            name,
            &[
                ("flick.toml", APP),
                ("src/App/Main.fl", &source),
                ("src/App/forever.mjs", FOREVER),
            ],
        );
        let mut child = flick(&dir, &["run"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        let rx = lines(&mut child);
        if let Some(s) = signal_name {
            expect_line(&rx, "started v");
            signal(&child, s);
        }
        let status = child.wait().expect("wait");
        assert_eq!(status.code(), Some(expected), "{name}");
    }
    let dir = project(
        "exit_usage",
        &[(
            "flick.toml",
            "schema-version = 1\n[project]\nkind = \"lib\"\n",
        )],
    );
    assert_eq!(
        run(&dir, &["run"]).status.code(),
        Some(2),
        "a library has no main"
    );
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn json_diagnostics_own_the_whole_stderr() {
    let main = "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () =\n  Platform.stderr_line \"\\u{1b}[31mred\\u{1b}[0m\"\n  App.fail \"nope\"\n";
    let dir = project(
        "run_json",
        &[("flick.toml", APP), ("src/App/Main.fl", main)],
    );
    let out = run(&dir, &["--diagnostics", "json", "run"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    let events: Vec<serde_json::Value> = stderr
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("not NDJSON: {l:?}")))
        .collect();
    assert!(events.iter().all(|e| e["schema-version"] == 1));
    assert!(!stderr.contains('\u{1b}'), "no ANSI codes");
    assert!(
        events
            .iter()
            .any(|e| e["type"] == "application-stderr" && e["text"] == "red"),
        "{stderr}"
    );
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn simultaneous_runs_keep_the_survivors_files() {
    let dir = project(
        "run_generations",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", &waiting("v1", false)),
            ("src/App/forever.mjs", FOREVER),
        ],
    );
    let mut first = flick(&dir, &["run"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    let first_path = dir.join(".flick/run").join(format!("{}-1", first.id()));
    let mut second = flick(&dir, &["run"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    let second_path = dir.join(".flick/run").join(format!("{}-1", second.id()));
    let rx = lines(&mut first);
    let rx2 = lines(&mut second);
    expect_line(&rx, "started v1");
    expect_line(&rx2, "started v1");
    assert_ne!(first_path, second_path);
    let module = second_path.join("App/Main.js");
    let before = fs::read_to_string(&module).expect("survivor artifact");
    assert!(first_path.join("App/Main.js").exists());
    assert_eq!(run(&dir, &["clean"]).status.code(), Some(0));
    assert!(module.exists(), "clean keeps both running generations");
    signal(&first, "TERM");
    assert_eq!(first.wait().expect("first cleanup").code(), Some(143));
    assert!(
        !first_path.exists(),
        "only the first reservation is released"
    );
    assert!(second.try_wait().expect("survivor status").is_none());
    assert_eq!(
        fs::read_to_string(&module).expect("surviving artifact"),
        before
    );
    let late_import = Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            "await import(process.argv[1]);",
        ])
        .arg(&module)
        .output()
        .expect("late import");
    assert!(late_import.status.success(), "{late_import:?}");
    assert_eq!(run(&dir, &["clean"]).status.code(), Some(0));
    assert!(module.exists(), "clean still keeps the survivor");
    signal(&second, "TERM");
    assert_eq!(second.wait().expect("second cleanup").code(), Some(143));
    assert!(!second_path.exists());
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn dev_replaces_the_app_only_after_its_shutdown() {
    let dir = project(
        "dev_cycle",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", &waiting("v1", false)),
            ("src/App/forever.mjs", FOREVER),
        ],
    );
    let main = dir.join("src/App/Main.fl");
    let mut dev = flick(&dir, &["--quiet", "dev"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let rx = lines(&mut dev);
    expect_line(&rx, "started v1");
    // a build that fails leaves v1 running
    thread::sleep(Duration::from_millis(1100));
    fs::write(
        &main,
        "module App.Main exposing (main)\nlet main () = nope\n",
    )
    .expect("write");
    thread::sleep(Duration::from_secs(2));
    assert!(rx.try_recv().is_err(), "nothing restarted");
    fs::write(&main, waiting("v2", true)).expect("write");
    expect_line(&rx, "cleanup v1");
    expect_line(&rx, "started v2");
    // v2 cannot clean up: the next change stops the restarts
    thread::sleep(Duration::from_millis(1100));
    fs::write(&main, waiting("v3", false)).expect("write");
    let status = dev.wait().expect("wait");
    assert_eq!(status.code(), Some(1));
    assert!(rx.iter().all(|l| l != "started v3"), "v3 never starts");
    let mut err = String::new();
    dev.stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut err)
        .expect("read");
    // the final Cause keeps the finalizer's defect with its stack, and it turns
    // the interrupted v2 into a failure (exit 1, not 143)
    assert!(
        err.contains("SyntaxError") && err.contains("at JSON.parse"),
        "the cleanup failure is reported: {err}"
    );
    assert!(
        err.contains("FLICK-P021") && err.contains("exit code 1)"),
        "{err}"
    );
}

/// Logs `aborted` when interrupted but settles only after its own delay.
const LINGER: &str = "export const linger = (signal) =>
  new Promise((resolve) => {
    signal.addEventListener(\"abort\", () => console.log(\"aborted v1\"));
    setTimeout(() => { console.log(\"settled v1\"); resolve(); }, 5000);
  });
";

const LINGERING: &str = "module App.Main exposing (main)

linger : Unit ->{} Unit
foreign async linger from \"./linger.mjs\" \"linger\"
  arity 0
  cancel signal

main : Unit ->{; AppError} Unit
let main () =
  scope {
    Resource.acquire_release (task { Platform.stdout_line \"started v1\" }) (fun _ -> Platform.stdout_line \"cleanup v1\")
    linger ()
  }
";

#[test]
#[ignore = "needs Node and the installed runtime"]
fn dev_starts_the_next_version_after_foreign_work_settles() {
    let dir = project(
        "dev_quiescence",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", LINGERING),
            ("src/App/linger.mjs", LINGER),
            ("src/App/forever.mjs", FOREVER),
        ],
    );
    let mut dev = flick(&dir, &["--quiet", "dev"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    let rx = lines(&mut dev);
    expect_line(&rx, "started v1");
    thread::sleep(Duration::from_millis(1100));
    fs::write(dir.join("src/App/Main.fl"), waiting("v2", false)).expect("write");
    let mut seen = Vec::new();
    while seen.last().is_none_or(|l| l != "started v2") {
        let line = rx.recv_timeout(Duration::from_secs(60)).expect("a line");
        seen.push(line);
    }
    signal(&dev, "TERM");
    let _ = dev.wait();
    // the promise ignored the abort, yet v2 waited for it and for the finalizer
    assert_eq!(
        seen,
        ["aborted v1", "settled v1", "cleanup v1", "started v2"],
        "after started v1"
    );
}

#[test]
#[ignore = "needs Node, pnpm and the installed runtime"]
fn pack_writes_a_complete_package_that_runs_nothing_on_import() {
    let config = "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[build]\nemit-dts = true\n";
    let source = "module Lib.Greeting exposing (greet)\n\ngreet : String -> String\nlet greet name = \"Hello, \" ++ name\n";
    let manifest = "{ \"name\": \"@acme/greeting\", \"version\": \"1.2.3\" }\n";
    let dir = project(
        "pack_lib",
        &[
            ("flick.toml", config),
            ("src/Lib/Greeting.fl", source),
            ("package.json", manifest),
        ],
    );
    let out = run(&dir, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let tarball = dir.join("artifacts/acme-greeting-1.2.3.tgz");
    let listing = Command::new("tar")
        .arg("-tzf")
        .arg(&tarball)
        .output()
        .expect("tar");
    let listing = String::from_utf8_lossy(&listing.stdout);
    for file in [
        "package/dist/Lib/Greeting.js",
        "package/dist/Lib/Greeting.iface",
        "package/dist/Lib/Greeting.d.ts",
        "package/dist/_core/Lib/Greeting.js",
        "package/package.json",
    ] {
        assert!(listing.contains(file), "{file} in\n{listing}");
    }
    assert!(
        !listing.contains(runner_path(false)),
        "a library has no runner"
    );
    let probe = dir.join(".flick/pack/probe.mjs");
    fs::write(
        &probe,
        "const m = await import(\"./dist/Lib/Greeting.js\");\nconst core = await import(\"./dist/_core/Lib/Greeting.js\");\nconsole.log(typeof m.greet);\nconsole.log(m.greet(\"Flick\"));\nconsole.log(core.greet(\"Flick\"));\n",
    )
    .expect("write");
    let imported = Command::new("node").arg(&probe).output().expect("node");
    assert_eq!(
        String::from_utf8_lossy(&imported.stdout),
        "function\nHello, Flick\nHello, Flick\n",
        "{imported:?}"
    );

    // an export the package lacks is refused before any tarball
    let broken = "{ \"name\": \"@acme/broken\", \"version\": \"1.0.0\", \"exports\": { \".\": \"./dist/Lib/Missing.js\" } }\n";
    fs::write(dir.join("package.json"), broken).expect("write");
    let out = run(&dir, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-P020"));
    assert!(!dir.join("artifacts/acme-broken-1.0.0.tgz").exists());

    // `todo` and `echo` stay out of packages
    fs::write(dir.join("package.json"), manifest).expect("write");
    let unfinished = source.replace("\"Hello, \" ++ name", "echo (todo \"greet\")");
    fs::write(dir.join("src/Lib/Greeting.fl"), unfinished).expect("write");
    let out = run(&dir, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    for code in ["FLICK-T022", "FLICK-T023", "FLICK-P024"] {
        assert!(stderr.contains(code), "{code} in {stderr}");
    }
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn json_reporter_and_json_diagnostics_keep_their_streams() {
    let suite = "module Test.Noisy exposing (tests)\n\nnote : String ->{} Unit\nlet note text = Platform.stderr_line text\n\ntests : List Test.Test\nlet tests = [Test.effect_test \"logs\" (task { note \"to stderr\" })]\n";
    let config = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";
    let dir = project(
        "test_streams",
        &[("flick.toml", config), ("test/Test/Noisy.fl", suite)],
    );
    let out = run(
        &dir,
        &["--diagnostics", "json", "test", "--reporter", "json"],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|_| panic!("stdout not JSON: {line:?}"));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let events: Vec<serde_json::Value> = stderr
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("stderr not NDJSON: {l:?}")))
        .collect();
    assert!(
        events
            .iter()
            .any(|e| e["type"] == "application-stderr" && e["text"] == "to stderr"),
        "{stderr}"
    );
}

/// The example sessions of the CLI specification, without the commands that reach the
/// npm registry (`install`, `add`), on a project from `flick new`.
#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_spec_examples_and_the_ci_set_pass() {
    let root = project("spec_examples", &[]);
    fs::create_dir_all(&root).expect("dir");
    assert_eq!(
        run(&root, &["new", "my-app", "--template", "app"])
            .status
            .code(),
        Some(0)
    );
    let app = root.join("my-app");
    let domain = "module App.Domain exposing (User, user_json, api)\n\ntype alias User = { name : String; age : Number }\n  deriving Schema\n\nuser_json : PureCodec Json User\nlet user_json = Codec.json (schema User)\n\nme : HttpApi.Endpoint \"me\" (HttpApi.Request Unit Unit Unit Unit ->{r;} User)\nlet me =\n  HttpApi.endpoint HttpApi.Get \"/me\" (Record.label : Record.Label \"me\")\n    |> HttpApi.with_success 200 user_json\n\napi : HttpApi.Api { users : { me : HttpApi.Request Unit Unit Unit Unit ->{r;} User } }\nlet api =\n  HttpApi.api \"Users\"\n    |> HttpApi.add_group (HttpApi.group (Record.label : Record.Label \"users\") |> HttpApi.add me)\n";
    fs::write(app.join("src/App/Domain.fl"), domain).expect("write");
    let sessions: [&[&str]; 12] = [
        &["check"],
        &["fmt", "--check"],
        &["check", "--diagnostics", "json"],
        &["test", "--filter", "Greeting", "--jobs", "4"],
        &["build", "--profile", "release"],
        &["run", "--profile", "release", "--", "--port", "3000"],
        &[
            "schema",
            "export",
            "App.Domain.user_json",
            "--target",
            "json-schema-2020-12",
            "--out",
            "schema/user.schema.json",
        ],
        &[
            "schema",
            "export",
            "App.Domain.api",
            "--target",
            "openapi-3.1",
            "--out",
            "schema/openapi.json",
        ],
        &["doc", "--out-dir", "docs"],
        &["check", "--warnings-as-errors"],
        &["test", "--profile", "release"],
        &["build", "--profile", "release"],
    ];
    for args in sessions {
        let out = run(&app, args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
    }
    let schema = fs::read_to_string(app.join("schema/user.schema.json")).expect("schema");
    assert!(
        schema.contains("\"age\"") && schema.contains("2020-12"),
        "{schema}"
    );
    let openapi = fs::read_to_string(app.join("schema/openapi.json")).expect("openapi");
    assert!(
        openapi.contains("\"3.1.0\"") && openapi.contains("\"/me\""),
        "{openapi}"
    );
}
