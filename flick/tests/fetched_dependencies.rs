//! Dependencies fetched from git or a tarball: `flick.lock` pins them, the
//! package store holds their files, and a project compiles their sources
//! like those of a path dependency.

mod common;

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{fs, thread};

use common::project;
use flick_core::build::go::executable;

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

/// Serves `body` at `http://127.0.0.1:<port>/geo.tgz` until it is asked for
/// once; the URL. Any other request, such as the probe of a forwarder of
/// local ports, gets the body too and does not count.
fn serve_once(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("address").port();
    thread::spawn(move || {
        for mut stream in listener.incoming().map_while(Result::ok) {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request = String::new();
            let _ = reader.read_line(&mut request);
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
            if request.starts_with("GET /geo.tgz ") {
                break;
            }
        }
    });
    format!("http://127.0.0.1:{port}/geo.tgz")
}

/// Asks the server of `url` for `/`, as a forwarder of local ports (WSL, an
/// editor) does when a port starts listening.
fn probe(url: &str) {
    let address = url["http://".len()..]
        .split('/')
        .next()
        .expect("an address");
    let mut stream = TcpStream::connect(address).expect("the server");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: probe\r\n\r\n")
        .expect("a request");
    let _ = io::copy(&mut stream, &mut io::sink());
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
        .env("COPYFILE_DISABLE", "1")
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
    probe(&url);
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

/// `flick <args>` in `dir` with the package store `store`, without the
/// std of the working tree, as a released compiler runs.
fn released(dir: &Path, store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("FLICK_STORE", store)
        .env("XDG_CACHE_HOME", store.join("cache"))
        .env_remove("FLICK_STD_PATH")
        .output()
        .expect("flick runs")
}

#[test]
fn a_released_std_is_fetched_by_its_tarball() {
    let std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std");
    let store = project("std-tarball-store", &[]);
    let out = store.join("dist");
    let packed = released(&std, &store, &["pack", "--out-dir", &out.to_string_lossy()]);
    assert!(packed.status.success(), "{packed:?}");
    let line = String::from_utf8_lossy(&packed.stdout).into_owned();
    let (sha, file) = line.trim().split_once("  ").expect("sha and path");
    let url = serve_once(fs::read(file).expect("tarball"));
    let manifest = format!(
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[dependencies]\nstd = {{ url = \"{url}\", sha256 = \"{sha}\" }}\n"
    );
    let main = "module App.Main exposing (later)\n\nlater : Result Duration.DurationError Duration.Duration\nlet later = Duration.seconds 2\n";
    let app = project(
        "std-tarball-app",
        &[("flick.toml", &manifest), ("src/App/Main.fl", main)],
    );
    let installed = released(&app, &store, &["install"]);
    assert!(installed.status.success(), "{installed:?}");
    let checked = released(&app, &store, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
}

const GREET: &str = "module Lib.Greet exposing (hi)

hi : String ->{} String
foreign hi from \"./greet.mjs\" \"hi\"
";

const GREET_MAIN: &str = "module App.Main exposing (main)

import Lib.Greet

main : Unit ->{} Unit
let main () = Platform.stdout_line (Lib.Greet.hi \"there\")
";

/// The library `lib` of a foreign module on both targets, and an app of it
/// from `source`, built from the store by `emit`.
fn built_with_foreign(name: &str, source: &str, store: &Path, emit: &str) -> PathBuf {
    let manifest = format!(
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[dependencies]\nlib = {source}\n\n[build]\nemit = \"{emit}\"\n"
    );
    let app = project(
        name,
        &[("flick.toml", &manifest), ("src/App/Main.fl", GREET_MAIN)],
    );
    let install = run(&app, store, &["install"]);
    assert!(install.status.success(), "{install:?}");
    let build = run(&app, store, &["build"]);
    assert!(build.status.success(), "{build:?}");
    app
}

/// The library `lib` under `dir`.
fn foreign_library(name: &str) -> PathBuf {
    project(
        name,
        &[
            (
                "lib/flick.toml",
                "schema-version = 1\n\n[package]\nname = \"lib\"\nversion = \"1.0.0\"\n\n[project]\nkind = \"lib\"\n",
            ),
            ("lib/src/Lib/Greet.fl", GREET),
            (
                "lib/src/Lib/greet.mjs",
                "export const hi = (name) => `hello ${name}`;\n",
            ),
            (
                "lib/src/Lib/greet.go",
                "package main\n\nimport \"flick.app/flickrt\"\n\nfunc hi(fb *flickrt.Fiber, name flickrt.Value) flickrt.Value {\n\treturn \"hello \" + name.(string)\n}\n",
            ),
        ],
    )
}

/// A git source of `foreign_library`, tagged `v1.0.0`.
fn foreign_repository(name: &str) -> String {
    let repo = foreign_library(name);
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "lib"]);
    git(&repo, &["tag", "v1.0.0"]);
    format!(
        "{{ git = \"file://{}\", tag = \"v1.0.0\", subdir = \"lib\" }}",
        repo.display()
    )
}

/// A tarball source of `foreign_library`, as `flick pack` makes one.
fn foreign_tarball(name: &str, store: &Path) -> String {
    let lib = foreign_library(name).join("lib");
    let out = lib.join("packed");
    let packed = run(&lib, store, &["pack", "--out-dir", &out.to_string_lossy()]);
    assert!(packed.status.success(), "{packed:?}");
    let line = String::from_utf8_lossy(&packed.stdout).into_owned();
    let (sha, file) = line.trim().split_once("  ").expect("sha and path");
    let url = serve_once(fs::read(file).expect("tarball"));
    format!("{{ url = \"{url}\", sha256 = \"{sha}\" }}")
}

#[test]
fn a_fetched_library_ships_its_foreign_module() {
    let store = project("foreign-store", &[]);
    let sources = [
        foreign_repository("foreign-git"),
        foreign_tarball("foreign-tarball", &store),
    ];
    for (i, source) in sources.iter().enumerate() {
        let app = built_with_foreign(&format!("foreign-js-{i}"), source, &store, "js");
        let code = fs::read_to_string(app.join("dist/Lib/Greet.js")).expect("the module");
        assert!(code.contains("from \"./greet.mjs\""), "{code}");
        assert!(app.join("dist/Lib/greet.mjs").is_file());
    }
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_fetched_library_runs_with_its_foreign_module_on_js() {
    let store = project("foreign-node-store", &[]);
    let source = foreign_repository("foreign-node-git");
    let app = built_with_foreign("foreign-node", &source, &store, "js");
    let ran = Command::new("node")
        .arg("dist/flick-main.js")
        .current_dir(&app)
        .output()
        .expect("node runs");
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "hello there");
}

#[test]
#[ignore = "needs Go"]
fn a_fetched_library_runs_with_its_foreign_module_on_go() {
    let store = project("foreign-go-store", &[]);
    let sources = [
        foreign_repository("foreign-go-git"),
        foreign_tarball("foreign-go-tarball", &store),
    ];
    for (i, source) in sources.iter().enumerate() {
        let app = built_with_foreign(&format!("foreign-go-{i}"), source, &store, "go");
        // the file is named by its package, not by where the store is
        assert!(app.join("dist/go/main/ffi_lib_Lib_greet.go").is_file());
        let ran = Command::new(app.join("dist").join(executable("main")))
            .output()
            .expect("the program runs");
        assert!(ran.status.success(), "{ran:?}");
        assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "hello there");
    }
}
