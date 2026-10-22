mod common;

use std::fs;
use std::path::Path;

use common::flick_in;
use flick_core::iface;

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
    assert!(
        main.contains("\"@acme/hello/App.Main.Greeting.Hello\""),
        "{main}"
    );
    assert!(main.ends_with("//# sourceMappingURL=Main.js.map\n"));
    let map: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("dist/App/Main.js.map")).expect("map"))
            .expect("json");
    assert_eq!(map["version"], 3);
    assert!(
        map["sources"][0]
            .as_str()
            .is_some_and(|s| s.ends_with("src/App/Main.fl"))
    );
    assert!(dir.join("dist/_flick/Platform.js").exists());
    let iface = fs::read(dir.join("dist/App/Main.iface")).expect("interface");
    let interface = iface::read(&iface, "App.Main").expect("valid interface");
    let abi = interface.abi.expect("a built interface has its ABI");
    assert_eq!(
        abi.data[0].constructors[0].tag,
        "@acme/hello/App.Main.Greeting.Hello"
    );
    assert_eq!(abi.values[0].stages, "!");
    let runner = fs::read_to_string(dir.join("dist/main.js")).expect("runner");
    assert!(
        runner.contains("import { main } from \"./App/Main.js\""),
        "{runner}"
    );
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

#[test]
fn schema_aot_turns_on_compiled_codecs_in_the_runner() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_schema_aot");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(
        dir.join("Main.fl"),
        "module Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"hi\"\n",
    )
    .expect("write");
    let out = flick_in(
        &dir,
        &["build", "Main.fl", "--out-dir", "out", "--schema-aot"],
    );
    assert!(out.status.success(), "{out:?}");
    let runner = fs::read_to_string(dir.join("out/main.js")).expect("runner");
    assert!(
        runner.contains("enableSchemaAot();\nawait runMain(main);"),
        "{runner}"
    );
    let plain = flick_in(&dir, &["build", "Main.fl", "--out-dir", "plain"]);
    assert!(plain.status.success(), "{plain:?}");
    let runner = fs::read_to_string(dir.join("plain/main.js")).expect("runner");
    assert!(!runner.contains("enableSchemaAot"), "{runner}");
}
