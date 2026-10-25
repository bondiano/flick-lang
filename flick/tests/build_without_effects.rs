mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::flick_in;
use flick_core::build::runner_file;

const CONFIG: &str = "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\neffect = \"none\"\n";

const MAIN: &str = "module App.Main exposing (main)

import Option

say : String -> Unit
foreign pure say from global \"console.log\"

type Shape = Circle Number | Rect Number Number
  deriving Eq, Show

area : Shape -> Number
let area s =
  match s with
  | Circle r -> 3 * r * r
  | Rect w h -> w * h

main : Unit -> Unit
let main () =
  let areas = List.map area [Circle 1, Rect 2 3]
  let _ = say (show areas)
  let _ = say (show (Circle 2 == Circle 2))
  say (show (Option.with_default 0 (List.get 5 areas)))
";

/// A project built with `effect = "none"` in `name`, with `main` as its entry module.
fn project(name: &str, main: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    fs::write(dir.join("flick.toml"), CONFIG).expect("write");
    fs::write(dir.join("src/App/Main.fl"), main).expect("write");
    dir
}

/// Every module specifier the files under `dir` import.
fn specifiers(dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            specifiers(&path, out);
        } else if path.extension().is_some_and(|e| e == "js") {
            let text = fs::read_to_string(&path).expect("read");
            out.extend(
                text.split(" from \"")
                    .skip(1)
                    .filter_map(|rest| rest.split_once('"').map(|(spec, _)| spec.to_owned())),
            );
        }
    }
}

#[test]
fn a_build_without_effects_imports_only_the_core_runtime() {
    let dir = project("build_without_effects", MAIN);
    let out = flick_in(&dir, &["build"]);
    assert!(out.status.success(), "{out:?}");
    let mut imported = Vec::new();
    specifiers(&dir.join("dist"), &mut imported);
    assert!(
        imported
            .iter()
            .any(|s| s == "@flick-lang/runtime/core/Option"),
        "{imported:?}"
    );
    let outside: Vec<&String> = imported
        .iter()
        .filter(|s| !s.starts_with('.') && !s.starts_with("@flick-lang/runtime/core"))
        .collect();
    assert!(outside.is_empty(), "{outside:?}");
}

#[test]
fn a_build_without_effects_rejects_what_needs_effect() {
    let dir = project(
        "build_without_effects_rejects",
        "module App.Main exposing (main)\n\nimport NonEmpty\n\nmain : Unit -> Unit\nlet main () =\n  let _ = NonEmpty.from_list [1]\n  ()\n",
    );
    let out = flick_in(&dir, &["build"]);
    assert!(!out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FLICK-E008") && stderr.contains("`NonEmpty`"),
        "{stderr}"
    );
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_build_without_effects_runs_without_loading_effect() {
    let dir = project("build_without_effects_runs", MAIN);
    let out = flick_in(&dir, &["build"]);
    assert!(out.status.success(), "{out:?}");
    // fails the run when anything resolves into the `effect` package
    let hook = "import { registerHooks } from \"node:module\";\nregisterHooks({ resolve(spec, ctx, next) { const r = next(spec, ctx); if (r.url.includes(\"/node_modules/effect/\")) throw new Error(`loaded ${r.url}`); return r; } });\n";
    fs::write(dir.join("no-effect.mjs"), hook).expect("write");
    let run = Command::new("node")
        .args(["--import", "./no-effect.mjs"])
        .arg(Path::new("dist").join(runner_file("main", false)))
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "[3, 6]\ntrue\n0\n");
}

/// Copies the directory `from` to `to`, as installing a package does.
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("dir");
    for entry in fs::read_dir(from).expect("dir") {
        let path = entry.expect("entry").path();
        let target = to.join(path.file_name().expect("name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            fs::copy(&path, &target).expect("copy");
        }
    }
}

/// `@acme/text`, built with `effect` in its `flick.toml` and installed into
/// an app at `dir/app` that builds with `app_effect`: `Text.Shout` is pure,
/// `Text.Log` needs Effect.
fn library_in_app(dir: &Path, effect: &str, app_effect: &str, main: &str) -> Output {
    let _ = fs::remove_dir_all(dir);
    let lib = dir.join("lib");
    fs::create_dir_all(lib.join("src/Text")).expect("dir");
    let config = format!(
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[build]\neffect = \"{effect}\"\n"
    );
    fs::write(lib.join("flick.toml"), config).expect("write");
    fs::write(lib.join("package.json"), "{ \"name\": \"@acme/text\" }\n").expect("write");
    let shout = "module Text.Shout exposing (shout)\n\nimport String\n\nshout : String -> String\nlet shout s = String.to_upper s ++ \"!\"\n";
    fs::write(lib.join("src/Text/Shout.fl"), shout).expect("write");
    if effect == "effect" {
        let log = "module Text.Log exposing (log)\n\nlog : String ->{; AppError} Unit\nlet log s = Platform.stdout_line s\n";
        fs::write(lib.join("src/Text/Log.fl"), log).expect("write");
    }
    let out = flick_in(&lib, &["build"]);
    assert!(out.status.success(), "{out:?}");
    let installed = dir.join("app/node_modules/@acme/text");
    copy_dir(&lib.join("dist"), &installed.join("dist"));
    fs::write(
        installed.join("package.json"),
        "{ \"name\": \"@acme/text\", \"flick\": { \"iface-dir\": \"./dist/\" } }\n",
    )
    .expect("write");
    let app = dir.join("app");
    fs::create_dir_all(app.join("src/App")).expect("dir");
    let config = format!(
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\neffect = \"{app_effect}\"\n"
    );
    fs::write(app.join("flick.toml"), config).expect("write");
    fs::write(
        app.join("package.json"),
        "{ \"name\": \"@acme/app\", \"dependencies\": { \"@acme/text\": \"1.0.0\" } }\n",
    )
    .expect("write");
    fs::write(app.join("src/App/Main.fl"), main).expect("write");
    flick_in(&app, &["build"])
}

const SHOUT_MAIN: &str = "module App.Main exposing (main)\n\nimport Text.Shout exposing (shout)\n\nsay : String -> Unit\nforeign pure say from global \"console.log\"\n\nmain : Unit -> Unit\nlet main () = say (shout \"hi\")\n";

#[test]
fn a_library_on_effect_is_also_built_without_it_for_its_pure_modules() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_without_effects_package");
    let out = library_in_app(&dir, "effect", "none", SHOUT_MAIN);
    assert!(out.status.success(), "{out:?}");
    let lib = dir.join("lib/dist");
    assert!(lib.join("_core/Text/Shout.js").exists() && !lib.join("_core/Text/Log.js").exists());
    let iface = fs::read_to_string(lib.join("Text/Shout.iface")).expect("iface");
    assert!(iface.contains("\"core\": \"_core/\""), "{iface}");
    let main = fs::read_to_string(dir.join("app/dist/App/Main.js")).expect("module");
    assert!(
        main.contains("from \"@acme/text/dist/_core/Text/Shout.js\""),
        "{main}"
    );
    let log = "module App.Main exposing (main)\n\nimport Text.Log exposing (log)\n\nmain : Unit -> Unit\nlet main () = ()\n";
    let out = library_in_app(&dir, "effect", "none", log);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FLICK-E008") && stderr.contains("`Text.Log` of `@acme/text`"),
        "{stderr}"
    );
}

#[test]
fn an_app_on_effect_refuses_a_library_built_only_without_it() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_without_effects_only");
    let main = "module App.Main exposing (main)\n\nimport Text.Shout exposing (shout)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (shout \"hi\")\n";
    let out = library_in_app(&dir, "none", "effect", main);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-P027"), "{stderr}");
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn an_app_without_effects_runs_a_library_built_on_effect() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_without_effects_package_runs");
    let out = library_in_app(&dir, "effect", "none", SHOUT_MAIN);
    assert!(out.status.success(), "{out:?}");
    let run = Command::new("node")
        .arg(Path::new("dist").join(runner_file("main", false)))
        .current_dir(dir.join("app"))
        .output()
        .expect("node");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "HI!\n", "{run:?}");
}
