#[path = "build_command/cache.rs"]
mod cache;
#[path = "build_command/capabilities.rs"]
mod capabilities;
mod common;
#[path = "build_command/ir.rs"]
mod ir;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::{APP, flick_in, project};
use flick_core::{build::runner_path, iface};

const MAIN: &str = "module App.Main exposing (main)\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"hi\"\n";
const LIB: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n";
const FAILURE: &str = "module B exposing (answer)\nanswer : Float\nlet answer = \"wrong\"\n";

#[test]
fn silent_build_preserves_artifacts_and_diagnostics() {
    let files = [("flick.toml", APP), ("src/App/Main.fl", MAIN)];
    let dir = project("build_silent", &files);
    let ordinary = flick_in(&dir, &["build"]);
    assert_eq!(ordinary.status.code(), Some(0), "{ordinary:?}");
    let module = fs::read(dir.join("dist/App/Main.js")).expect("ordinary module");
    let interface = fs::read(dir.join("dist/App/Main.iface")).expect("ordinary interface");

    for args in [
        ["--silent", "build"],
        ["build", "--silent"],
        ["--quiet", "build"],
    ] {
        let dir = project("build_silent", &files);
        for _ in 0..2 {
            let out = flick_in(&dir, &args);
            assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
            assert!(out.stdout.is_empty(), "{args:?}: {out:?}");
            assert!(out.stderr.is_empty(), "{args:?}: {out:?}");
            assert_eq!(
                fs::read(dir.join("dist/App/Main.js")).expect("module"),
                module,
                "{args:?}"
            );
            assert_eq!(
                fs::read(dir.join("dist/App/Main.iface")).expect("interface"),
                interface,
                "{args:?}"
            );
        }
    }

    let warning = project(
        "build_silent_warning",
        &[
            ("flick.toml", LIB),
            (
                "src/App/Warning.fl",
                "module App.Warning exposing (..)\ntype Kind = A | B\nlet a : Kind = A\n",
            ),
        ],
    );
    for (args, code) in [
        (&["build", "--silent"][..], 0),
        (&["build", "--silent", "--warnings-as-errors"][..], 1),
    ] {
        let out = flick_in(&warning, args);
        assert_eq!(out.status.code(), Some(code), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
        let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
        assert!(stderr.contains("FLICK-T021"), "{stderr}");
        assert!(!stderr.contains("  flick  "), "{stderr}");
    }

    let failure = project(
        "build_silent_failure",
        &[("flick.toml", LIB), ("src/B.fl", FAILURE)],
    );
    let out = flick_in(&failure, &["build", "--silent"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("FLICK-T003"),
        "{out:?}"
    );
}

#[test]
fn build_progress_preserves_stream_contracts() {
    let dir = project(
        "build_progress",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    for expected in [
        &[
            "Preparing compiler",
            "Resolving project",
            "Checking build cache",
            "Compiling Flick modules",
            "Emitting JavaScript",
            "Writing artifacts",
            "Recording build outputs",
            "Saving build cache",
        ][..],
        &[
            "Preparing compiler",
            "Resolving project",
            "Checking build cache",
            "Restoring cached artifacts",
        ][..],
    ] {
        let out = flick_in(&dir, &["build"]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
        let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
        assert!(!stderr.contains(['\r', '\x1b']), "{stderr}");
        let phases: Vec<_> = stderr
            .lines()
            .filter_map(|line| line.strip_prefix("  flick  "))
            .map(|line| line.rsplit_once("  ").expect("phase and elapsed time").0)
            .collect();
        assert_eq!(phases, expected, "{stderr}");
    }

    let out = flick_in(&dir, &["build", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    assert!(out.stderr.is_empty(), "{out:?}");

    let failure = project(
        "build_progress_failure",
        &[("flick.toml", LIB), ("src/B.fl", FAILURE)],
    );
    let out = flick_in(&failure, &["build"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
    assert!(!stderr.contains(['\r', '\x1b']), "{stderr}");
    assert!(stderr.contains("Compiling Flick modules"), "{stderr}");
    assert!(stderr.contains("FLICK-T003"), "{stderr}");
    assert!(!stderr.contains("Emitting"), "{stderr}");
    assert!(!stderr.contains("Writing artifacts"), "{stderr}");

    let out = flick_in(&failure, &["build", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
    let codes: Vec<_> = stderr
        .lines()
        .map(|line| {
            let diagnostic: serde_json::Value =
                serde_json::from_str(line).expect("NDJSON diagnostic");
            assert_eq!(diagnostic["schema-version"], 1);
            assert_eq!(diagnostic["type"], "diagnostic");
            diagnostic["code"].as_str().expect("code").to_owned()
        })
        .collect();
    assert_eq!(codes, ["FLICK-T003"]);
}

#[test]
fn build_writes_modules_maps_builtins_and_the_runner() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_command");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{ \"name\": \"@acme/hello\" }\n").expect("write");
    fs::write(
        dir.join("src/App/Main.fl"),
        "module App.Main exposing (main)\n\ntype Greeting = Hello\n\nmain : Unit ->{none; AppError} Unit\nlet main () =\n  Platform.stdout_line \"hi\"\n",
    )
    .expect("write");
    let out = flick_in(&dir, &["build"]);
    assert!(out.status.success(), "{out:?}");
    let main = fs::read_to_string(dir.join("dist/App/Main.js")).expect("module");
    assert!(main.contains("_tag: \"Hello#"), "{main}");
    assert!(main.ends_with("//# sourceMappingURL=Main.js.map\n"));
    let map: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("dist/App/Main.js.map")).expect("map"))
            .expect("json");
    assert_eq!(map["version"], 3);
    // relative to the map: no path of the build machine ships
    assert_eq!(map["sources"][0], "../../src/App/Main.fl");
    assert!(dir.join("dist/_flick/Platform.js").exists());
    // only the built-in modules the program reaches
    let builtins = fs::read_dir(dir.join("dist/_flick"))
        .expect("builtins")
        .count();
    assert!(builtins <= 5, "{builtins} built-in modules");
    let iface = fs::read(dir.join("dist/App/Main.iface")).expect("interface");
    let interface = iface::read(&iface, "App.Main").expect("valid interface");
    let abi = interface.abi.expect("a built interface has its ABI");
    let hello = &abi.data[0].constructors[0];
    assert_eq!(hello.id, "@acme/hello/App.Main.Greeting.Hello");
    assert!(
        hello
            .tag
            .as_deref()
            .is_some_and(|t| t.starts_with("Hello#")),
        "{hello:?}"
    );
    assert_eq!(abi.values[0].stages, "!");
    assert!(dir.join("dist").join(runner_path(false)).exists());
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn standalone_main_preserves_exports_and_runs_in_all_emit_profiles() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_standalone_main");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(
        dir.join("Main.fl"),
        "module Main exposing (answer, main)\n\nanswer : Float\nlet answer = 42\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"ran main\"\n",
    )
    .expect("write");
    for (emit, profile) in [("js", "debug"), ("ts", "debug"), ("js", "release")] {
        let output = format!("{emit}-{profile}");
        let build = flick_in(
            &dir,
            &[
                "build",
                "Main.fl",
                "--out-dir",
                &output,
                "--emit",
                emit,
                "--profile",
                profile,
            ],
        );
        assert!(build.status.success(), "{emit}/{profile}: {build:?}");
        let out = dir.join(output);
        let imported = Command::new("node")
            .args(["--input-type=module", "--eval"])
            .arg(format!(
                "const {{ answer }} = await import(\"./Main.{emit}\"); console.log(answer);"
            ))
            .current_dir(&out)
            .output()
            .expect("node");
        assert_eq!(
            imported.status.code(),
            Some(0),
            "{emit}/{profile}: {imported:?}"
        );
        assert_eq!(String::from_utf8_lossy(&imported.stdout), "42\n");
        assert!(imported.stderr.is_empty(), "{emit}/{profile}: {imported:?}");
        let run = Command::new("node")
            .arg(runner_path(emit == "ts"))
            .current_dir(&out)
            .output()
            .expect("node");
        assert_eq!(run.status.code(), Some(0), "{emit}/{profile}: {run:?}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), "ran main\n");
        assert!(run.stderr.is_empty(), "{emit}/{profile}: {run:?}");
    }
}

#[test]
fn build_reports_what_it_cannot_write() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_blocked");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(
        dir.join("Lib.fl"),
        "module Lib exposing (one)\none : Float\nlet one = 1\n",
    )
    .expect("write");
    // a file where the output directory should be
    fs::write(dir.join("out"), "").expect("write");
    let out = flick_in(&dir, &["build", "Lib.fl", "--out-dir", "out"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-P013"));
}

#[test]
#[ignore = "needs Node"]
fn dotted_and_underscored_modules_run_in_all_emit_profiles() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_module_aliases");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/Foo")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    for (module, value) in [("Foo.Bar", 1), ("Foo_Bar", 2)] {
        fs::write(
            dir.join("src")
                .join(format!("{}.fl", module.replace('.', "/"))),
            format!("module {module} exposing (value)\nvalue : Float\nlet value = {value}\n"),
        )
        .expect("write");
    }
    fs::write(
        dir.join("src/Main.fl"),
        "module Main exposing (answer)\nimport Foo.Bar\nimport Foo_Bar\nanswer : Float\nlet answer = Foo.Bar.value + Foo_Bar.value\n",
    )
    .expect("write");
    for (emit, profile) in [("js", "debug"), ("ts", "debug"), ("js", "release")] {
        let output = format!("{emit}-{profile}");
        let build = flick_in(
            &dir,
            &[
                "build",
                "--emit",
                emit,
                "--profile",
                profile,
                "--out-dir",
                &output,
            ],
        );
        assert!(build.status.success(), "{emit}/{profile}: {build:?}");
        let node = Command::new("node")
            .args(["--input-type=module", "--eval"])
            .arg(format!(
                "const {{ answer }} = await import('./Main.{emit}'); console.log(answer);"
            ))
            .current_dir(dir.join(output))
            .output()
            .expect("node");
        assert!(node.status.success(), "{emit}/{profile}: {node:?}");
        assert_eq!(String::from_utf8_lossy(&node.stdout), "3\n");
        assert!(node.stderr.is_empty(), "{emit}/{profile}: {node:?}");
    }
}

const AOT_APP: &str = r#"module Main exposing (main)

type alias Point = { x : Float; y : Float }
  deriving Schema

point_json : PureCodec Json Point
let point_json = Codec.json Point.schema

main : Unit ->{; AppError} Unit
let main () =
  match Json.parse "{\"x\": 1, \"y\": 2}" with
  | Err _ -> Platform.stdout_line "not JSON"
  | Ok input ->
    match Codec.decode_result point_json input with
    | Ok p -> Platform.stdout_line (show (p.x + p.y))
    | Err _ -> Platform.stdout_line "not a point"
"#;

#[test]
#[ignore = "needs Node and pnpm install"]
fn schema_aot_writes_compiled_decoders_the_runner_installs() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_schema_aot");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("Main.fl"), AOT_APP).expect("write");
    let out = flick_in(
        &dir,
        &["build", "Main.fl", "--out-dir", "out", "--schema-aot"],
    );
    assert!(out.status.success(), "{out:?}");
    let aot = fs::read_to_string(dir.join("out/schemas.aot.js")).expect("compiled decoders");
    assert!(aot.contains("from \"@flick-lang/effect-ts\""), "{aot}");
    let run = Command::new("node")
        .arg(Path::new("out").join(runner_path(false)))
        .current_dir(&dir)
        .output()
        .expect("node");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n");
    // a stale module is refused, and decoding still works
    fs::write(
        dir.join("out/schemas.aot.js"),
        "export const fingerprint = \"\";\nexport const install = () => {};\n",
    )
    .expect("write");
    let stale = Command::new("node")
        .arg(Path::new("out").join(runner_path(false)))
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(String::from_utf8_lossy(&stale.stdout), "3\n");
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("stale"),
        "{stale:?}"
    );
    let plain = flick_in(&dir, &["build", "Main.fl", "--out-dir", "plain"]);
    assert!(plain.status.success(), "{plain:?}");
    assert!(!dir.join("plain/schemas.aot.js").exists());
}

#[test]
fn build_stops_at_an_unresolved_name() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_unresolved");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(
        dir.join("Lib.fl"),
        "module Lib exposing (one)\nimport List\none : Float\nlet one = List.nope 1\n",
    )
    .expect("write");
    let out = flick_in(&dir, &["build", "Lib.fl"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-R004"));
}

#[test]
fn build_stops_at_an_unknown_module() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_unknown_module");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(
        dir.join("Main.fl"),
        "module Main exposing (main)\nimport App.Missing exposing (thing)\nmain : Unit ->{} Unit\nlet main () = thing ()\n",
    )
    .expect("write");
    let out = flick_in(&dir, &["build", "Main.fl"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-R003"));
}

#[test]
fn several_entries_build_a_runner_each() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_entries");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentries = { server = \"App.Server\", cli = \"App\" }\n",
    )
    .expect("write");
    fs::write(
        dir.join("src/App/Server.fl"),
        "module App.Server exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"serving\"\n",
    )
    .expect("write");
    fs::write(
        dir.join("src/App/Cli.fl"),
        "module App.Cli exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("write");
    // a facade: its `main` is the one of App.Cli
    fs::write(
        dir.join("src/App.fl"),
        "module App exposing (main)\n\nimport App.Cli exposing (main)\n",
    )
    .expect("write");
    let out = flick_in(&dir, &["build"]);
    assert!(out.status.success(), "{out:?}");
    let server = fs::read_to_string(dir.join("dist/flick-server.js")).expect("server runner");
    assert!(server.starts_with("#!/usr/bin/env node\n"), "{server}");
    assert!(server.contains("from \"./App/Server.js\""), "{server}");
    let cli = fs::read_to_string(dir.join("dist/flick-cli.js")).expect("cli runner");
    // the facade's module has no `main` of its own to export
    assert!(cli.contains("from \"./App/Cli.js\""), "{cli}");
    assert!(cli.contains("main(undefined);"), "{cli}");
    assert!(!dir.join("dist").join(runner_path(false)).exists());
    let manifest = fs::read_to_string(dir.join("dist/package.json")).expect("manifest");
    assert!(
        manifest.contains("\"sideEffects\": [\"./flick-cli.js\", \"./flick-server.js\"]"),
        "{manifest}"
    );
}

#[test]
fn an_output_directory_holding_the_project_is_refused_before_anything_is_written() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_out_dir_overlap");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
    )
    .expect("write");
    let manifest = "{ \"name\": \"@acme/kept\", \"private\": true }\n";
    fs::write(dir.join("package.json"), manifest).expect("write");
    fs::write(
        dir.join("src/App/Main.fl"),
        "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"hi\"\n",
    )
    .expect("write");
    for out_dir in [".", "..", "src"] {
        let out = flick_in(&dir, &["build", "--out-dir", out_dir]);
        assert!(!out.status.success(), "{out_dir}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("FLICK-P028"),
            "{out_dir}: {out:?}"
        );
        assert_eq!(
            fs::read_to_string(dir.join("package.json")).expect("manifest"),
            manifest
        );
        assert!(!dir.join("App").exists() && !dir.join("src/App/Main.js").exists());
    }
    // a directory of its own builds and cleans as before
    assert!(
        flick_in(&dir, &["build", "--out-dir", "out"])
            .status
            .success()
    );
    assert!(dir.join("out/App/Main.js").exists());
    assert!(flick_in(&dir, &["clean"]).status.success());
    assert_eq!(
        fs::read_to_string(dir.join("package.json")).expect("manifest"),
        manifest
    );
}
