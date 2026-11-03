//! Dependencies fetched from git or a tarball: `flick.lock` pins them, the
//! package store holds their files, and a project compiles their sources
//! like those of a path dependency.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{fs, thread};

use common::project;

const APP: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[dependencies]
geo = { git = \"https://example.com/geo\", tag = \"v1.0.0\", subdir = \"lib\" }
";

const MAIN: &str = "module App.Main exposing (main, start)

import Geo exposing (origin)

start : Float
let start = origin.x

main : Unit -> Unit
let main () = ()
";

const LIB: &str = "schema-version = 1

[package]
name = \"geo\"
version = \"1.0.0\"
flick = \"REQUIREMENT\"

[project]
kind = \"lib\"
";

const GEO: &str = "module Geo exposing (Point, origin)

type alias Point = { x : Float; y : Float }

origin : Point
let origin = { x = 0.0; y = 0.0 }
";

const HASH: &str = "4f0e0bd5b9b8b1f2a0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a3b2c1d0e9f8a7";

fn lock(source: &str) -> String {
    format!(
        "version = 1\n\n[[package]]\nname = \"geo\"\nversion = \"1.0.0\"\nsource = \"{source}\"\nchecksum = \"sha256:{HASH}\"\n"
    )
}

/// `flick check` in `dir` with the package store `store`, as NDJSON codes.
fn check(dir: &Path, store: &Path) -> (Option<i32>, Vec<String>) {
    let out: Output = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["check", "--diagnostics", "json"])
        .current_dir(dir)
        .env("FLICK_STORE", store)
        .output()
        .expect("flick runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let codes = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|v| v["code"].as_str().map(str::to_owned))
        .collect();
    (out.status.code(), codes)
}

/// A store with the fetched repository of `geo`, which needs `requirement`.
fn store(name: &str, requirement: &str) -> PathBuf {
    let lib = LIB.replace("REQUIREMENT", requirement);
    let files = [
        (format!("{HASH}/lib/flick.toml"), lib),
        (format!("{HASH}/lib/src/Geo.fl"), GEO.to_owned()),
    ];
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    project(name, &files)
}

#[test]
fn a_locked_git_dependency_compiles_from_the_store() {
    let store = store("fetched-store", "*");
    let source = "git+https://example.com/geo?tag=v1.0.0&subdir=lib#0123abcd";
    let app = project(
        "fetched-app",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("flick.lock", &lock(source)),
        ],
    );
    assert_eq!(check(&app, &store), (Some(0), Vec::new()));
}

#[test]
fn a_dependency_missing_from_the_lock_is_reported() {
    let store = store("fetched-store-other-tag", "*");
    let source = "git+https://example.com/geo?tag=v0.9.0&subdir=lib#0123abcd";
    let app = project(
        "fetched-app-other-tag",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("flick.lock", &lock(source)),
        ],
    );
    let (status, codes) = check(&app, &store);
    assert_eq!(status, Some(1));
    assert!(codes.contains(&"FLICK-P034".to_owned()), "{codes:?}");
}

#[test]
fn a_library_for_another_compiler_is_reported() {
    let store = store("fetched-store-old", "<0.0.1");
    let source = "git+https://example.com/geo?tag=v1.0.0&subdir=lib#0123abcd";
    let app = project(
        "fetched-app-old",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("flick.lock", &lock(source)),
        ],
    );
    let (status, codes) = check(&app, &store);
    assert_eq!(status, Some(1));
    assert!(codes.contains(&"FLICK-P033".to_owned()), "{codes:?}");
}

/// `flick <args>` in `dir` with the package store `store`.
fn run(dir: &Path, store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("FLICK_STORE", store)
        .output()
        .expect("flick runs")
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// The library `geo` as a repository at `lib/` of `name`, tagged `v1.0.0`.
fn repository(name: &str) -> PathBuf {
    let lib = LIB.replace("REQUIREMENT", "*");
    let repo = project(name, &[("lib/flick.toml", &lib), ("lib/src/Geo.fl", GEO)]);
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "geo"]);
    git(&repo, &["tag", "v1.0.0"]);
    repo
}

#[test]
fn install_fetches_a_git_tag_and_pins_it() {
    let repo = repository("install-geo-repo");
    let store = project("install-store", &[]);
    let manifest = APP.replace(
        "https://example.com/geo",
        &format!("file://{}", repo.display()),
    );
    let app = project(
        "install-app",
        &[("flick.toml", &manifest), ("src/App/Main.fl", MAIN)],
    );
    let out = run(&app, &store, &["install"]);
    assert!(out.status.success(), "{out:?}");
    let lock = fs::read_to_string(app.join("flick.lock")).expect("a lock");
    assert!(
        lock.contains("name = \"geo\"") && lock.contains("version = \"1.0.0\""),
        "{lock}"
    );
    assert!(
        lock.contains("?tag=v1.0.0&subdir=lib#"),
        "the commit is pinned: {lock}"
    );
    assert_eq!(check(&app, &store), (Some(0), Vec::new()));
    // the store has it: no network, and the lock stays as it is
    fs::remove_dir_all(&repo).expect("remove the repository");
    let again = run(&app, &store, &["install", "--locked", "--offline"]);
    assert!(again.status.success(), "{again:?}");
    assert_eq!(
        fs::read_to_string(app.join("flick.lock")).expect("lock"),
        lock
    );
}

#[test]
fn install_locked_refuses_a_dependency_the_lock_lacks() {
    let repo = repository("locked-geo-repo");
    let store = project("locked-store", &[]);
    let manifest = APP.replace(
        "https://example.com/geo",
        &format!("file://{}", repo.display()),
    );
    let app = project(
        "locked-app",
        &[("flick.toml", &manifest), ("src/App/Main.fl", MAIN)],
    );
    let out = run(&app, &store, &["install", "--locked"]);
    assert!(!out.status.success());
    assert!(!app.join("flick.lock").exists());
}

/// Serves `body` once at `http://127.0.0.1:<port>/geo.tgz`; the URL.
fn serve_once(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("address").port();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("a request");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
            line.clear();
        }
        let head = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(&body);
    });
    format!("http://127.0.0.1:{port}/geo.tgz")
}

#[test]
fn install_fetches_a_tarball_of_the_named_hash() {
    let lib = LIB.replace("REQUIREMENT", "*");
    let src = project(
        "tarball-src",
        &[("package/flick.toml", &lib), ("package/src/Geo.fl", GEO)],
    );
    let tgz = src.join("geo.tgz");
    let packed = Command::new("tar")
        .args(["czf", "geo.tgz", "package"])
        .current_dir(&src)
        .status()
        .expect("tar runs");
    assert!(packed.success());
    let bytes = fs::read(&tgz).expect("tarball");
    let sum = Command::new("sha256sum")
        .arg(&tgz)
        .output()
        .expect("sha256sum runs");
    let sha = String::from_utf8_lossy(&sum.stdout)[..64].to_owned();
    let url = serve_once(bytes);
    let manifest = APP.replace(
        "{ git = \"https://example.com/geo\", tag = \"v1.0.0\", subdir = \"lib\" }",
        &format!("{{ url = \"{url}\", sha256 = \"{sha}\" }}"),
    );
    let store = project("tarball-store", &[]);
    let app = project(
        "tarball-app",
        &[("flick.toml", &manifest), ("src/App/Main.fl", MAIN)],
    );
    let out = run(&app, &store, &["install"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(check(&app, &store), (Some(0), Vec::new()));
}

#[test]
fn add_pins_a_library_and_remove_drops_it() {
    let repo = repository("add-geo-repo");
    let store = project("add-store", &[]);
    let bare = "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n";
    let app = project(
        "add-app",
        &[("flick.toml", bare), ("src/App/Main.fl", MAIN)],
    );
    let url = format!("file://{}", repo.display());
    let bad = run(&app, &store, &["add", "geo", "--git", &url, "--tag", "v9"]);
    assert!(!bad.status.success());
    assert_eq!(
        fs::read_to_string(app.join("flick.toml")).expect("manifest"),
        bare,
        "a failed add changes nothing"
    );
    let added = run(
        &app,
        &store,
        &[
            "add", "geo", "--git", &url, "--tag", "v1.0.0", "--subdir", "lib",
        ],
    );
    assert!(added.status.success(), "{added:?}");
    assert_eq!(check(&app, &store), (Some(0), Vec::new()));
    let listed = run(&app, &store, &["deps"]);
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains("geo 1.0.0"),
        "{listed:?}"
    );
    let removed = run(&app, &store, &["remove", "geo"]);
    assert!(removed.status.success(), "{removed:?}");
    let manifest = fs::read_to_string(app.join("flick.toml")).expect("manifest");
    assert!(!manifest.contains("geo ="), "{manifest}");
    let lock = fs::read_to_string(app.join("flick.lock")).expect("lock");
    assert!(!lock.contains("geo"), "{lock}");
}
