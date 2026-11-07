//! What a library brings on the js target: the npm packages of its
//! `[js.dependencies]`, which `flick install` adds to the project's
//! `package.json`, and the foreign modules beside its Flick modules.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{env, fs};

use common::{flick_in, project};
use flick_core::build::go::executable;

const APP: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"Main\"

[dependencies]
pad = { path = \"../pad\" }
";

const MAIN: &str = "module Main exposing (main)

main : Unit -> Unit
let main () = ()
";

/// A library that needs the npm package `left-pad` at `version`.
fn library(version: &str) -> String {
    format!(
        "schema-version = 1

[project]
kind = \"lib\"

[js.dependencies]
left-pad = \"{version}\"
"
    )
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_build_names_the_npm_package_a_library_needs_until_it_is_installed() {
    let dir = project(
        "package_js_unmet",
        &[
            ("app/flick.toml", APP),
            ("app/src/Main.fl", MAIN),
            ("pad/flick.toml", &library("^1.3.0")),
        ],
    );
    let out = flick_in(dir.join("app"), &["build"]);
    assert!(!out.status.success(), "{out:?}");
    let text = stderr(&out);
    assert!(text.contains("FLICK-P034"), "{text}");
    assert!(
        text.contains("`pad` needs the npm package `left-pad`"),
        "{text}"
    );
    let check = flick_in(dir.join("app"), &["check"]);
    assert!(check.status.success(), "a check runs nothing: {check:?}");
}

#[test]
fn two_libraries_that_disagree_on_an_npm_version_are_reported() {
    let app = APP.replace(
        "pad = { path = \"../pad\" }",
        "pad = { path = \"../pad\" }\nother = { path = \"../other\" }",
    );
    let dir = project(
        "package_js_conflict",
        &[
            ("app/flick.toml", &app),
            ("app/src/Main.fl", MAIN),
            ("pad/flick.toml", &library("^1.3.0")),
            ("other/flick.toml", &library("^2.0.0")),
        ],
    );
    for args in [&["build"][..], &["install"][..]] {
        let out = flick_in(dir.join("app"), args);
        assert!(!out.status.success(), "{args:?}: {out:?}");
        let text = stderr(&out);
        assert!(text.contains("FLICK-P040"), "{args:?}: {text}");
        assert!(
            text.contains("`^1.3.0`") && text.contains("`^2.0.0`"),
            "{text}"
        );
    }
    assert!(
        !dir.join("app/package.json").exists(),
        "a refused install changes nothing"
    );
}

#[test]
fn the_project_package_json_counts_as_one_requirement() {
    let dir = project(
        "package_js_own_version",
        &[
            ("app/flick.toml", APP),
            ("app/src/Main.fl", MAIN),
            (
                "app/package.json",
                "{ \"dependencies\": { \"left-pad\": \"^1.0.0\" } }\n",
            ),
            ("pad/flick.toml", &library("^1.3.0")),
        ],
    );
    let out = flick_in(dir.join("app"), &["install"]);
    assert!(stderr(&out).contains("FLICK-P040"), "{out:?}");
}

/// A `pnpm` on `PATH` that `install` turns into an installed `left-pad`.
#[cfg(unix)]
fn fake_pnpm(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("dir");
    let script = bin.join("pnpm");
    fs::write(
        &script,
        "#!/bin/sh
if [ \"$1\" = --version ]; then echo 9.9.9; exit 0; fi
touch pnpm-lock.yaml
mkdir -p node_modules/left-pad
echo '{ \"name\": \"left-pad\", \"exports\": \"./index.mjs\" }' > node_modules/left-pad/package.json
echo 'export default (s, n) => s.padStart(n);' > node_modules/left-pad/index.mjs
",
    )
    .expect("write");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("chmod");
    bin
}

/// `flick <args>` in `dir` with the fake `pnpm` of `bin` first on `PATH`.
#[cfg(unix)]
fn with_pnpm(bin: &Path, dir: &Path, args: &[&str]) -> Output {
    let path = env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![bin.to_path_buf()];
    paths.extend(env::split_paths(&path));
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("PATH", env::join_paths(paths).expect("a path"))
        .output()
        .expect("flick runs")
}

#[cfg(unix)]
#[test]
fn install_adds_what_a_library_needs_to_the_package_json_and_pnpm_installs_it() {
    let manifest = "{\n  \"name\": \"app\",\n  \"packageManager\": \"pnpm@9.9.9\"\n}\n";
    let dir = project(
        "package_js_install",
        &[
            ("app/flick.toml", APP),
            ("app/src/Main.fl", MAIN),
            ("app/package.json", manifest),
            ("pad/flick.toml", &library("^1.3.0")),
        ],
    );
    let bin = fake_pnpm(&dir);
    let app = dir.join("app");
    let locked = with_pnpm(&bin, &app, &["install", "--locked"]);
    assert!(stderr(&locked).contains("FLICK-P034"), "{locked:?}");
    assert_eq!(
        fs::read_to_string(app.join("package.json")).expect("read"),
        manifest,
        "`--locked` changes nothing"
    );
    let installed = with_pnpm(&bin, &app, &["install"]);
    assert!(installed.status.success(), "{installed:?}");
    let written = fs::read_to_string(app.join("package.json")).expect("read");
    assert_eq!(
        written,
        "{\n  \"name\": \"app\",\n  \"packageManager\": \"pnpm@9.9.9\",\n  \"dependencies\": {\n    \"left-pad\": \"^1.3.0\"\n  }\n}\n"
    );
    let build = flick_in(&app, &["build"]);
    assert!(build.status.success(), "{build:?}");
    // the next install finds it all in place
    let again = with_pnpm(&bin, &app, &["install", "--locked"]);
    assert!(again.status.success(), "{again:?}");
    assert_eq!(
        fs::read_to_string(app.join("package.json")).expect("read"),
        written
    );
}

#[test]
fn a_manifest_names_only_packages_of_the_registry() {
    let dir = project(
        "package_js_invalid",
        &[
            ("app/flick.toml", APP),
            ("app/src/Main.fl", MAIN),
            ("pad/flick.toml", &library("file:../secret")),
        ],
    );
    let out = flick_in(dir.join("app"), &["check"]);
    let text = stderr(&out);
    assert!(text.contains("FLICK-P032"), "{text}");
    assert!(text.contains("file:../secret"), "{text}");
}

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

const GREET: &str = "module Lib.Greet exposing (hi)

hi : String ->{} String
foreign hi from \"./greet.mjs\" \"hi\"
";

const GREET_JS: &str = "import { mark } from \"./helper.mjs\";

export const hi = (name) => `${mark} ${name}`;
";

const HELPER_JS: &str = "export const mark = \"hello\";\n";

const GREET_GO: &str = "package main

import \"flick.app/flickrt\"

func hi(fb *flickrt.Fiber, name flickrt.Value) flickrt.Value {
\treturn \"hello \" + name.(string)
}
";

const LIB_APP: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"Main\"

[dependencies]
lib = { path = \"../lib\" }
";

const LIB_MAIN: &str = "module Main exposing (main)

import Lib.Greet

main : Unit ->{} Unit
let main () = Console.log (Lib.Greet.hi \"there\")
";

/// A project `app` that depends on the library `lib` by path, with `app`
/// emitting `emit`.
fn with_library(name: &str, emit: &str) -> PathBuf {
    project(
        name,
        &[
            ("lib/flick.toml", LIB),
            ("lib/src/Lib/Greet.fl", GREET),
            ("lib/src/Lib/greet.mjs", GREET_JS),
            ("lib/src/Lib/helper.mjs", HELPER_JS),
            ("lib/src/Lib/greet.go", GREET_GO),
            (
                "app/flick.toml",
                &format!("{LIB_APP}\n[build]\nemit = \"{emit}\"\n"),
            ),
            ("app/src/Main.fl", LIB_MAIN),
        ],
    )
}

#[test]
fn a_foreign_module_of_a_library_ships_beside_its_module() {
    let dir = with_library("package_js_ships", "js");
    let app = dir.join("app");
    let build = flick_in(&app, &["build"]);
    assert!(build.status.success(), "{build:?}");
    let dist = app.join("dist/Lib");
    let code = fs::read_to_string(dist.join("Greet.js")).expect("the module");
    assert!(code.contains("from \"./greet.mjs\""), "{code}");
    assert_eq!(
        fs::read_to_string(dist.join("greet.mjs")).expect("shipped"),
        GREET_JS
    );
    assert_eq!(
        fs::read_to_string(dist.join("helper.mjs")).expect("what it imports"),
        HELPER_JS
    );
    assert!(!dist.join("greet.go").exists());
    // a change of the file is a change of the build
    let changed = GREET_JS.replace("${mark}", "${mark}!");
    fs::write(dir.join("lib/src/Lib/greet.mjs"), &changed).expect("write");
    let build = flick_in(&app, &["build"]);
    assert!(build.status.success(), "{build:?}");
    assert_eq!(
        fs::read_to_string(dist.join("greet.mjs")).expect("shipped"),
        changed
    );
}

#[test]
fn a_foreign_module_of_the_project_stays_where_it_is() {
    let main = "module Main exposing (main)

now : Unit ->{} Float
foreign now from \"./clock.mjs\" \"now\"

main : Unit ->{} Unit
let main () =
  let _ = now ()
  ()
";
    let dir = with_library("package_js_own_file", "js");
    let app = dir.join("app");
    fs::write(app.join("src/Main.fl"), main).expect("write");
    fs::write(app.join("src/clock.mjs"), "export const now = () => 1;\n").expect("write");
    let build = flick_in(&app, &["build"]);
    assert!(build.status.success(), "{build:?}");
    let code = fs::read_to_string(app.join("dist/Main.js")).expect("the module");
    assert!(code.contains("from \"../src/clock.mjs\""), "{code}");
    assert!(!app.join("dist/clock.mjs").exists());
}

#[test]
fn a_file_a_library_cannot_ship_is_reported() {
    let dir = with_library("package_js_missing", "js");
    fs::remove_file(dir.join("lib/src/Lib/greet.mjs")).expect("remove");
    let out = flick_in(dir.join("app"), &["build"]);
    assert!(!out.status.success(), "{out:?}");
    let text = stderr(&out);
    assert!(text.contains("FLICK-P041"), "{text}");
    assert!(text.contains("./greet.mjs"), "{text}");
}

#[test]
fn two_libraries_cannot_ship_different_files_at_one_path() {
    let dir = with_library("package_js_clash", "js");
    let other = "module Other.Thing exposing (thing)

thing : Unit ->{} Float
foreign thing from \"../Lib/helper.mjs\" \"thing\"
";
    for (file, text) in [
        ("other/flick.toml", LIB),
        ("other/src/Other/Thing.fl", other),
        (
            "other/src/Lib/helper.mjs",
            "export const thing = () => 1;\n",
        ),
    ] {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().expect("a parent")).expect("dir");
        fs::write(path, text).expect("write");
    }
    fs::write(
        dir.join("app/flick.toml"),
        LIB_APP.replace(
            "lib = { path = \"../lib\" }",
            "lib = { path = \"../lib\" }\nother = { path = \"../other\" }",
        ),
    )
    .expect("write");
    fs::write(
        dir.join("app/src/Main.fl"),
        LIB_MAIN.replace(
            "import Lib.Greet\n",
            "import Lib.Greet\nimport Other.Thing\n",
        ) + "\nthing : Unit ->{} Float\nlet thing = Other.Thing.thing\n",
    )
    .expect("write");
    let out = flick_in(dir.join("app"), &["build"]);
    let text = stderr(&out);
    assert!(text.contains("FLICK-P041"), "{text}");
    assert!(text.contains("Lib/helper.mjs"), "{text}");
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_shipped_foreign_module_runs_and_finds_npm_packages_from_the_project() {
    let dir = with_library("package_js_runs", "js");
    let app = dir.join("app");
    let run = flick_in(&app, &["run"]);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "hello there");
    // a file of the library imports what the project installs, as `flick install` does
    let pad = app.join("node_modules/left-pad");
    fs::create_dir_all(&pad).expect("dir");
    fs::write(
        pad.join("package.json"),
        "{ \"name\": \"left-pad\", \"exports\": \"./index.mjs\" }\n",
    )
    .expect("write");
    fs::write(pad.join("index.mjs"), "export default (s) => `  ${s}`;\n").expect("write");
    fs::write(
        app.join("package.json"),
        "{ \"dependencies\": { \"left-pad\": \"^1.3.0\" } }\n",
    )
    .expect("write");
    fs::write(
        dir.join("lib/flick.toml"),
        format!("{LIB}\n[js.dependencies]\nleft-pad = \"^1.3.0\"\n"),
    )
    .expect("write");
    fs::write(
        dir.join("lib/src/Lib/greet.mjs"),
        "import pad from \"left-pad\";\n\nexport const hi = (name) => pad(name);\n",
    )
    .expect("write");
    let run = flick_in(&app, &["run"]);
    assert!(run.status.success(), "{run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim_end(), "  there");
}

#[test]
#[ignore = "needs Go"]
fn a_foreign_go_file_of_a_library_is_compiled_under_the_name_of_its_package() {
    let dir = with_library("package_js_go", "go");
    let app = dir.join("app");
    // the go target needs none of the npm packages of a library
    fs::write(
        dir.join("lib/flick.toml"),
        format!("{LIB}\n[js.dependencies]\nleft-pad = \"^1.3.0\"\n"),
    )
    .expect("write");
    let build = flick_in(&app, &["build"]);
    assert!(build.status.success(), "{build:?}");
    assert!(app.join("dist/go/main/ffi_lib_Lib_greet.go").is_file());
    let run = Command::new(app.join("dist").join(executable("main")))
        .output()
        .expect("the program runs");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "hello there");
}

#[test]
#[ignore = "needs pnpm"]
fn a_packed_app_has_the_foreign_modules_of_its_libraries() {
    let dir = with_library("package_js_pack", "js");
    let app = dir.join("app");
    fs::write(
        app.join("package.json"),
        "{ \"name\": \"packed-app\", \"version\": \"1.0.0\" }\n",
    )
    .expect("write");
    let out = dir.join("out");
    let pack = flick_in(&app, &["pack", "--out-dir", &out.to_string_lossy()]);
    assert!(pack.status.success(), "{pack:?}");
    let tgz = fs::read_dir(&out)
        .expect("the tarball's directory")
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "tgz"))
        .expect("a tarball");
    let listed = Command::new("tar")
        .arg("tzf")
        .arg(&tgz)
        .output()
        .expect("tar runs");
    let listed = String::from_utf8_lossy(&listed.stdout);
    for file in ["Lib/Greet.js", "Lib/greet.mjs", "Lib/helper.mjs"] {
        assert!(listed.contains(&format!("package/dist/{file}")), "{listed}");
    }
}
