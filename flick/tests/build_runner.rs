mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::flick_in;
use flick_core::build::runner_file;

/// An app at `name` whose entry `App.Main` has `main`, started by `runner`.
fn app(name: &str, runner: &str, main: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    let config = format!(
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\nrunner = \"{runner}\"\n"
    );
    fs::write(dir.join("flick.toml"), config).expect("write");
    fs::write(dir.join("src/App/Main.fl"), main).expect("write");
    let out = flick_in(&dir, &["build"]);
    assert!(out.status.success(), "{out:?}");
    dir
}

const NODE_MAIN: &str = "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"started by Runner.Node\"\n";

#[test]
fn a_runner_module_starts_the_entry() {
    let dir = app("build_runner", "Runner.Node", NODE_MAIN);
    let runner =
        fs::read_to_string(dir.join("dist").join(runner_file("main", false))).expect("runner");
    assert!(
        runner.contains("import { run } from \"./_flick/Runner/Node.js\";")
            && runner.contains("export default run(main);"),
        "{runner}"
    );
    assert!(dir.join("dist/_flick/Runner/Node.js").exists());
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn the_node_runner_runs_main() {
    let dir = app("build_runner_runs", "Runner.Node", NODE_MAIN);
    let run = Command::new("node")
        .arg(Path::new("dist").join(runner_file("main", false)))
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "started by Runner.Node\n"
    );
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn the_cloudflare_runner_answers_fetch_with_main() {
    let main = "module App.Main exposing (main)

import Http.ServerRequest exposing (ServerRequest)
import Http.ServerResponse exposing (ServerResponse)

main : ServerRequest ->{; AppError} ServerResponse
let main request =
  if Http.ServerRequest.url request == \"/boom\" then App.fail \"boom\"
  else Http.ServerResponse.text (\"hello from \" ++ Http.ServerRequest.url request) Http.ServerResponse.defaults
";
    let dir = app("build_runner_cloudflare", "Runner.Cloudflare", main);
    // a Worker's runtime imports the module and calls its `fetch`
    let script = format!(
        "const {{ default: worker }} = await import(\"./dist/{}\");\nfor (const path of [\"/hi\", \"/boom\"]) {{\n  const response = await worker.fetch(new Request(`http://example.com${{path}}`));\n  console.log(response.status, await response.text());\n}}\n",
        runner_file("main", false)
    );
    let run = Command::new("node")
        .args(["--input-type=module", "--eval", &script])
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "200 hello from /hi\n500 Internal Server Error\n"
    );
    assert_eq!(String::from_utf8_lossy(&run.stderr), "error: boom\n");
}
