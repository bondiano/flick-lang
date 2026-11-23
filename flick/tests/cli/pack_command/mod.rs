//! Backend-neutral `flick pack`: deterministic Flick source tarballs that
//! round-trip through `flick add --url ... --sha256 ...`, Go projects packing
//! without Node or pnpm, `FLICK-P024` and API-diff warnings across targets,
//! and optional JS backend `pack` capabilities.

mod go;
mod js;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output};
use std::{fs, thread};

use crate::common::{flick_in, project};

#[cfg(unix)]
pub(super) fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

pub(super) fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn run_store(dir: &Path, store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("FLICK_STORE", store)
        .output()
        .expect("flick runs")
}

/// Serves `body` over loopback HTTP until `GET /pkg.tgz` is requested once.
pub(super) fn serve_tarball(body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("local addr").port();
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
            if request.starts_with("GET /pkg.tgz ") {
                break;
            }
        }
    });
    format!("http://127.0.0.1:{port}/pkg.tgz")
}

#[cfg(unix)]
pub(super) fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

pub(super) fn tar_entries(tgz: &Path) -> Vec<String> {
    let out = Command::new("tar")
        .arg("-tzf")
        .arg(tgz)
        .output()
        .expect("tar runs");
    assert!(out.status.success(), "{out:?}");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

/// AC #1: A tarball produced by `flick pack` installs through
/// `flick add --url ... --sha256 ...`, and the dependent project builds.
#[test]
fn pack_tarball_installs_via_flick_add_url_and_dependent_builds() {
    let store = project("pack_add_url_store", &[]);
    let lib = project(
        "pack_add_url_lib",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"mathlib\"\nversion = \"0.2.0\"\n\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/Math/Double.fl",
                "module Math.Double exposing (twice)\n\ntwice : Int -> Int\nlet twice n = n * 2\n",
            ),
            ("README.md", "# mathlib\n"),
        ],
    );
    let packed = run_store(&lib, &store, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(packed.status.code(), Some(0), "{packed:?}");
    let line = stdout(&packed);
    let (sha, path) = line.trim().split_once("  ").expect("sha and tarball path");
    assert_eq!(sha.len(), 64, "{line}");
    let bytes = fs::read(path).expect("packed tarball");
    let url = serve_tarball(bytes);

    let app = project(
        "pack_add_url_app",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nimport Math.Double\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (Math.Double.twice 21))\n",
            ),
        ],
    );
    let added = run_store(
        &app,
        &store,
        &["add", "mathlib", "--url", &url, "--sha256", sha],
    );
    assert_eq!(added.status.code(), Some(0), "{added:?}");
    let manifest = fs::read_to_string(app.join("flick.toml")).expect("manifest");
    assert!(
        manifest.contains(&format!(
            "mathlib = {{ url = \"{url}\", sha256 = \"{sha}\" }}"
        )),
        "{manifest}"
    );
    let built = run_store(&app, &store, &["build"]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    let emitted = fs::read_to_string(app.join("dist/Math/Double.js")).expect("compiled lib");
    assert!(emitted.contains("twice"), "{emitted}");
}

/// Plugin component declared in `[package] plugin` is included in the Flick
/// source tarball and installs cleanly through `flick add --url --sha256`.
#[test]
fn pack_includes_declared_plugin_component_in_source_tarball() {
    let store = project("pack_plugin_store", &[]);
    let lib = project(
        "pack_plugin_lib",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"myplug\"\nversion = \"0.1.0\"\nplugin = \"plug.wasm\"\n\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/Plug.fl",
                "module Plug exposing (ok)\n\nok : Bool\nlet ok = true\n",
            ),
            ("plug.wasm", "fake-wasm-bytes"),
        ],
    );
    let packed = run_store(&lib, &store, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(packed.status.code(), Some(0), "{packed:?}");
    let line = stdout(&packed);
    let (sha, path) = line.trim().split_once("  ").expect("sha and path");
    let entries = tar_entries(Path::new(path));
    assert!(
        entries.iter().any(|e| e == "package/plug.wasm"),
        "expected package/plug.wasm in {entries:?}"
    );
    let url = serve_tarball(fs::read(path).expect("tarball"));
    let consumer = project(
        "pack_plugin_consumer",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/Use.fl",
                "module Use exposing (v)\n\nimport Plug\n\nv : Bool\nlet v = Plug.ok\n",
            ),
        ],
    );
    let added = run_store(
        &consumer,
        &store,
        &["add", "myplug", "--url", &url, "--sha256", sha],
    );
    assert_eq!(added.status.code(), Some(0), "{added:?}");
    let listed = run_store(&consumer, &store, &["plugins"]);
    assert_eq!(listed.status.code(), Some(0), "{listed:?}");
    assert!(
        stdout(&listed).contains("myplug:") && stdout(&listed).contains("plug.wasm"),
        "{}",
        stdout(&listed)
    );
}

/// Edge case: `source-dir = "."` skips `.git` (both directories and worktree
/// regular files), `.flick`, `dist`, symlinked skipped dirs (`dist`,
/// `node_modules`), and `out-dir` / the destination archive across repeated runs
/// (including `--out-dir .`).
#[test]
fn pack_with_dot_source_dir_skips_metadata_and_output_dirs() {
    let dir = project(
        "pack_dot_source_dir",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"dotlib\"\nversion = \"0.1.0\"\n\n[project]\nkind = \"lib\"\nsource-dir = \".\"\n",
            ),
            (
                "Root.fl",
                "module Root exposing (x)\n\nx : Int\nlet x = 1\n",
            ),
            (".git/HEAD", "ref: refs/heads/main\n"),
            ("dist/stale.js", "export const stale = 1;\n"),
        ],
    );
    let first = flick_in(&dir, &["pack", "--target", "go", "--out-dir", "artifacts"]);
    assert_eq!(first.status.code(), Some(0), "{first:?}");
    let second = flick_in(&dir, &["pack", "--target", "go", "--out-dir", "artifacts"]);
    assert_eq!(second.status.code(), Some(0), "{second:?}");
    assert_eq!(stdout(&first), stdout(&second));
    let entries = tar_entries(&dir.join("artifacts/dotlib-0.1.0.tgz"));
    assert_eq!(entries, ["package/Root.fl", "package/flick.toml"]);

    // Worktree/submodule `.git` regular file, symlinked skipped dirs, and `--out-dir .`
    // repeated twice must keep identical bytes and never leak `.git` or the tarball itself.
    let worktree = project(
        "pack_dot_worktree",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"wtlib\"\nversion = \"0.1.0\"\n\n[project]\nkind = \"lib\"\nsource-dir = \".\"\n",
            ),
            (
                "Root.fl",
                "module Root exposing (x)\n\nx : Int\nlet x = 2\n",
            ),
            (".git", "gitdir: /abs/local/path/.git/worktrees/demo\n"),
        ],
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = project(
            "pack_dot_worktree_outside",
            &[
                ("nm/pkg.js", "export {};\n"),
                ("dist/old.js", "export {};\n"),
            ],
        );
        symlink(outside.join("nm"), worktree.join("node_modules")).expect("symlink node_modules");
        symlink(outside.join("dist"), worktree.join("dist")).expect("symlink dist");
    }
    let go1 = flick_in(&worktree, &["pack", "--target", "go", "--out-dir", "."]);
    assert_eq!(go1.status.code(), Some(0), "{go1:?}");
    let go2 = flick_in(&worktree, &["pack", "--target", "go", "--out-dir", "."]);
    assert_eq!(go2.status.code(), Some(0), "{go2:?}");
    assert_eq!(stdout(&go1), stdout(&go2));
    assert_eq!(
        tar_entries(&worktree.join("wtlib-0.1.0.tgz")),
        ["package/Root.fl", "package/flick.toml"]
    );

    // On default JS target, symlinked `node_modules` (a backend package dir) is also skipped cleanly.
    let js_wt = project(
        "pack_dot_worktree_js",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"wtjs\"\nversion = \"0.1.0\"\n\n[project]\nkind = \"lib\"\nsource-dir = \".\"\n",
            ),
            (
                "Root.fl",
                "module Root exposing (x)\n\nx : Int\nlet x = 3\n",
            ),
            (".git", "gitdir: /abs/local/path/.git/worktrees/js\n"),
        ],
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = project(
            "pack_dot_worktree_js_outside",
            &[("nm/pkg.js", "export {};\n")],
        );
        symlink(outside.join("nm"), js_wt.join("node_modules")).expect("symlink node_modules");
    }
    let js1 = flick_in(&js_wt, &["pack", "--out-dir", "."]);
    assert_eq!(js1.status.code(), Some(0), "{js1:?}");
    let js2 = flick_in(&js_wt, &["pack", "--out-dir", "."]);
    assert_eq!(js2.status.code(), Some(0), "{js2:?}");
    assert_eq!(stdout(&js1), stdout(&js2));
    assert_eq!(
        tar_entries(&js_wt.join("wtjs-0.1.0.tgz")),
        ["package/Root.fl", "package/flick.toml"]
    );
}
