//! Test companions: `src/Lib/Users.test.fl` tests what `Lib.Users` keeps to
//! itself under `flick test`, on both targets and on Flick's own runtime,
//! while `flick build` and `flick check` never read it, so the output of a
//! build is the same byte for byte with and without one.

use crate::common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use common::{flick_in, project};

const USERS: &str = r#"module Lib.Users exposing (greet)

type Title = Mr | Ms

trim : String -> String
let trim name = String.trim name

title : Title -> String
let title t =
  match t with
  | Mr -> "Mr"
  | Ms -> "Ms"

greet : String -> String
let greet name = "Hello, " ++ title Ms ++ " " ++ trim name ++ "!"
"#;

const COMPANION: &str = r#"module Lib.Users

suite "private helpers" {
  test "trim" { Test.expect (Test.equal "Ada" (trim "  Ada ")) }
  test "title" { Test.expect (Test.equal "Ms" (title Ms)) }
}

test "greet" { Test.expect (Test.equal "Hello, Ms Ada!" (greet " Ada")) }
"#;

const BY_EXPORTS: &str = r#"module Lib.UsersTest

import Lib.Users exposing (greet)

test "greets" { Test.expect (Test.equal "Hello, Ms Bo!" (greet "Bo")) }
"#;

fn library(name: &str, companion: Option<&str>) -> PathBuf {
    let mut files = vec![
        (
            "flick.toml",
            "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
        ),
        ("package.json", "{ \"name\": \"@acme/users\" }\n"),
        ("src/Lib/Users.fl", USERS),
        ("test/Lib/UsersTest.fl", BY_EXPORTS),
    ];
    files.extend(companion.map(|text| ("src/Lib/Users.test.fl", text)));
    project(name, &files)
}

/// Every file under `dir`, by its path from there, with its bytes.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).expect("read dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).expect("under the root");
                out.insert(relative.to_path_buf(), fs::read(&path).expect("read"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

#[test]
fn build_and_check_never_read_a_companion() {
    // the companion does not even check: neither command reports it
    let broken = format!("{COMPANION}\nlet broken : Float = \"one\"\n");
    let with = library("companion_build_with", Some(&broken));
    let without = library("companion_build_without", None);
    for dir in [&with, &without] {
        let check = flick_in(dir, &["check"]);
        assert_eq!(check.status.code(), Some(0), "{check:?}");
        for emit in ["js", "ts"] {
            let out = flick_in(
                dir,
                &["build", "--emit", emit, "--emit-dts", "--out-dir", emit],
            );
            assert_eq!(out.status.code(), Some(0), "{emit}: {out:?}");
        }
    }
    for emit in ["js", "ts"] {
        let (a, b) = (tree(&with.join(emit)), tree(&without.join(emit)));
        assert!(
            a.keys().any(|p| p.ends_with("Lib/Users.iface")),
            "{:?}",
            a.keys()
        );
        assert!(
            a.keys().any(|p| p.ends_with("Lib/Users.d.ts")),
            "{:?}",
            a.keys()
        );
        assert_eq!(
            a.keys().collect::<Vec<_>>(),
            b.keys().collect::<Vec<_>>(),
            "{emit}"
        );
        for (path, bytes) in &a {
            assert!(b[path] == *bytes, "{emit}: {} differs", path.display());
        }
    }
}

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn a_companion_tests_what_its_module_keeps_on_every_target() {
    let dir = library("companion_tests", Some(COMPANION));
    for flags in [&["--emit", "js"][..], &["--emit", "go"], &["--rt"]] {
        let out = flick_in(&dir, &[&["test"], flags].concat());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(0), "{flags:?}: {out:?}");
        for name in [
            "Lib.Users > private helpers > trim",
            "Lib.Users > private helpers > title",
            "Lib.Users > greet",
            "Lib.UsersTest > greets",
        ] {
            assert!(stdout.contains(name), "{flags:?}: {name} in {stdout}");
        }
        assert!(
            stdout.contains("4 passed · 0 failed"),
            "{flags:?}: {stdout}"
        );
    }
    // a module of the test root sees only the exports
    let private = BY_EXPORTS.replace("(greet \"Bo\")", "(Lib.Users.trim \"Bo\")");
    fs::write(dir.join("test/Lib/UsersTest.fl"), private).expect("write");
    let out = flick_in(&dir, &["test", "--emit", "js"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(stderr.contains("trim"), "{stderr}");
}
