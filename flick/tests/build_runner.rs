mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::flick_in;
use flick_core::build::runner_file;

const ADAPTER: &str = r#"{"type":"module","dependencies":{"@flick-lang/effect-ts":"*"}}"#;

/// An app at `name` whose entry `App.Main` has `main`, started by `runner`.
fn app(name: &str, runner: &str, main: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    let config = format!(
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\nrunner = \"{runner}\"\n"
    );
    fs::write(dir.join("flick.toml"), config).expect("write");
    // These host runners belong to the optional Effect adapter.
    fs::write(dir.join("package.json"), ADAPTER).expect("manifest");
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

const WORKER: &str = "module App.Worker exposing (main)

import Runner.Cloudflare exposing (Fetch, Env)
import Http.ServerResponse exposing (ServerResponse)

main : Fetch ->{Env; AppError} ServerResponse
let main event =
  if Http.ServerRequest.url event.request == \"/boom\" then App.fail \"boom\"
  else
    let greeting = Option.with_default \"hello\" (Runner.Cloudflare.var \"GREETING\")
    Runner.Cloudflare.wait_until event.context (task { Platform.stdout_line \"after the response\" })
    Http.ServerResponse.text (greeting ++ \" from \" ++ Http.ServerRequest.url event.request) Http.ServerResponse.defaults
";

/// An app with a Worker, `web`, and a command line on the project's runner, `cli`.
fn workers_and_cli(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    fs::write(dir.join("package.json"), ADAPTER).expect("manifest");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nrunner = \"Runner.Node\"\nentries = { web = { module = \"App.Worker\", runner = \"Runner.Cloudflare\" }, cli = \"App.Cli\" }\n",
    )
    .expect("write");
    fs::write(dir.join("src/App/Worker.fl"), WORKER).expect("write");
    fs::write(
        dir.join("src/App/Cli.fl"),
        "module App.Cli exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"cli on node\"\n",
    )
    .expect("write");
    let out = flick_in(&dir, &["build"]);
    assert!(out.status.success(), "{out:?}");
    dir
}

#[test]
fn each_entry_is_started_by_its_own_runner() {
    let dir = workers_and_cli("build_runner_entries");
    let runner = |name: &str| {
        fs::read_to_string(dir.join("dist").join(runner_file(name, false))).expect("runner")
    };
    assert!(runner("web").contains("from \"./_flick/Runner/Cloudflare.js\""));
    assert!(runner("cli").contains("from \"./_flick/Runner/Node.js\""));
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn the_cloudflare_runner_answers_fetch_with_main() {
    let dir = workers_and_cli("build_runner_cloudflare");
    // a Worker's runtime imports the module and calls its `fetch` with the bindings and the context
    let script = format!(
        "const {{ default: worker }} = await import(\"./dist/{}\");\nconst pending = [];\nconst context = {{ waitUntil: (p) => pending.push(p) }};\nfor (const path of [\"/hi\", \"/boom\"]) {{\n  const response = await worker.fetch(new Request(`http://example.com${{path}}`), {{ GREETING: \"hey\" }}, context);\n  console.log(response.status, await response.text());\n}}\nawait Promise.all(pending);\n",
        runner_file("web", false)
    );
    let run = Command::new("node")
        .args(["--input-type=module", "--eval", &script])
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        stdout.contains("200 hey from /hi\n")
            && stdout.contains("500 Internal Server Error\n")
            && stdout.contains("after the response\n"),
        "{stdout}"
    );
    assert_eq!(String::from_utf8_lossy(&run.stderr), "error: boom\n");
    let cli = Command::new("node")
        .arg(Path::new("dist").join(runner_file("cli", false)))
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(
        String::from_utf8_lossy(&cli.stdout),
        "cli on node\n",
        "{cli:?}"
    );
}
