mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::flick_in;
use flick_core::build::runner_file;

const CONFIG: &str = "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\neffect = \"none\"\n";

const MAIN: &str = "module App.Main exposing (main)

import Option

say : String -> Unit
foreign pure say from global \"console.log\"

type Shape = Circle Float | Rect Float Float
  deriving Eq, Show

area : Shape -> Float
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
