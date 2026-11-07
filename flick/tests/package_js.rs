//! What a library brings on the js target: the npm packages of its
//! `[js.dependencies]`, which `flick install` adds to the project's
//! `package.json`, and the foreign modules beside its Flick modules.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{env, fs};

use common::{flick_in, project};

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
