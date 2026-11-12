//! The RPC client of Flick's own runtime speaks the protocol of Effect's RPC
//! server: `rpc_rt/client.fl`, built with `--rt`, calls `rpc_rt/server.fl`,
//! built on the Effect adapter, in NDJSON and in JSON. Running them needs
//! Node and the workspace's runtime: the test is ignored by default and
//! part of `just e2e-tests`.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn build(source: &str, out: &Path, rt: bool) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rpc_rt")
        .join(source);
    let mut command = Command::new(env!("CARGO_BIN_EXE_flick"));
    command.arg("build").arg(source).arg("--out-dir").arg(out);
    if rt {
        command.arg("--rt");
    }
    let built = command.output().expect("flick runs");
    assert!(built.status.success(), "{built:?}");
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn a_client_on_flicks_runtime_calls_an_effect_server() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("rpc_rt");
    let (server, client) = (dir.join("server"), dir.join("client"));
    build("server.fl", &server, false);
    build("client.fl", &client, true);
    let rpc = fs::read_to_string(client.join("_flick/Rpc.js")).expect("client");
    assert!(rpc.contains("@flick-lang/runtime/rt/effect/rpc/Rpc"), "{rpc}");

    let mut running = Command::new("node")
        .arg(server.join("flick-main.js"))
        .stdout(Stdio::piped())
        .spawn()
        .expect("node");
    let mut line = String::new();
    BufReader::new(running.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("the server's ports");
    let ports: Vec<&str> = line.trim().split(' ').skip(1).collect();
    let [lines, json] = ports[..] else {
        panic!("no ports in {line:?}")
    };
    let out = Command::new("node")
        .arg(client.join("flick-main.js"))
        .env("RPC_NDJSON", format!("http://127.0.0.1:{lines}/rpc"))
        .env("RPC_JSON", format!("http://127.0.0.1:{json}/rpc"))
        .output()
        .expect("node");
    let _ = running.kill();
    let _ = running.wait();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let round = "got Ada\nnot found 9\nping answered\n";
    assert_eq!(stdout, format!("{round}{round}no server\n"));
    // the server's defect ends the client as a defect of its own
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("boom"),
        "{out:?}"
    );
}
