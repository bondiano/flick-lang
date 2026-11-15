//! `flick plugins` lists the plugin of every dependency source: a path, a git
//! tag and a url with its sha256, the same chain a restored store shows.

use std::fs;
use std::io::{BufRead, BufReader, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;

use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};

use super::common::project;

const HASH: &str = "4f0e0bd5b9b8b1f2a0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a3b2c1d0e9f8a7";

const APP: &str = "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n";

const MAIN: &str = "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n";

/// An app whose only dependency is a fetched `geo` library.
const FETCHED: &str = "schema-version = 1

[project]
kind = \"app\"
entry = \"App.Main\"

[dependencies]
geo = { git = \"https://example.com/geo\", tag = \"v1.0.0\", subdir = \"lib\" }
";

fn plugin_manifest(name: &str) -> String {
    format!(
        "schema-version = 1\n\n[package]\nname = \"{name}\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nwatch = [\"Pairs.tally\"]\n\n[project]\nkind = \"lib\"\n"
    )
}

fn std_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std")
}

fn flick(dir: &Path, store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("FLICK_STORE", store)
        .env("FLICK_STD_PATH", std_path())
        .env("NO_COLOR", "1")
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

/// The library `name` as a repository with its plugin at `pkg/`, tagged v1.0.0.
fn repository(name: &str) -> PathBuf {
    let pairs = plugin_manifest(name);
    let repo = project(
        name,
        &[("pkg/flick.toml", &pairs), ("pkg/pairs.wasm", "git build")],
    );
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "--quiet", "-m", "pairs"]);
    git(&repo, &["tag", "v1.0.0"]);
    repo
}

/// A gzipped tarball holding `package/` with `files`, as a url dependency
/// names it, and the sha256 of its bytes.
fn tarball(files: &[(&str, &str)]) -> (Vec<u8>, String) {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (path, text) in files {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(text.len() as u64);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("package/{path}"), text.as_bytes())
            .expect("append");
    }
    let bytes = builder
        .into_inner()
        .expect("the archive")
        .finish()
        .expect("gzip");
    let digest = Sha256::digest(&bytes);
    (bytes, format!("{digest:x}"))
}

/// Serves `body` at `http://127.0.0.1:<port>/pairs.tgz` until it is asked for;
/// the URL. Another request, such as the probe of a forwarder of local ports,
/// gets the body too and does not count.
fn serve(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let address = listener.local_addr().expect("an address");
    thread::spawn(move || {
        for mut stream in listener.incoming().map_while(Result::ok) {
            let mut reader = BufReader::new(stream.try_clone().expect("a stream"));
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
            if request.starts_with("GET /pairs.tgz ") {
                break;
            }
        }
    });
    format!("http://{address}/pairs.tgz")
}

/// A workspace whose `app` member inherits a path, a git and a url plugin.
fn workspace(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let path_pairs = plugin_manifest("path_pairs");
    let root = project(
        name,
        &[
            ("path_pairs/flick.toml", &path_pairs),
            ("path_pairs/pairs.wasm", "path build"),
            ("app/flick.toml", APP),
            ("app/src/App/Main.fl", MAIN),
        ],
    );
    let repo = repository(&format!("{name}-repo"));
    let url_pairs = plugin_manifest("url_pairs");
    let (tgz, sha) = tarball(&[
        ("flick.toml", url_pairs.as_str()),
        ("pairs.wasm", "url build"),
    ]);
    let url = serve(tgz);
    let manifest = format!(
        "schema-version = 1\n\n[workspace]\nmembers = [\"app\"]\n\n\
         [[workspace.plugins]]\npackage = \"path_pairs\"\nsource = {{ path = \"path_pairs\" }}\n\n\
         [[workspace.plugins]]\npackage = \"git_pairs\"\nsource = {{ git = \"file://{}\", tag = \"v1.0.0\", subdir = \"pkg\" }}\n\n\
         [[workspace.plugins]]\npackage = \"url_pairs\"\nsource = {{ url = \"{url}\", sha256 = \"{sha}\" }}\n",
        repo.display()
    );
    fs::write(root.join("flick.toml"), manifest).expect("the workspace manifest");
    let store = project(&format!("{name}-store"), &[]);
    (root, store, repo)
}

#[test]
fn path_git_and_url_plugins_are_listed_in_declaration_order() {
    let (root, store, repo) = workspace("plugins-sources");
    let app = root.join("app");
    let installed = flick(&app, &store, &["install"]);
    assert!(installed.status.success(), "{installed:?}");
    let listed = flick(&app, &store, &["plugins"]);
    assert!(listed.status.success(), "{listed:?}");
    let stdout = String::from_utf8_lossy(&listed.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 4, "{stdout}");
    assert!(lines[0].starts_with("std: linked into flick"), "{stdout}");
    assert!(
        lines[1].starts_with("path_pairs (from the workspace): ../path_pairs/pairs.wasm, sha256:"),
        "{stdout}"
    );
    assert!(lines[1].ends_with(", pinned"), "{stdout}");
    assert!(
        lines[2].starts_with("git_pairs (from the workspace): git+file://"),
        "{stdout}"
    );
    assert!(lines[2].contains("?tag=v1.0.0&subdir=pkg#"), "{stdout}");
    assert!(lines[2].contains("(pairs.wasm), sha256:"), "{stdout}");
    assert!(lines[2].ends_with(", pinned"), "{stdout}");
    assert!(
        lines[3].starts_with("url_pairs (from the workspace): url+http://127.0.0.1:"),
        "{stdout}"
    );
    assert!(
        lines[3].contains("/pairs.tgz (pairs.wasm), sha256:"),
        "{stdout}"
    );
    assert!(lines[3].ends_with(", pinned"), "{stdout}");
    assert!(
        !stdout.contains(&store.to_string_lossy().to_string()),
        "the package store is no source: {stdout}"
    );
    // a restored project needs neither the repository nor the server: the
    // lock and the store give the same chain
    fs::remove_dir_all(repo).expect("drop the repository");
    let lock = fs::read(app.join("flick.lock")).expect("the lock");
    let restored = flick(&app, &store, &["--offline", "plugins"]);
    assert_eq!(restored.status.code(), Some(0), "{restored:?}");
    assert_eq!(
        restored.stdout.as_slice(),
        listed.stdout.as_slice(),
        "the restored chain differs"
    );
    assert_eq!(fs::read(app.join("flick.lock")).expect("the lock"), lock);
}

fn fetched_app(name: &str, lock: Option<&str>) -> PathBuf {
    let mut files = vec![("flick.toml", FETCHED), ("src/App/Main.fl", MAIN)];
    if let Some(lock) = lock {
        files.push(("flick.lock", lock));
    }
    project(name, &files)
}

fn lock(source: &str) -> String {
    format!(
        "version = 1\n\n[[package]]\nname = \"geo\"\nversion = \"1.0.0\"\nsource = \"{source}\"\nchecksum = \"sha256:{HASH}\"\n"
    )
}

fn assert_p034(dir: &Path, store: &Path, message: &str) {
    let out = flick(dir, store, &["plugins"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-P034"), "{stderr}");
    assert!(stderr.contains(message), "{stderr}");
}

#[test]
fn a_fetched_plugin_without_a_lock_is_p034() {
    let store = project("plugins-no-lock-store", &[]);
    let app = fetched_app("plugins-no-lock", None);
    assert_p034(
        &app,
        &store,
        "dependency `geo` is not in `flick.lock`: run `flick install`",
    );
}

#[test]
fn a_lock_for_another_tag_is_p034() {
    let store = project("plugins-stale-store", &[]);
    let stale = lock("git+https://example.com/geo?tag=v0.9.0&subdir=lib#0123abcd");
    let app = fetched_app("plugins-stale-lock", Some(&stale));
    assert_p034(
        &app,
        &store,
        "dependency `geo` is not in `flick.lock`: run `flick install`",
    );
}

#[test]
fn a_locked_package_missing_from_the_store_is_p034() {
    let store = project("plugins-empty-store", &[]);
    let pinned = lock("git+https://example.com/geo?tag=v1.0.0&subdir=lib#0123abcd");
    let app = fetched_app("plugins-missing-store", Some(&pinned));
    assert_p034(&app, &store, "dependency `geo` is not fetched");
}

#[test]
fn an_invalid_lock_is_p034() {
    let store = project("plugins-invalid-store", &[]);
    let app = fetched_app("plugins-invalid-lock", Some("version = 2\n"));
    assert_p034(&app, &store, "invalid `flick.lock`");
}
