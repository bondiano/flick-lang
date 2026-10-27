//! `run --devtools`, on Node with the workspace's runtime. Ignored by
//! default: part of `just e2e`.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use common::{APP, project};

/// A `DevTools` server that prints its port, then the name of the first span
/// it gets within a minute.
const DEVTOOLS: &str = "import { devToolsListen } from \"@flick-lang/runtime/effect/devtools/DevToolsServer\";
const server = await devToolsListen();
console.log(server.port);
for (let i = 0; i < 3000 && server.spans.length === 0; i += 1) await new Promise((resolve) => setTimeout(resolve, 20));
console.log(server.spans[0] ?? \"no span\");
await server.close();
";

#[test]
#[ignore = "needs Node and the installed runtime"]
fn run_with_devtools_sends_the_spans_there() {
    let main = "module App.Main exposing (main)

options : Tracer.SpanOptions
let options =
  {
    attributes = [];
    links = [];
    parent = None;
    root = false;
    kind_ = Tracer.internal;
    sampled = None;
    level = None;
    captureStackTrace = false;
    disablePropagation = false;
  }

main : Unit ->{; AppError} Unit
let main () = Tracer.with_span \"App.save\" options Effect.void
";
    let dir = project(
        "run_devtools",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", main),
            ("devtools.mjs", DEVTOOLS),
        ],
    );
    let mut server = Command::new("node")
        .arg("devtools.mjs")
        .current_dir(&dir)
        .stdout(Stdio::piped())
        .spawn()
        .expect("node runs");
    let mut lines = BufReader::new(server.stdout.take().expect("piped stdout")).lines();
    let port = lines.next().expect("the port").expect("a line");
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["run", &format!("--devtools=ws://127.0.0.1:{port}")])
        .current_dir(&dir)
        .output()
        .expect("flick runs");
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // the server logs to stdout too, as a client disconnects
    let got = lines.map_while(Result::ok).any(|line| line == "App.save");
    let _ = server.wait();
    assert!(got, "the DevTools got no span `App.save`");
}
