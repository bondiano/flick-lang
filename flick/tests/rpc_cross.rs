//! Rpc and `server fn` across targets: a server built for one target
//! answers a client built for the other, over HTTP and WebSocket, in each
//! serialization, with typed errors, streams and auth, and a frontend on
//! another origin meets the same CORS answers from both servers. Ignored
//! by default: part of `just e2e`, needs Go and Node.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use common::{APP, flick_in, project};
use flick_core::build::go::executable;
use flick_core::build::runner_path;

const API: &str = include_str!("rpc_cross/Api.fl");

/// What the client prints, whichever target serves it.
const EXPECTED: &str = "\
http ndjson: 42, not found -1, [1, 2, 3], [], not found -2
http binary: 42, not found -1, [1, 2, 3], [], not found -2
http json_rpc: 42, not found -1, [1, 2, 3], [], not found -2
ws json: 42, not found -1, [1, 2, 3], [], not found -2
ws binary: 42, not found -1, [1, 2, 3], [], not found -2
ws connection: 2, 4, [1, 2]
auth http: ada, refused
auth ws: ada, refused
server fn: hoho, ada, refused
";

/// The project of `main`, built for both targets.
fn built(name: &str, main: &str) -> PathBuf {
    let dir = project(
        name,
        &[
            ("flick.toml", APP),
            (
                "package.json",
                r#"{"type":"module","dependencies":{"@flick-lang/effect-ts":"*"}}"#,
            ),
            ("src/App/Api.fl", API),
            ("src/App/Main.fl", main),
        ],
    );
    for emit in ["go", "js"] {
        let out = format!("dist-{emit}");
        let build = flick_in(&dir, &["build", "--emit", emit, "--out-dir", &out]);
        assert!(build.status.success(), "{name} for {emit}: {build:?}");
    }
    dir
}

/// The program of `dir` built for `emit`.
fn command(dir: &Path, emit: &str) -> Command {
    let out = dir.join(format!("dist-{emit}"));
    if emit == "go" {
        Command::new(out.join(executable("main")))
    } else {
        let mut node = Command::new("node");
        node.arg(out.join(runner_path(false)));
        node
    }
}

/// A running server, stopped when dropped.
struct Server {
    child: Child,
    ports: Vec<String>,
    base: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(dir: &Path, emit: &str) -> Server {
    let mut child = command(dir, emit)
        .stdout(Stdio::piped())
        .spawn()
        .expect("the server runs");
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx
        .recv_timeout(Duration::from_mins(1))
        .expect("the server prints its ports");
    let mut words: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
    assert_eq!(words.first().map(String::as_str), Some("ports"), "{line}");
    let base = words.pop().expect("the address of the server functions");
    Server {
        child,
        ports: words.split_off(1),
        base,
    }
}

/// The status and the `access-control-*` headers of a raw request.
fn cors(port: &str, method: &str, extra: &str, body: &str) -> (String, Vec<String>) {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("timeout");
    let request = format!(
        "{method} /rpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://elsewhere.test\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("writes");
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    let mut lines = answer.lines();
    let status = lines.next().unwrap_or_default().to_owned();
    let mut headers: Vec<String> = lines
        .take_while(|l| !l.is_empty())
        .map(str::to_ascii_lowercase)
        .filter(|l| l.starts_with("access-control-") || l.starts_with("vary"))
        .collect();
    headers.sort();
    (status, headers)
}

fn cors_answers(server: &Server) -> Vec<(String, Vec<String>)> {
    let port = &server.ports[0];
    let call =
        "{\"_tag\":\"Request\",\"id\":\"1\",\"tag\":\"double\",\"payload\":2,\"headers\":[]}\n";
    vec![
        cors(
            port,
            "OPTIONS",
            "Access-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: content-type\r\n",
            "",
        ),
        cors(port, "POST", "Content-Type: application/ndjson\r\n", call),
    ]
}

#[test]
#[ignore = "needs Go and Node"]
fn a_server_of_one_target_answers_a_client_of_the_other() {
    let server = built("rpc_cross_server", include_str!("rpc_cross/Server.fl"));
    let client = built("rpc_cross_client", include_str!("rpc_cross/Client.fl"));
    let mut answers = Vec::new();
    for (serving, calling) in [("js", "go"), ("go", "js")] {
        let running = serve(&server, serving);
        let mut call = command(&client, calling);
        for (i, port) in running.ports.iter().enumerate() {
            call.env(format!("P{i}"), port);
        }
        let out = call
            .env("FLICK_SERVER_URL", &running.base)
            .output()
            .expect("the client runs");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            EXPECTED,
            "{serving} server, {calling} client: {out:?}"
        );
        answers.push(cors_answers(&running));
    }
    assert_eq!(
        answers[0], answers[1],
        "CORS answers of the js and go servers"
    );
}
