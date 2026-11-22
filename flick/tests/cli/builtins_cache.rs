//! The saved interfaces of the built-in modules follow the sources of
//! `std`: another `std` is checked again, not read from the cache.

use crate::common;

#[path = "builtins_cache/backend.rs"]
mod backend;
#[path = "builtins_cache/go.rs"]
mod go;

use std::path::{Path, PathBuf};
use std::process::{self, Command, Output};
use std::{env, fs};

use common::project;
use flick_core::implicit_packages::IMPLICIT_PACKAGES;

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("dir");
    for entry in fs::read_dir(from).expect("dir") {
        let path = entry.expect("entry").path();
        let target = to.join(path.file_name().expect("name"));
        if path.is_dir() {
            copy(&path, &target);
        } else {
            fs::copy(&path, &target).expect("copy");
        }
    }
}

/// A complete override: the first implicit package and all its siblings.
fn copy_packages(root: &Path) -> PathBuf {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages");
    for name in IMPLICIT_PACKAGES {
        copy(&packages.join(name), &root.join(name));
    }
    root.join(IMPLICIT_PACKAGES[0])
}

fn check(dir: &Path, std: &Path, cache: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["check", "src/App/Main.fl"])
        .current_dir(dir)
        .env("FLICK_STD_PATH", std)
        .env("XDG_CACHE_HOME", cache)
        .output()
        .expect("flick runs")
}

#[test]
fn a_changed_std_is_checked_again() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("builtins_cache");
    let _ = fs::remove_dir_all(&root);
    let std = copy_packages(&root);
    let cache = root.join("cache");
    let main = "module App.Main exposing (x)\n\nx : String\nlet x = Markdown.shout \"hi\"\n";
    let app = project(
        "builtins_cache/app",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
            ),
            ("src/App/Main.fl", main),
        ],
    );
    let before = check(&app, &std, &cache);
    assert!(
        !before.status.success(),
        "`Markdown.shout` does not exist yet"
    );
    let markdown = std.join("src/Markdown.fl");
    let text = fs::read_to_string(&markdown).expect("Markdown.fl");
    let text = text.replacen("exposing (", "exposing (shout, ", 1)
        + "\nshout : String -> String\nlet shout s = s ++ \"!\"\n";
    fs::write(&markdown, text).expect("write");
    let after = check(&app, &std, &cache);
    assert!(after.status.success(), "{after:?}");
    // Go's binding tables decide a build for go as the modules do
    let keyed = key(&app, &std, &cache);
    let table = std.join("src/go/globals.txt");
    let text = fs::read_to_string(&table).expect("a table");
    fs::write(&table, format!("{text}\n# a change\n")).expect("write");
    assert_ne!(key(&app, &std, &cache), keyed);
}

#[test]
fn moved_sources_and_go_tables_invalidate_cached_interfaces_and_build_inputs() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("builtins_cache_moved");
    let _ = fs::remove_dir_all(&root);
    let std = copy_packages(&root);
    let cache = root.join("cache");
    let app = project(
        "builtins_cache_moved/app",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (x)\nx : String\nlet x = Http.Method.cache_probe \"hi\"\n",
            ),
        ],
    );
    let missing = check(&app, &std, &cache);
    assert!(
        !missing.status.success(),
        "the new export does not exist yet"
    );
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("cache_probe"),
        "{missing:?}"
    );

    let method = root.join("http/src/Http/Method.fl");
    let text = fs::read_to_string(&method).expect("the moved HTTP module");
    fs::write(
        &method,
        text + "\ncache_probe : String -> String\nlet cache_probe text = text ++ \"!\"\n",
    )
    .expect("change moved source");
    let changed = check(&app, &std, &cache);
    assert!(changed.status.success(), "{changed:?}");
    let warmed = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["check", "src/App/Main.fl", "--timings"])
        .current_dir(&app)
        .env("FLICK_STD_PATH", &std)
        .env("XDG_CACHE_HOME", &cache)
        .output()
        .expect("flick runs");
    assert!(warmed.status.success(), "{warmed:?}");
    assert!(
        String::from_utf8_lossy(&warmed.stderr)
            .contains("built-in interfaces read from their cache"),
        "unchanged moved inputs reuse the saved interfaces: {warmed:?}"
    );
    let unchanged = key(&app, &std, &cache);
    assert_eq!(key(&app, &std, &cache), unchanged);
    let table = root.join("http/src/go/http.txt");
    let text = fs::read_to_string(&table).expect("the moved Go table");
    fs::write(&table, format!("{text}\n# a changed HTTP binding input\n")).expect("change table");
    let changed_table = key(&app, &std, &cache);
    assert_ne!(changed_table, unchanged);

    let manifest = root.join("http/flick.toml");
    let text = fs::read_to_string(&manifest).expect("the moved package manifest");
    fs::write(&manifest, format!("{text}\n# presentation only\n")).expect("comment");
    assert_eq!(key(&app, &std, &cache), changed_table);
    let dependency =
        format!("{text}\n[target.js.dependencies]\n\"@acme/cache-probe\" = \"1.0.0\"\n");
    fs::write(&manifest, &dependency).expect("declare a new dependency");
    let changed_manifest = key(&app, &std, &cache);
    assert_ne!(changed_manifest, changed_table);
    assert_eq!(key(&app, &std, &cache), changed_manifest);
    fs::write(&manifest, dependency.replace("1.0.0", "1.0.1")).expect("change dependency pin");
    assert_ne!(key(&app, &std, &cache), changed_manifest);
}

/// A project's build key.
fn key(dir: &Path, std: &Path, cache: &Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["build", "src/App/Main.fl", "--inputs"])
        .current_dir(dir)
        .env("FLICK_STD_PATH", std)
        .env("XDG_CACHE_HOME", cache)
        .output()
        .expect("flick runs");
    assert!(out.status.success(), "{out:?}");
    let inputs: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    inputs["key"].as_str().expect("a key").to_owned()
}

#[test]
fn a_missing_moved_go_binding_family_is_reported_before_building() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("builtins_missing_http_go");
    let _ = fs::remove_dir_all(&root);
    let std = copy_packages(&root);
    let table = root.join("http/src/go/http.txt");
    let text = fs::read_to_string(&table).expect("HTTP bindings");
    let (before, family) = text
        .split_once("[flick:http/Headers]\n")
        .expect("Headers family");
    let after = family.find("\n[").map_or("", |at| &family[at..]);
    fs::write(&table, format!("{before}{after}")).expect("remove the family");
    let app = project(
        "builtins_missing_http_go/app",
        &[
            ("flick.toml", common::APP),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (Http.Headers.has \"x\" Http.Headers.empty))\n",
            ),
        ],
    );
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["build", "--target", "go", "--offline"])
        .current_dir(&app)
        .env("FLICK_STD_PATH", &std)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .output()
        .expect("flick runs");
    let diagnostics = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{diagnostics}");
    assert!(
        diagnostics.contains("FLICK-P012") && diagnostics.contains("Http.Headers.empty"),
        "{diagnostics}"
    );
    assert!(!app.join("dist/go/main/go.mod").exists());
}

#[test]
fn lsp_in_source_checkout_uses_local_std_without_cargo_env() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("checkout_std_lsp_cache");
    for dir in [repo.to_path_buf(), repo.join("examples/first-slice")] {
        let output = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["lsp", "--stdio"])
            .current_dir(&dir)
            .env_remove("FLICK_STD_PATH")
            .env_remove("RUST_LOG")
            .env("XDG_CACHE_HOME", &cache)
            .output()
            .expect("flick lsp runs");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("language server started"),
            "{dir:?}: {stderr}"
        );
        assert!(!stderr.contains("panicked"), "{dir:?}: {stderr}");
        assert!(
            output.stdout.is_empty(),
            "protocol stdout stays empty on EOF"
        );
    }
}

#[test]
fn a_std_that_does_not_check_is_a_diagnostic() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("builtins_cache_unfit");
    let _ = fs::remove_dir_all(&root);
    let std = copy_packages(&root);
    // a module of another compiler's `std`, as far as this one sees it
    let markdown = std.join("src/Markdown.fl");
    let text = fs::read_to_string(&markdown).expect("Markdown.fl");
    fs::write(&markdown, text + "\nbroken : Int\nlet broken = \"text\"\n").expect("write");
    let app = project(
        "builtins_cache_unfit/app",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (x)\n\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let out = check(&app, &std, &root.join("cache"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("FLICK-P043"), "{stderr}");
    assert!(stderr.contains("`Markdown` of `std`"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

/// Outside its checkout, a compiler built from source takes the `std` of that
/// checkout, not the release tag its version names, so a new project checks
/// without `flick install` and without the network.
#[test]
fn a_compiler_built_from_source_uses_its_std_outside_the_checkout() {
    let root = env::temp_dir().join(format!("flick-built-std-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("dir");
    let flick = |dir: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(dir)
            .env_remove("FLICK_STD_PATH")
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("FLICK_STORE", root.join("store"))
            .output()
            .expect("flick runs")
    };
    let created = flick(&root, &["new", "app"]);
    assert!(created.status.success(), "{created:?}");
    let checked = flick(&root.join("app"), &["check"]);
    let _ = fs::remove_dir_all(&root);
    assert!(checked.status.success(), "{checked:?}");
}

/// The lowered built-in modules are kept beside their interfaces: the first
/// build of a compiler lowers them, a later one reads them, and a file that
/// is no program of this IR is lowered again and replaced.
#[test]
fn a_build_lowers_the_built_in_modules_once() {
    let app = project(
        "builtins_cache_modules",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let cache = app.join("cache");
    let std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std");
    // each build misses the build cache, so each emits the built-in modules
    let build = || {
        let _ = fs::remove_dir_all(app.join(".flick"));
        let out = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["build", "--timings", "--quiet"])
            .current_dir(&app)
            .env("FLICK_STD_PATH", &std)
            .env("XDG_CACHE_HOME", &cache)
            .output()
            .expect("flick runs");
        assert!(out.status.success(), "{out:?}");
        let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
        (
            stderr.contains("built-in modules lowered"),
            stderr.contains("built-in modules read from their cache"),
        )
    };
    assert_eq!(build(), (true, false), "the first build lowers them");
    assert_eq!(build(), (false, true), "the second reads them");
    let kept: Vec<_> = fs::read_dir(cache.join("flick"))
        .expect("cache")
        .map(|e| e.expect("entry").path())
        .filter(|path| path.to_string_lossy().ends_with(".ir.cbor"))
        .collect();
    assert!(!kept.is_empty(), "the lowered modules are a file");
    for path in &kept {
        fs::write(path, b"no program").expect("write");
    }
    assert_eq!(
        build(),
        (true, false),
        "what does not read is lowered again"
    );
    assert_eq!(build(), (false, true), "and replaced");
}

/// A changed `std` is lowered again, as it is checked again: what a build
/// emits of a built-in module follows the sources the build is given.
#[test]
fn a_changed_std_is_lowered_again() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("builtins_cache_lowered");
    let _ = fs::remove_dir_all(&root);
    let std = copy_packages(&root);
    let cache = root.join("cache");
    let manifest = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";
    let main = |call: &str| format!("module App.Main exposing (x)\n\nx : String\nlet x = {call}\n");
    let app = project(
        "builtins_cache_lowered/app",
        &[
            ("flick.toml", manifest),
            ("src/App/Main.fl", &main("Markdown.escape \"<\"")),
        ],
    );
    // each build misses the build cache, so each emits the built-in modules
    let build = || {
        let _ = fs::remove_dir_all(app.join(".flick"));
        let out = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["build", "--timings", "--quiet"])
            .current_dir(&app)
            .env("FLICK_STD_PATH", &std)
            .env("XDG_CACHE_HOME", &cache)
            .output()
            .expect("flick runs");
        assert!(out.status.success(), "{out:?}");
        let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
        stderr.contains("built-in modules lowered")
    };
    assert!(build(), "the first build lowers them");
    assert!(!build(), "the second reads them");
    let emitted = app.join("dist/_flick/Markdown.js");
    let before = fs::read_to_string(&emitted).expect("the built-in module");
    assert!(!before.contains("shout"), "{before}");

    let markdown = std.join("src/Markdown.fl");
    let text = fs::read_to_string(&markdown).expect("Markdown.fl");
    let text = text.replacen("exposing (", "exposing (shout, ", 1)
        + "\nshout : String -> String\nlet shout s = s ++ \"!\"\n";
    fs::write(&markdown, text).expect("write");
    fs::write(app.join("src/App/Main.fl"), main("Markdown.shout \"hi\"")).expect("write");
    assert!(build(), "another std is lowered, not read from the cache");
    let after = fs::read_to_string(&emitted).expect("the built-in module");
    assert!(after.contains("shout"), "{after}");
}

/// Saved interfaces cut short at the end of a line still read line by
/// line: the file is taken only with every built-in module in it.
#[test]
fn a_cache_cut_short_is_checked_again() {
    let app = project(
        "builtins_cache_cut",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (x)\n\nx : String\nlet x = Markdown.escape \"<\"\n",
            ),
        ],
    );
    let cache = app.join("cache");
    let std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std");
    let out = check(&app, &std, &cache);
    assert!(out.status.success(), "{out:?}");
    let saved = fs::read_dir(cache.join("flick"))
        .expect("cache")
        .map(|e| e.expect("entry").path())
        .find(|path| path.extension().is_some_and(|ext| ext == "jsonl"))
        .expect("the saved interfaces");
    let whole = fs::read(&saved).expect("read");
    for cut in [
        0,
        whole
            .iter()
            .position(|byte| *byte == b'\n')
            .expect("a line")
            + 1,
    ] {
        fs::write(&saved, &whole[..cut]).expect("write");
        let out = check(&app, &std, &cache);
        assert!(out.status.success(), "{out:?}");
        assert_eq!(fs::read(&saved).expect("read"), whole, "saved anew");
    }
}
