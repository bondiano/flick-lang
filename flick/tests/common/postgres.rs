//! `PostgreSQL` in Docker for the tests that need a database.

use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const IMAGE: &str = "postgres:17-alpine";

/// The container, removed when dropped.
pub struct Container(pub String);

impl Drop for Container {
    fn drop(&mut self) {
        let _ = Command::new("docker").args(["rm", "-f", &self.0]).output();
    }
}

pub fn docker(args: &[&str]) -> String {
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

pub fn start() -> (Container, String) {
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
            started.elapsed() < Duration::from_mins(1),
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
