//! An app of several entries: each runs by name and packs as a `bin`.

mod common;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use common::project;

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("flick runs")
}

#[test]
#[ignore = "needs Node, pnpm and the installed runtime"]
fn entries_run_by_name_and_pack_as_bins() {
    let config = "schema-version = 1\n\n[project]\nkind = \"app\"\nentries = { server = \"App.Server\", cli = \"App.Cli\" }\n";
    let server = "module App.Server exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"serving\"\n";
    let cli = "module App.Cli exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"cli\"\n";
    let manifest = "{ \"name\": \"@acme/tools\", \"version\": \"1.0.0\" }\n";
    let dir = project(
        "run_entries",
        &[
            ("flick.toml", config),
            ("src/App/Server.fl", server),
            ("src/App/Cli.fl", cli),
            ("package.json", manifest),
        ],
    );
    let out = run(&dir, &["run", "server"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "serving\n");
    let out = run(&dir, &["run", "cli"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "cli\n", "{out:?}");
    // several entries and none named, or a name the project lacks
    for args in [&["run"][..], &["run", "worker"][..]] {
        let out = run(&dir, args);
        assert_eq!(out.status.code(), Some(2), "{out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("`cli`, `server`"), "{stderr}");
    }

    let out = run(&dir, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let staged: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".flick/pack/package.json")).expect("manifest"),
    )
    .expect("json");
    assert_eq!(staged["bin"]["server"], "./dist/flick-server.js");
    assert_eq!(staged["bin"]["cli"], "./dist/flick-cli.js");
    let bin = Command::new("node")
        .arg(dir.join(".flick/pack/dist/flick-server.js"))
        .output()
        .expect("node");
    assert_eq!(String::from_utf8_lossy(&bin.stdout), "serving\n", "{bin:?}");
}
