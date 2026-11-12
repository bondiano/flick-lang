//! `run --devtools`, on Node with the workspace's runtime. Ignored by
//! default: part of `just e2e`.

mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use common::{APP, project};

/// A `DevTools` server that prints its port, then the name of the first span
/// it gets within a minute.
const DEVTOOLS: &str = "import { devToolsListen } from \"@flick-lang/effect-ts/effect/devtools/DevToolsServer\";
const server = await devToolsListen();
console.log(server.port);
for (let i = 0; i < 3000 && server.spans.length === 0; i += 1) await new Promise((resolve) => setTimeout(resolve, 20));
console.log(server.spans[0] ?? \"no span\");
await server.close();
";

const SPANS: &str = "module App.Main exposing (main)

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

/// Runs the app of `SPANS` built with `effect` under `--devtools` and checks
/// that the `DevTools` got its span.
fn spans_reach_the_devtools(name: &str, effect: &str) {
    let dir = project(
        name,
        &[
            (
                "flick.toml",
                &format!("{APP}\n[build]\neffect = \"{effect}\"\n"),
            ),
            ("src/App/Main.fl", SPANS),
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
    assert_eq!(out.status.code(), Some(0), "{name}: {out:?}");
    // the server logs to stdout too, as a client disconnects
    let got = lines.map_while(Result::ok).any(|line| line == "App.save");
    let _ = server.wait();
    assert!(got, "{name}: the DevTools got no span `App.save`");
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn run_with_devtools_sends_the_spans_there() {
    spans_reach_the_devtools("run_devtools", "effect");
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn run_on_flicks_own_runtime_sends_the_spans_to_the_devtools() {
    spans_reach_the_devtools("run_devtools_rt", "rt");
}

/// Without `DevTools` listening, an app on Flick's own runtime runs as without
/// the flag, by the flag or by `effect = "rt"`.
#[test]
#[ignore = "needs Node and the installed runtime"]
fn run_on_flicks_own_runtime_without_devtools_listening_runs_as_without_them() {
    let main = "module App.Main exposing (main)

main : Unit ->{Async; AppError} Unit
let main () =
  Effect.sleep (Result.with_default Duration.zero (Duration.millis 1))
  Platform.stdout_line \"on the kernel\"
";
    let config = format!("{APP}\n[build]\neffect = \"rt\"\n");
    for (name, toml, args) in [
        (
            "run_rt_flag",
            APP,
            &["run", "--rt", "--devtools=ws://127.0.0.1:9"][..],
        ),
        (
            "run_rt_config",
            config.as_str(),
            &["run", "--devtools=ws://127.0.0.1:9"][..],
        ),
    ] {
        let dir = project(name, &[("flick.toml", toml), ("src/App/Main.fl", main)]);
        let out = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("flick runs");
        assert_eq!(out.status.code(), Some(0), "{name}: {out:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            "on the kernel\n",
            "{name}"
        );
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains("DevTools"),
            "{name}: {out:?}"
        );
    }
}
