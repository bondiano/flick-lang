//! A source package (`"flick": { "source-dir": … }`) is compiled by each
//! dependent for the target it emits, as modules of that package: a Go
//! server and a JS client share its types and its `HttpApi`, and a sum type
//! crosses the wire with the same constructor tags. Ignored by default:
//! part of `just e2e`, needs Go.

mod common;

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::{env, fs};

use common::{APP, flick_in, project};
use flick_core::build::go::executable;

const CONTRACT_PACKAGE: &str =
    "{ \"name\": \"@acme/contract\", \"flick\": { \"source-dir\": \"src\" } }\n";

const TYPES: &str = "module Shared.Types exposing (Shape(..), Box)

type Shape = Circle Float | Square { side : Float } | Dot
  deriving Eq, Show, Schema

type alias Box = { label : String; shapes : List Shape }
  deriving Schema
";

const API: &str = "module Shared.Api exposing (Handlers, api, get_box)

import Shared.Types exposing (Box)

get_box : HttpApi.Endpoint \"get_box\" (HttpApi.Request Unit Unit Unit Unit ->{r;} Box)
let get_box =
  HttpApi.endpoint HttpApi.Get \"/box\" (Record.label : Record.Label \"get_box\")
  |> HttpApi.with_success 200 (Codec.json (schema Box))

type alias Handlers r = { get_box : HttpApi.Request Unit Unit Unit Unit ->{r;} Box }

api : HttpApi.Api r { boxes : Handlers r }
let api =
  HttpApi.api \"Boxes\"
  |> HttpApi.add_group (HttpApi.group (Record.label : Record.Label \"boxes\") |> HttpApi.add get_box)
";

const SERVER: &str = "module App.Main exposing (main)

import Shared.Api exposing (Handlers, api)
import Shared.Types exposing (Shape(..))

main : Unit ->{; AppError} Unit
let main () =
  let handlers : { boxes : Handlers none } = {
    boxes = {
      get_box = fun _ -> { label = \"mixed\"; shapes = [Circle 1.5, Square { side = 2.0 }, Dot] };
    };
  }
  let options : HttpApi.ServeOptions = {
    host = \"127.0.0.1\";
    port = 0;
    openapi = None;
    swagger = None;
    scalar = None;
  }
  try
    scope s {
      let server = HttpApi.listen options api handlers
      Platform.stdout_line (show (HttpApi.port server))
      Effect.never ()
    }
  with
  | HttpApi.ServeError message -> Platform.stdout_line (\"serve error \" ++ message)
";

const CLIENT: &str = "module App.Main exposing (main)

import Shared.Api exposing (get_box)
import Shared.Types exposing (Shape(..))

main : Unit ->{; AppError} Unit
let main () =
  let base = Option.with_default \"\" (Platform.env \"BASE\")
  let request = { path = (); query = (); headers = (); payload = () }
  let line =
    try
      let box = HttpApi.call base get_box request
      show box ++ \" \" ++ show (box.shapes == [Circle 1.5, Square { side = 2.0 }, Dot])
    with
    | HttpApi.ClientError e -> \"client error \" ++ e.message
  Platform.stdout_line line
";

/// A workspace of the contract package and a dependent `app` that emits
/// `emit`, the package linked into its `node_modules` as pnpm links one.
fn workspace(name: &str, app_main: &str, emit: &str) -> PathBuf {
    let config = format!("{APP}\n[build]\nemit = \"{emit}\"\n");
    let dir = project(
        name,
        &[
            ("contract/package.json", CONTRACT_PACKAGE),
            (
                "contract/flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
            ),
            ("contract/src/Shared/Types.fl", TYPES),
            ("contract/src/Shared/Api.fl", API),
            (
                "app/package.json",
                "{ \"name\": \"@acme/app\", \"dependencies\": { \"@acme/contract\": \"workspace:*\" } }\n",
            ),
            ("app/flick.toml", &config),
            ("app/src/App/Main.fl", app_main),
        ],
    );
    link(
        &dir.join("contract"),
        &dir.join("app/node_modules/@acme/contract"),
    );
    dir
}

fn link(target: &Path, at: &Path) {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_dir as symlink;
    fs::create_dir_all(at.parent().expect("a parent")).expect("dir");
    symlink(target, at).expect("symlink");
}

#[test]
#[ignore = "needs Go"]
fn a_go_server_and_a_js_client_share_a_source_package() {
    let server = workspace("source_package_server", SERVER, "go").join("app");
    let build = flick_in(&server, &["build"]);
    assert!(build.status.success(), "{build:?}");
    let mut child = Command::new(server.join("dist").join(executable("main")))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server runs");
    let (tx, rx) = mpsc::channel();
    let stdout = child.stdout.take().expect("piped stdout");
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let port = rx.recv_timeout(Duration::from_secs(60)).expect("the port");
    let client = workspace("source_package_client", CLIENT, "js").join("app");
    let run = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("run")
        .current_dir(&client)
        .env("BASE", format!("http://127.0.0.1:{port}"))
        .env_remove("NO_COLOR")
        .output()
        .expect("flick runs");
    child.kill().expect("kill the server");
    let _ = child.wait();
    assert!(run.status.success(), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).trim(),
        "{ label = \"mixed\", shapes = [Circle 1.5, Square { side = 2 }, Dot] } true"
    );
}

#[test]
fn a_source_package_module_is_checked_for_the_target_of_its_dependent() {
    let ui = "module Shared.View exposing (hello)\n\nimport Ui\n\nhello : Ui\nlet hello = Ui.text \"hi\"\n";
    let main = "module App.Main exposing (main)\n\nimport Shared.View exposing (hello)\n\nmain : Unit -> Unit\nlet main () = ()\n";
    let dir = workspace("source_package_go_only", main, "go");
    fs::write(dir.join("contract/src/Shared/View.fl"), ui).expect("write");
    fs::write(
        dir.join("app/src/App/Main.fl"),
        main.replace("let main () = ()", "let main () =\n  let _ = hello\n  ()"),
    )
    .expect("write");
    let out = flick_in(dir.join("app"), &["check"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FLICK-P030") && stderr.contains("`Ui`"),
        "{stderr}"
    );
}

#[test]
fn a_dependent_does_not_answer_for_the_warnings_of_a_source_package() {
    let unused = "module Shared.Types exposing (Box)\n\nimport Shared.Api exposing (api)\n\ntype alias Box = { label : String }\n";
    let main = "module App.Main exposing (main)\n\nimport Shared.Types exposing (Box)\n\nmain : Unit -> Unit\nlet main () =\n  let _box : Box = { label = \"a\" }\n  ()\n";
    let dir = workspace("source_package_warnings", main, "js");
    fs::write(dir.join("contract/src/Shared/Types.fl"), unused).expect("write");
    fs::write(
        dir.join("contract/src/Shared/Api.fl"),
        "module Shared.Api exposing (api)\n\napi : Int\nlet api = 1\n",
    )
    .expect("write");
    let out = flick_in(dir.join("app"), &["check", "--warnings-as-errors"]);
    assert!(out.status.success(), "{out:?}");
}
