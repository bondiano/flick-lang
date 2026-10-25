mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::flick_in;
use flick_core::{build::runner_path, iface};

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
        "module Main exposing (answer, main)\n\nanswer : Number\nlet answer = 42\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"ran main\"\n",
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
        "module Lib exposing (one)\none : Number\nlet one = 1\n",
    )
    .expect("write");
    // a file where the output directory should be
    fs::write(dir.join("out"), "").expect("write");
    let out = flick_in(&dir, &["build", "Lib.fl", "--out-dir", "out"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-P013"));
}

#[test]
fn a_package_module_is_imported_next_to_its_interface() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_package");
    let _ = fs::remove_dir_all(&dir);
    let lib = dir.join("lib");
    fs::create_dir_all(lib.join("src/Text")).expect("dir");
    fs::write(
        lib.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(lib.join("package.json"), "{ \"name\": \"@acme/text\" }\n").expect("write");
    fs::write(
        lib.join("src/Text/Shout.fl"),
        "module Text.Shout exposing (shout)\n\nshout : String -> String\nlet shout s = s ++ \"!\"\n",
    )
    .expect("write");
    let out = flick_in(&lib, &["build"]);
    assert!(out.status.success(), "{out:?}");
    // the library as installed: its build output is its `iface-dir`
    let installed = dir.join("app/node_modules/@acme/text");
    fs::create_dir_all(installed.join("dist/Text")).expect("dir");
    for file in ["Shout.js", "Shout.iface"] {
        fs::copy(
            lib.join("dist/Text").join(file),
            installed.join("dist/Text").join(file),
        )
        .expect("copy");
    }
    fs::write(
        installed.join("package.json"),
        "{ \"name\": \"@acme/text\", \"flick\": { \"iface-dir\": \"./dist/\" } }\n",
    )
    .expect("write");
    let app = dir.join("app");
    fs::create_dir_all(app.join("src/App")).expect("dir");
    fs::write(
        app.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
    )
    .expect("write");
    fs::write(
        app.join("package.json"),
        "{ \"name\": \"@acme/app\", \"dependencies\": { \"@acme/text\": \"1.0.0\" } }\n",
    )
    .expect("write");
    fs::write(
        app.join("src/App/Main.fl"),
        "module App.Main exposing (main)\n\nimport Text.Shout exposing (shout)\n\nmain : Unit ->{; AppError} Unit\nlet main () =\n  Platform.stdout_line (shout \"hi\")\n",
    )
    .expect("write");
    let out = flick_in(&app, &["build"]);
    assert!(out.status.success(), "{out:?}");
    let main = fs::read_to_string(app.join("dist/App/Main.js")).expect("module");
    assert!(
        main.contains("from \"@acme/text/dist/Text/Shout.js\""),
        "{main}"
    );
    assert!(installed.join("dist/Text/Shout.js").exists());
}

const AOT_APP: &str = r#"module Main exposing (main)

type alias Point = { x : Number; y : Number }
  deriving Schema

point_json : PureCodec Json Point
let point_json = Codec.json (schema Point)

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
    assert!(aot.contains("from \"@flick-lang/runtime\""), "{aot}");
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
        "module Lib exposing (one)\nimport List\none : Number\nlet one = List.nope 1\n",
    )
    .expect("write");
    let out = flick_in(&dir, &["build", "Lib.fl"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-R004"));
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
