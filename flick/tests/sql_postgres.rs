//! E2E against `PostgreSQL`: starts `postgres:17-alpine` in Docker, loads
//! `tests/postgres/schema.sql`, builds a program of `tests/postgres/` and
//! compares its stdout with its `.stdout` file: `app.fl`, the queries, and
//! `tx.fl`, transactions and cancellation. Ignored by default: it needs
//! Docker and the installed runtime (`just e2e-postgres`).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::{fs, thread};

use flick_core::build::runner_path;

mod common;

const IMAGE: &str = "postgres:17-alpine";

/// The container, removed when dropped.
struct Container(String);

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker").args(["rm", "-f", &self.0]).output();
    }
}

fn docker(args: &[&str]) -> String {
    let out = Command::new("docker")
        .args(args)
        .output()
        .expect("docker runs");
    assert!(out.status.success(), "docker {args:?}: {out:?}");
    String::from_utf8(out.stdout)
        .expect("utf-8")
        .trim()
        .to_owned()
}

fn start() -> (Container, String) {
    let id = docker(&["run", "-d", "-e", "POSTGRES_PASSWORD=flick", IMAGE]);
    let container = Container(id.clone());
    let started = Instant::now();
    // the server initializing listens on its socket only; wait for TCP
    while Command::new("docker")
        .args([
            "exec",
            &id,
            "psql",
            "-h",
            "127.0.0.1",
            "-U",
            "postgres",
            "-c",
            "select 1",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| !s.success())
    {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "postgres did not start"
        );
        thread::sleep(Duration::from_millis(300));
    }
    // the bridge address: published ports are not reachable everywhere
    let ip = docker(&[
        "inspect",
        "-f",
        "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}",
        &id,
    ]);
    (
        container,
        format!("postgres://postgres:flick@{ip}:5432/postgres"),
    )
}

/// A server with the schema loaded.
fn prepared() -> (Container, String) {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let (container, url) = start();
    let schema = fs::read_to_string(repo.join("tests/postgres/schema.sql")).expect("schema");
    let mut psql = Command::new("docker")
        .args([
            "exec",
            "-i",
            &container.0,
            "psql",
            "-h",
            "127.0.0.1",
            "-U",
            "postgres",
            "-v",
            "ON_ERROR_STOP=1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("psql runs");
    psql.stdin
        .as_mut()
        .expect("stdin")
        .write_all(schema.as_bytes())
        .expect("schema written");
    drop(psql.stdin.take());
    assert!(psql.wait().expect("psql ends").success(), "schema loads");
    (container, url)
}

/// Builds and runs `tests/postgres/<name>.fl` against `url`, checking its
/// stdout against `<name>.stdout`.
fn run_program(name: &str, url: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/postgres");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("postgres_{name}"));
    let _ = fs::remove_dir_all(&out);
    let source = dir.join(format!("{name}.fl"));
    let build = common::flick(&[
        "build",
        source.to_str().expect("utf-8 path"),
        "--out-dir",
        out.to_str().expect("utf-8 path"),
    ]);
    assert!(build.status.success(), "build: {build:?}");
    let run = Command::new("node")
        .arg(out.join(runner_path(false)))
        .env("DATABASE_URL", url)
        .output()
        .expect("node runs");
    let stdout = String::from_utf8_lossy(&run.stdout);
    let expected = fs::read_to_string(dir.join(format!("{name}.stdout"))).expect("expected stdout");
    assert_eq!(
        stdout,
        expected,
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.status.success(), "{run:?}");
}

#[test]
#[ignore = "needs Docker and the installed runtime"]
fn queries_against_postgres() {
    let (container, url) = prepared();
    run_program("app", &url);
    // the payload stayed data: the table is still there
    let count = docker(&[
        "exec",
        &container.0,
        "psql",
        "-U",
        "postgres",
        "-tAc",
        "select count(*) from users",
    ]);
    assert_eq!(count, "2");
}

#[test]
#[ignore = "needs Docker and the installed runtime"]
fn transactions_and_cancellation_against_postgres() {
    let (container, url) = prepared();
    run_program("tx", &url);
    // no cancelled query is left running on the server
    let sleeping = docker(&[
        "exec",
        &container.0,
        "psql",
        "-U",
        "postgres",
        "-tAc",
        "select count(*) from pg_stat_activity where query = 'select pg_sleep(30)'",
    ]);
    assert_eq!(sleeping, "0");
}
