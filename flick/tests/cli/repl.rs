//! CLI integration tests for `flick repl` on the live in-process VM (`#999`).
//!
//! Covers all six acceptance criteria without requiring a TTY:
//! 1. Persistent bindings (`let n = 41` then `n + 1` -> `42 : Int`).
//! 2. Host effects under `Host.live` (`Platform.stdout_line "hi"` -> `hi` + `() : Unit`).
//! 3. Unhandled user effect reports its name + `:handle` hint; after `:handle`, it runs.
//! 4. Auto-await of tasks (`task { 7 }` -> `7 : Int`) and typed failure without crashing.
//! 5. Ctrl-C / `SIGINT` interrupting an infinite loop inside a scope with a finalizer
//!    runs its finalizer once, returns to the prompt, and preserves earlier bindings.
//! 6. `:bg` on a looping task returns immediately, `:jobs` lists it, and `:kill 1`
//!    stops it with scope finalization.

use std::io::{BufRead, BufReader, Write as _};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn run_repl_piped(input: &str) -> Output {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["repl"])
        .current_dir(&repo)
        .env("PATH", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn flick repl");
    {
        let mut stdin = child.stdin.take().expect("piped stdin");
        stdin
            .write_all(input.as_bytes())
            .expect("write repl script");
    }
    child.wait_with_output().expect("wait flick repl")
}

#[test]
fn repl_persistent_bindings_and_host_effects() {
    let script = "\
let n = 41
n + 1
Platform.stdout_line \"hi\"
:quit
";
    let out = run_repl_piped(script);
    assert!(
        out.status.success(),
        "repl failed: status={:?}\nstdout={}\nstderr={}",
        out.status,
        stdout(&out),
        stderr(&out)
    );
    let out_text = stdout(&out);
    let lines: Vec<&str> = out_text.lines().collect();
    assert_eq!(lines, ["42 : Int", "hi", "() : Unit"]);
}

#[test]
fn repl_unhandled_effect_reports_hint_and_runs_after_handle_and_provide() {
    let script = "\
effect Ask =
  ask : Unit ->{} Int

Ask.ask ()
:handle fun body -> try body () with | Ask.ask () -> 42
Ask.ask ()
service Cfg =
  port : Unit ->{} Int

:provide Layer.succeed Cfg { port = fun () -> 8080 }
Cfg.port ()
:quit
";
    let out = run_repl_piped(script);
    assert!(
        out.status.success(),
        "repl failed: status={:?}\nstdout={}\nstderr={}",
        out.status,
        stdout(&out),
        stderr(&out)
    );
    let err = stderr(&out);
    assert!(
        err.contains("Ask") && err.contains(":handle"),
        "expected unhandled effect name and :handle hint in stderr, got:\n{err}"
    );
    let out_text = stdout(&out);
    assert!(
        out_text.contains("42 : Int"),
        "expected handled `Ask.ask ()` to print `42 : Int`, got:\n{out_text}"
    );
    assert!(
        out_text.contains("8080 : Int"),
        "expected provided `Cfg.port ()` to print `8080 : Int`, got stdout:\n{out_text}\nstderr:\n{err}"
    );
}

#[test]
fn repl_auto_awaits_tasks_and_continues_after_typed_error() {
    let script = "\
type error Boom = Boom String
task { 7 }
task { Fail.fail (Boom \"oops\") }
10 + 20
:quit
";
    let out = run_repl_piped(script);
    assert!(
        out.status.success(),
        "repl failed: status={:?}\nstdout={}\nstderr={}",
        out.status,
        stdout(&out),
        stderr(&out)
    );
    let out_text = stdout(&out);
    let err_text = stderr(&out);
    assert!(
        out_text.contains("7 : Int"),
        "expected `task {{ 7 }}` to print `7 : Int`, got:\n{out_text}"
    );
    assert!(
        err_text.contains("Boom") && err_text.contains("oops"),
        "expected typed error in stderr, got:\n{err_text}"
    );
    assert!(
        out_text.contains("30 : Int"),
        "expected session to continue after typed error, got:\n{out_text}"
    );
}

#[cfg(unix)]
#[test]
fn repl_sigint_interrupts_infinite_loop_runs_finalizer_once_and_keeps_bindings() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["repl"])
        .current_dir(&repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn flick repl");

    let pid = child.id();
    let mut stdin = child.stdin.take().expect("piped stdin");
    let child_stdout = child.stdout.take().expect("piped stdout");
    let mut reader = BufReader::new(child_stdout);

    // 1. Bind `n = 41` and warm up `Scope` + `Platform` in the VM cache.
    writeln!(stdin, "let n = 41").expect("write binding");
    writeln!(stdin, "scope {{ Scope.add_finalizer (task {{ () }}); 1 }}").expect("write warmup");
    stdin.flush().expect("flush warmup");

    let mut line = String::new();
    reader.read_line(&mut line).expect("read warmup result");
    assert_eq!(line.trim(), "1 : Int");

    // 2. Start an infinite loop inside a scope with a finalizer.
    writeln!(
        stdin,
        "scope {{\n  Scope.add_finalizer (task {{ Platform.stdout_line \"finalizer ran\" }})\n  loop {{ () }}\n}}"
    )
    .expect("write infinite loop");
    stdin.flush().expect("flush infinite loop");

    // Give the already-warm VM plenty of time to enter the scope and spin in `loop`.
    thread::sleep(Duration::from_millis(250));
    assert!(
        child.try_wait().expect("try_wait").is_none(),
        "repl exited unexpectedly before SIGINT"
    );

    // 3. Deliver real OS SIGINT (no TTY).
    let status = Command::new("kill")
        .args(["-s", "INT", &pid.to_string()])
        .status()
        .expect("send SIGINT");
    assert!(status.success(), "kill -s INT failed");

    // 4. Finalizer runs once and control returns to the prompt.
    line.clear();
    reader.read_line(&mut line).expect("read finalizer output");
    assert_eq!(line.trim(), "finalizer ran");

    // 5. Earlier binding `n = 41` is still usable!
    writeln!(stdin, "n + 1").expect("write post-interrupt expression");
    writeln!(stdin, ":quit").expect("write quit");
    stdin.flush().expect("flush post-interrupt");
    drop(stdin);

    let mut rest = Vec::new();
    for entry in reader.lines() {
        rest.push(entry.expect("read remaining stdout line"));
    }
    assert_eq!(rest, ["42 : Int"]);

    let output = child.wait_with_output().expect("wait repl exit");
    assert!(
        output.status.success(),
        "repl exited unsuccessfully after SIGINT: {:?}\nstderr={}",
        output.status,
        stderr(&output)
    );
    let err = stderr(&output);
    assert!(
        err.contains("interrupted"),
        "expected `interrupted` notice on stderr, got:\n{err}"
    );
}

#[test]
fn repl_bg_jobs_and_kill_finalize_cleanly() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["repl"])
        .current_dir(&repo)
        .env("PATH", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn flick repl");

    let mut stdin = child.stdin.take().expect("piped stdin");
    let child_stdout = child.stdout.take().expect("piped stdout");
    let mut reader = BufReader::new(child_stdout);

    // Start a looping background task with a scope finalizer; returns immediately.
    writeln!(
        stdin,
        ":bg task {{\n  scope {{\n    Scope.add_finalizer (task {{ Platform.stdout_line \"bg finalized\" }})\n    loop {{ () }}\n  }}\n}}"
    )
    .expect("write :bg");
    stdin.flush().expect("flush :bg");

    let mut line = String::new();
    reader.read_line(&mut line).expect("read :bg started");
    assert_eq!(line.trim(), "[1] started");

    // List active background jobs.
    writeln!(stdin, ":jobs").expect("write :jobs");
    stdin.flush().expect("flush :jobs");
    line.clear();
    reader.read_line(&mut line).expect("read :jobs output");
    assert!(
        line.contains("[1]") && line.contains("running"),
        "expected running job 1 in :jobs output, got: {line:?}"
    );

    // Give the background thread a moment to enter the scope, then kill it.
    thread::sleep(Duration::from_millis(50));
    writeln!(stdin, ":kill 1").expect("write :kill 1");
    writeln!(stdin, ":quit").expect("write :quit");
    stdin.flush().expect("flush :kill and :quit");
    drop(stdin);

    let mut remaining = Vec::new();
    for entry in reader.lines() {
        remaining.push(entry.expect("read remaining lines"));
    }
    assert_eq!(
        remaining,
        ["bg finalized", "[1] killed"],
        "expected finalizer output followed by killed status"
    );

    let output = child.wait_with_output().expect("wait repl exit");
    assert!(
        output.status.success(),
        "repl failed after :kill: {:?}\nstderr={}",
        output.status,
        stderr(&output)
    );
}
