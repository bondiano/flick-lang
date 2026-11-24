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
            "version": "0.2.0",
            "sha256": hex(&component()),
            "url": self.dir.join("index/component.wasm"),
            "backend_api": { "minimum": "0.1.0", "maximum_exclusive": "0.2.0" },
            "ir": { "minimum": "0.18.0", "maximum_exclusive": "0.19.0" },
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
            "[[backend]]\nid = \"flick/js\"\nversion = \"0.2.0\"\nsha256 = \"{sha}\""
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
    assert!(s.lock().contains("version = \"0.2.0\""), "{}", s.lock());
    let out = s.flick("store", &["install", "--locked"]);
    assert!(out.status.success(), "{}", stderr(&out));

    // a newer release in the index: install keeps the pin, update moves it
    s.index(|e| e["version"] = json!("0.1.1"));
    assert!(s.flick("store", &["install"]).status.success());
    assert!(s.lock().contains("version = \"0.2.0\""), "{}", s.lock());
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

#[test]
fn fresh_project_and_standalone_accept_backend_options_via_index_and_cached_descriptor() {
    let s = Scratch::new("options_via_index");
    s.index(|_| {});
    let sha = hex(&component());

    // A bogus option on a fresh project fails with FLICK-P058 and does not mutate flick.lock.
    let bogus = s.flick("store", &["build", "--bogus"]);
    assert_eq!(bogus.status.code(), Some(2), "{}", stderr(&bogus));
    assert!(stderr(&bogus).contains("FLICK-P058"), "{}", stderr(&bogus));
    assert!(!s.dir.join("app/flick.lock").exists());

    // A fresh project without flick.lock resolves flick/js from the index and accepts --typescript.
    let ts_project = s.flick("store-fresh", &["build", "--typescript"]);
    assert!(ts_project.status.success(), "{}", stderr(&ts_project));
    assert!(s.dir.join("app/dist/Lib.ts").is_file());
    assert!(!s.dir.join("app/dist/Lib.js").exists());
    assert!(
        s.dir
            .join(format!("store-fresh/descriptors/{sha}.cbor"))
            .is_file()
    );

    // Empty store + --offline with --typescript fails with FLICK-P056, not a misleading FLICK-P058.
    let s_offline = Scratch::new("options_offline_empty");
    s_offline.index(|_| {});
    let offline = s_offline.flick("empty-store", &["build", "--typescript", "--offline"]);
    assert!(!offline.status.success());
    assert!(
        stderr(&offline).contains("FLICK-P056"),
        "{}",
        stderr(&offline)
    );

    // Standalone file (no flick.toml) on a fresh empty store accepts --typescript,
    // and once cached, --help and subsequent --typescript builds reuse the cached descriptor.
    let standalone = s.dir.join("standalone");
    fs::create_dir_all(&standalone).expect("standalone dir");
    fs::write(standalone.join("Main.fl"), MODULE).expect("standalone module");
    let run_standalone = |store: &str, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(&standalone)
            .env("FLICK_BACKENDS_INDEX", s.dir.join("index/backends.json"))
            .env("FLICK_STORE", s.dir.join(store))
            .env("NO_COLOR", "1")
            .output()
            .expect("flick runs")
    };

    // First build with no flags populates store/descriptors/<sha>.cbor.
    let first = run_standalone(
        "standalone-store",
        &["build", "Main.fl", "--out-dir", "out-js"],
    );
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(standalone.join("out-js/Lib.js").is_file());
    assert!(
        s.dir
            .join(format!("standalone-store/descriptors/{sha}.cbor"))
            .is_file()
    );

    // Subsequent standalone --typescript succeeds using the cached descriptor.
    let second = run_standalone(
        "standalone-store",
        &["build", "Main.fl", "--typescript", "--out-dir", "out-ts"],
    );
    assert!(second.status.success(), "{}", stderr(&second));
    assert!(standalone.join("out-ts/Lib.ts").is_file());

    // Standalone --typescript on a brand-new empty store fetches the descriptor and succeeds.
    let cold = run_standalone(
        "standalone-cold",
        &["build", "Main.fl", "--typescript", "--out-dir", "out-cold"],
    );
    assert!(cold.status.success(), "{}", stderr(&cold));
    assert!(standalone.join("out-cold/Lib.ts").is_file());

    // Even after removing the index, `--help` reads `<store>/descriptors/*.cbor` with zero network.
    fs::remove_file(s.dir.join("index/backends.json")).expect("remove index");
    let help = run_standalone("standalone-store", &["build", "--help"]);
    assert!(help.status.success(), "{}", stderr(&help));
    let help_stdout = String::from_utf8_lossy(&help.stdout);
    assert!(
        help_stdout.contains("Options of `flick/js`:") && help_stdout.contains("--typescript"),
        "{help_stdout}"
    );
}

#[test]
fn non_backend_commands_never_fetch_or_load_backends() {
    let s = Scratch::new("no_backend_load");
    fs::write(
        s.dir.join("app/flick.toml"),
        format!("{LIB}\n[scripts]\nhello = \"printf 'from-script'\"\n"),
    )
    .expect("config with script");

    let unreachable = "http://127.0.0.1:1/backends.json";
    let store = s.dir.join("empty-store");
    let run_unreachable = |dir: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(dir)
            .env("FLICK_BACKENDS_INDEX", unreachable)
            .env("FLICK_STORE", &store)
            .env("NO_COLOR", "1")
            .output()
            .expect("flick runs")
    };

    // Every non-`output` `inspect` representation succeeds without touching the store or network.
    for emit in ["iface", "ast", "typed-ir", "variance", "mapping"] {
        let out = run_unreachable(&s.dir.join("app"), &["inspect", "Lib", "--emit", emit]);
        assert!(out.status.success(), "{emit}: {}", stderr(&out));
    }

    // `inspect --emit iface` with a backend option rejects it with FLICK-P058 without loading any backend.
    let bad_iface = run_unreachable(
        &s.dir.join("app"),
        &["inspect", "Lib", "--emit", "iface", "--typescript"],
    );
    assert_eq!(bad_iface.status.code(), Some(2), "{}", stderr(&bad_iface));
    assert!(
        stderr(&bad_iface).contains("FLICK-P058"),
        "{}",
        stderr(&bad_iface)
    );

    // `build --inputs` succeeds without loading or fetching any backend.
    let inputs = run_unreachable(&s.dir.join("app"), &["build", "--inputs"]);
    assert!(inputs.status.success(), "{}", stderr(&inputs));
    let inputs_stdout = String::from_utf8_lossy(&inputs.stdout);
    assert!(inputs_stdout.contains("\"modules\""), "{inputs_stdout}");

    // `run <script>` succeeds without loading or fetching any backend.
    let script = run_unreachable(&s.dir.join("app"), &["run", "hello"]);
    assert!(script.status.success(), "{}", stderr(&script));
    assert_eq!(String::from_utf8_lossy(&script.stdout), "from-script");

    assert!(
        !store.join("descriptors").exists(),
        "descriptors dir must remain empty"
    );
    assert!(
        !store.join("backends").exists(),
        "backends dir must remain empty"
    );

    // Even with a real, valid, uncached `[build.backend]` component on disk,
    // `inspect --emit iface` never compiles or caches its descriptor.
    let pinned_dir = s.dir.join("pinned");
    fs::create_dir_all(pinned_dir.join("src")).expect("pinned project");
    let wasm = component();
    let sha = hex(&wasm);
    fs::write(pinned_dir.join("custom.wasm"), &wasm).expect("copy wasm");
    fs::write(
        pinned_dir.join("flick.toml"),
        format!(
            "{LIB}\n[build.backend]\nid = \"flick/js\"\nversion = \"0.2.0\"\nsha256 = \"{sha}\"\ncomponent = \"custom.wasm\"\n"
        ),
    )
    .expect("pinned config");
    fs::write(pinned_dir.join("src/Lib.fl"), MODULE).expect("pinned module");

    let pinned_iface = run_unreachable(&pinned_dir, &["inspect", "Lib", "--emit", "iface"]);
    assert!(pinned_iface.status.success(), "{}", stderr(&pinned_iface));
    assert!(String::from_utf8_lossy(&pinned_iface.stdout).contains("Lib"));
    assert!(
        !store.join("descriptors").exists(),
        "inspect --emit iface must not load or cache [build.backend]"
    );
    assert!(
        !store.join("backends").exists(),
        "inspect --emit iface must not touch store/backends"
    );
}

#[test]
fn new_from_backends_index_pins_backend_in_lock_and_builds_offline() {
    let s = Scratch::new("new_pins_backend");
    s.index(|_| {});
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["new", "fresh-lib", "--template", "lib"])
        .current_dir(&s.dir)
        .env("FLICK_BACKENDS_INDEX", s.dir.join("index/backends.json"))
        .env("FLICK_STORE", s.dir.join("store"))
        .env("NO_COLOR", "1")
        .output()
        .expect("flick new runs");
    assert!(out.status.success(), "{}", stderr(&out));

    let lib = s.dir.join("fresh-lib");
    let sha = hex(&component());
    let lock = fs::read_to_string(lib.join("flick.lock")).expect("flick.lock written");
    assert!(
        lock.contains(&format!(
            "[[backend]]\nid = \"flick/js\"\nversion = \"0.2.0\"\nsha256 = \"{sha}\""
        )),
        "{lock}"
    );
    assert!(s.dir.join(format!("store/backends/{sha}.wasm")).is_file());

    // Remove the index and point the lock URL to an invalid host: offline build uses the pin and store.
    fs::remove_file(s.dir.join("index/backends.json")).expect("remove index");
    let url = s.dir.join("index/component.wasm");
    let offline_lock = lock.replace(
        &format!("{:?}", url.display().to_string()),
        "\"https://example.invalid/component.wasm\"",
    );
    fs::write(lib.join("flick.lock"), offline_lock).expect("lock");
    let built = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["build", "--offline"])
        .current_dir(&lib)
        .env("FLICK_BACKENDS_INDEX", s.dir.join("index/backends.json"))
        .env("FLICK_STORE", s.dir.join("store"))
        .env("NO_COLOR", "1")
        .output()
        .expect("flick build runs");
    assert!(built.status.success(), "{}", stderr(&built));
}
