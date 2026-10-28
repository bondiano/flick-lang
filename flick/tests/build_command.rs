mod common;

use std::fs;
use std::path::{Path, PathBuf};
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

fn built_package_consumer(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
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
        lib.join("src/Text/Core.fl"),
        "module Text.Core exposing (suffix)\nsuffix : String\nlet suffix = \"!\"\n",
    )
    .expect("write");
    fs::write(
        lib.join("src/Text/Shout.fl"),
        "module Text.Shout exposing (shout)\n\nimport Text.Core exposing (suffix)\n\nshout : String -> String\nlet shout s = s ++ suffix\n",
    )
    .expect("write");
    fs::write(
        lib.join("src/Text/Api.fl"),
        "module Text.Api exposing (shout)\nimport Text.Shout exposing (shout)\n",
    )
    .expect("write");
    let out = flick_in(&lib, &["build"]);
    assert!(out.status.success(), "{out:?}");
    // the library as installed: its build output is its `iface-dir`
    let installed = dir.join("app/node_modules/@acme/text");
    fs::create_dir_all(installed.join("dist/Text")).expect("dir");
    for module in ["Api", "Core", "Shout"] {
        for ext in ["js", "iface"] {
            let file = format!("{module}.{ext}");
            fs::copy(
                lib.join("dist/Text").join(&file),
                installed.join("dist/Text").join(file),
            )
            .expect("copy");
        }
    }
    fs::write(
        installed.join("package.json"),
        "{ \"name\": \"@acme/text\", \"type\": \"module\", \"flick\": { \"iface-dir\": \"./dist/\" } }\n",
    )
    .expect("write");
    let app = dir.join("app");
    fs::create_dir_all(app.join("src/App")).expect("dir");
    fs::write(
        app.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(
        app.join("package.json"),
        "{ \"name\": \"@acme/app\", \"dependencies\": { \"@acme/text\": \"1.0.0\" } }\n",
    )
    .expect("write");
    fs::write(
        app.join("src/App/Main.fl"),
        "module App.Main exposing (greeting)\n\nimport Text.Api exposing (shout)\n\ngreeting : String\nlet greeting = shout \"hi\"\n",
    )
    .expect("write");
    let out = flick_in(&app, &["build"]);
    assert!(out.status.success(), "{out:?}");
    dir
}

#[test]
fn installed_package_graph_uses_published_fingerprints_and_reexport_origins() {
    let dir = built_package_consumer("build_package_graph");
    for (module, dependency) in [("Text.Api", "Text.Shout"), ("Text.Shout", "Text.Core")] {
        let published = |name: &str| {
            iface::read(
                &fs::read(
                    dir.join("lib/dist")
                        .join(format!("{}.iface", name.replace('.', "/"))),
                )
                .expect("interface"),
                name,
            )
            .expect("valid interface")
        };
        assert_eq!(
            published(module).dependencies[dependency],
            iface::fingerprint(&published(dependency)),
        );
    }
    let app = dir.join("app");
    // Older interfaces may record only the export origin, not its dependency hash.
    let api_path = app.join("node_modules/@acme/text/dist/Text/Api.iface");
    let mut api = iface::read(&fs::read(&api_path).expect("interface"), "Text.Api").expect("valid");
    api.dependencies.clear();
    fs::write(&api_path, iface::write(&api)).expect("write");
    let origins = flick_in(&app, &["build", "--out-dir", "origins"]);
    assert!(origins.status.success(), "{origins:?}");
    let core_path = app.join("node_modules/@acme/text/dist/Text/Core.iface");
    let mut core =
        iface::read(&fs::read(&core_path).expect("interface"), "Text.Core").expect("valid");
    let summary = &mut core.abi.as_mut().expect("ABI").values[0];
    summary.partial = !summary.partial;
    fs::write(&core_path, iface::write(&core)).expect("write");
    let mismatch = flick_in(&app, &["build", "--out-dir", "incompatible"]);
    assert_eq!(mismatch.status.code(), Some(1), "{mismatch:?}");
    let stderr = String::from_utf8_lossy(&mismatch.stderr);
    assert!(
        stderr.contains("FLICK-P014") && stderr.contains("Text.Core"),
        "{stderr}"
    );
    fs::remove_file(core_path).expect("remove interface");
    let missing = flick_in(&app, &["build", "--out-dir", "missing"]);
    assert_eq!(missing.status.code(), Some(1), "{missing:?}");
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(
        stderr.contains("FLICK-P014") && stderr.contains("Text.Core"),
        "{stderr}"
    );
    api.dependencies
        .insert("../Escaped".into(), "invalid".into());
    fs::write(api_path, iface::write(&api)).expect("write");
    let invalid = flick_in(&app, &["build", "--out-dir", "invalid"]);
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    let stderr = String::from_utf8_lossy(&invalid.stderr);
    assert!(
        stderr.contains("FLICK-P010") && stderr.contains("../Escaped"),
        "{stderr}"
    );
}

#[test]
#[ignore = "needs Node"]
fn an_external_node_consumer_runs_an_installed_library_reexport() {
    let dir = built_package_consumer("build_package_node");
    let app = dir.join("app");
    let api_path = app.join("node_modules/@acme/text/dist/Text/Api.iface");
    let mut api = iface::read(&fs::read(&api_path).expect("interface"), "Text.Api").expect("valid");
    api.dependencies.clear();
    fs::write(api_path, iface::write(&api)).expect("write");
    let origins = flick_in(&app, &["build", "--out-dir", "origins"]);
    assert!(origins.status.success(), "{origins:?}");
    for output in ["dist", "origins"] {
        let node = Command::new("node")
            .args(["--input-type=module", "--eval"])
            .arg(format!(
                "const {{ greeting }} = await import('./{output}/App/Main.js'); console.log(greeting);"
            ))
            .current_dir(&app)
            .output()
            .expect("node");
        assert!(node.status.success(), "{node:?}");
        assert_eq!(String::from_utf8_lossy(&node.stdout), "hi!\n");
        assert!(node.stderr.is_empty(), "{node:?}");
    }
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
            format!("module {module} exposing (value)\nvalue : Number\nlet value = {value}\n"),
        )
        .expect("write");
    }
    fs::write(
        dir.join("src/Main.fl"),
        "module Main exposing (answer)\nimport Foo.Bar\nimport Foo_Bar\nanswer : Number\nlet answer = Foo.Bar.value + Foo_Bar.value\n",
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
