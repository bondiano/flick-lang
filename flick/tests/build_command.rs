mod common;

use std::fs;
use std::path::Path;

use common::flick_in;

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
