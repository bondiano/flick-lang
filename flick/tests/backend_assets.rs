//! The official backends are release assets, not part of the compiler: a
//! project without `[build.backend]` resolves the JavaScript component from
//! the backends index, pins it in `flick.lock` and keeps it in the package
//! store. The index and the component here are local files that
//! `FLICK_BACKENDS_INDEX` names, so no test reaches the network.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;
use sha2::{Digest, Sha256};

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";
const MODULE: &str = "module Lib exposing (answer)\n\nanswer : Int\nlet answer = 42\n";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn component() -> Vec<u8> {
    fs::read(root().join("backends/js/package/component.wasm")).expect("the JS component")
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for b in Sha256::digest(bytes) {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// A scratch directory holding a project, a store and an index.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = root().join("target/tmp/backend_assets").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("app/src")).expect("project");
        fs::write(dir.join("app/flick.toml"), LIB).expect("config");
        fs::write(dir.join("app/src/Lib.fl"), MODULE).expect("module");
        fs::create_dir_all(dir.join("index")).expect("index");
        fs::write(dir.join("index/component.wasm"), component()).expect("component");
        Self { dir }
    }

    /// Writes the index with one entry for `flick/js`, its fields replaced
    /// by `change`.
    fn index(&self, change: impl FnOnce(&mut serde_json::Value)) {
        let mut entry = json!({
            "id": "flick/js",
            "version": "0.1.0",
            "sha256": hex(&component()),
            "url": self.dir.join("index/component.wasm"),
            "backend_api": { "minimum": "0.1.0", "maximum_exclusive": "0.2.0" },
            "ir": { "minimum": "0.12.0", "maximum_exclusive": "0.13.0" },
        });
        change(&mut entry);
        let index = json!({ "backends": [entry] });
        fs::write(self.dir.join("index/backends.json"), index.to_string()).expect("index");
    }

    fn flick(&self, store: &str, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(self.dir.join("app"))
            .env("FLICK_BACKENDS_INDEX", self.dir.join("index/backends.json"))
            .env("FLICK_STORE", self.dir.join(store))
            .env("NO_COLOR", "1")
            .output()
            .expect("flick runs")
    }

    fn lock(&self) -> String {
        fs::read_to_string(self.dir.join("app/flick.lock")).unwrap_or_default()
    }
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_fresh_project_pins_the_backend_once_and_then_builds_offline() {
    let s = Scratch::new("fresh");
    s.index(|_| {});
    let out = s.flick("store", &["build"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let sha = hex(&component());
    assert!(
        s.lock().contains(&format!(
            "[[backend]]\nid = \"flick/js\"\nversion = \"0.1.0\"\nsha256 = \"{sha}\""
        )),
        "{}",
        s.lock()
    );
    assert!(s.dir.join(format!("store/backends/{sha}.wasm")).is_file());

    // the index is gone and the pin names a release asset, as a published
    // one does: the lock and the store are enough offline
    fs::remove_file(s.dir.join("index/backends.json")).expect("no index");
    let url = s.dir.join("index/component.wasm");
    let lock = s.lock().replace(
        &format!("{:?}", url.display().to_string()),
        "\"https://example.invalid/component.wasm\"",
    );
    assert!(lock.contains("example.invalid"), "{lock}");
    fs::write(s.dir.join("app/flick.lock"), lock).expect("lock");
    let out = s.flick("store", &["build", "--offline"]);
    assert!(out.status.success(), "{}", stderr(&out));

    // an empty store offline has nothing to build with
    let out = s.flick("empty-store", &["build", "--offline"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("FLICK-P056"), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("is not in the package store"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_bad_digest_an_incompatible_release_or_no_entry_fails_before_generation() {
    type Change = fn(&mut serde_json::Value);
    let cases: [(&str, Change, &str); 3] = [
        (
            "digest",
            |e| e["sha256"] = json!("0".repeat(64)),
            "has sha256",
        ),
        (
            "api",
            |e| e["backend_api"]["minimum"] = json!("0.2.0"),
            "lists no release of `flick/js`",
        ),
        (
            "entry",
            |e| e["id"] = json!("flick/other"),
            "lists no release of `flick/js`",
        ),
    ];
    for (name, change, says) in cases {
        let s = Scratch::new(name);
        s.index(|e| {
            e["backend_api"]["maximum_exclusive"] = json!("0.3.0");
            change(e);
        });
        let out = s.flick("store", &["build"]);
        let err = stderr(&out);
        assert!(!out.status.success(), "{name}");
        assert!(
            err.contains("FLICK-P056") && err.contains(says),
            "{name}: {err}"
        );
        assert!(
            !s.dir.join("app/dist").exists(),
            "{name}: nothing is generated"
        );
        assert!(!s.lock().contains("[[backend]]"), "{name}: {}", s.lock());
    }
}

#[test]
fn install_pins_the_backend_locked_refuses_without_one_and_update_moves_it() {
    let s = Scratch::new("install");
    s.index(|_| {});
    let out = s.flick("store", &["install", "--locked"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("`flick/js` is not in `flick.lock`, and `--locked`"),
        "{}",
        stderr(&out)
    );

    let out = s.flick("store", &["install"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(s.lock().contains("version = \"0.1.0\""), "{}", s.lock());
    let out = s.flick("store", &["install", "--locked"]);
    assert!(out.status.success(), "{}", stderr(&out));

    // a newer release in the index: install keeps the pin, update moves it
    s.index(|e| e["version"] = json!("0.1.1"));
    assert!(s.flick("store", &["install"]).status.success());
    assert!(s.lock().contains("version = \"0.1.0\""), "{}", s.lock());
    let out = s.flick("store", &["update"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(s.lock().contains("version = \"0.1.1\""), "{}", s.lock());
}

/// The compiler holds no backend component: a window of each would be in
/// its bytes if it did.
#[test]
fn the_compiler_holds_no_backend_component() {
    let binary = fs::read(env!("CARGO_BIN_EXE_flick")).expect("the binary");
    for name in ["js", "go"] {
        let bytes = fs::read(root().join(format!("backends/{name}/package/component.wasm")))
            .expect("component");
        let middle = bytes.len() / 2;
        let window = &bytes[middle..middle + 256];
        assert!(
            !binary.windows(window.len()).any(|w| w == window),
            "the binary holds the {name} component"
        );
    }
}
