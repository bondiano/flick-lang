//! The Conduit example against the `RealWorld` spec: builds the Go server of
//! `examples/conduit/server`, which compiles the contract, its path
//! dependency, from its sources, serves it over a fresh `PostgreSQL` in Docker and runs the
//! official Hurl suite vendored in `examples/conduit/vendor/realworld`.
//! Ignored by default: it needs Docker, Go and `hurl` on `PATH`
//! (`just conduit-api`).

mod common;

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use common::postgres::start;
use flick_core::build::go::executable;
use flick_core::project::config::MAIN_ENTRY;

/// The server, stopped when dropped.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A port nothing listens on now.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .expect("a free port")
        .port()
}

#[test]
#[ignore = "needs Docker, Go and hurl"]
fn the_go_server_passes_the_realworld_api_suite() {
    let conduit = common::example("conduit", "conduit_api");
    let server = conduit.join("server");
    let build = common::flick_in(&server, &["build"]);
    assert!(build.status.success(), "{build:?}");
    let (_database, url) = start();
    let port = free_port();
    let host = format!("http://127.0.0.1:{port}");
    let _running = Server(
        Command::new(server.join("dist").join(executable(MAIN_ENTRY)))
            .env("DATABASE_URL", &url)
            .env("JWT_SECRET", "conduit-test-secret")
            .env("PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the server runs"),
    );
    let started = Instant::now();
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(
            started.elapsed() < Duration::from_mins(1),
            "the server did not start"
        );
        thread::sleep(Duration::from_millis(100));
    }
    let suite = conduit.join("vendor/realworld/specs/api/hurl");
    let mut files: Vec<_> = fs::read_dir(&suite)
        .expect("the suite")
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "hurl"))
        .collect();
    files.sort();
    let hurl = Command::new("hurl")
        .args(["--test", "--jobs", "1", "--variable"])
        .arg(format!("host={host}"))
        .args(["--variable", "uid=flick"])
        .args(&files)
        .output()
        .expect("`hurl` is on PATH: `nix shell nixpkgs#hurl`");
    assert!(
        hurl.status.success(),
        "{}",
        String::from_utf8_lossy(&hurl.stderr)
    );
}
