//! Real third-party backend component with tags, axes, and two `[host]`
//! packages selecting different modules and checking reachable `@when` uses.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use flick_backend::{Backend as _, BackendId, Descriptor, Environment};

use super::common::{flick_in, project};

fn literal_host_config(sha: &str, env: &str, extra: &str) -> String {
    format!(
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App\"\n\n\
         [dependencies]\nhost-alpha = {{ path = \"hosts/alpha\" }}\n\
         host-beta = {{ path = \"hosts/beta\" }}\n\n\
         [build]\nenvironment = \"{env}\"\n\n\
         [build.backend]\nid = \"test/literal\"\nversion = \"1.0.0\"\n\
         sha256 = \"{sha}\"\ncomponent = \"backend.wasm\"\n{extra}"
    )
}

fn find_json_diag(raw: &[u8], code: &str) -> serde_json::Value {
    String::from_utf8_lossy(raw)
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|d| d["code"] == code)
        .unwrap_or_else(|| panic!("expected {code} in {}", String::from_utf8_lossy(raw)))
}

fn diag_span_slice<'a>(diag: &serde_json::Value, source: &'a str) -> &'a str {
    let start =
        usize::try_from(diag["range"]["start"].as_u64().expect("start")).expect("start fits");
    let end = usize::try_from(diag["range"]["end"].as_u64().expect("end")).expect("end fits");
    &source[start..end]
}

fn setup_literal_host_project(app_src: &str) -> (PathBuf, String) {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(&component).expect("literal wasm component");
    let sha = flick_backend::sha256(&bytes);
    let root = project(
        "backend_host_packages_env",
        &[
            (
                "flick.toml",
                &literal_host_config(&sha, "literal/alpha-host", ""),
            ),
            (
                "hosts/alpha/flick.toml",
                "schema-version = 1\n[package]\nname = \"host-alpha\"\nversion = \"0.1.0\"\n\
                 [project]\nkind = \"lib\"\n[host]\nname = \"alpha-host\"\nbackend = \"literal\"\n\
                 requires = \"alpha\"\ntags = [\"console\"]\n",
            ),
            (
                "hosts/alpha/src/Host/Alpha.fl",
                "@when \"literal && console\"\nmodule Host.Alpha exposing (greet)\n\
                 greet : Unit -> Unit\nlet greet () = ()\n",
            ),
            (
                "hosts/beta/flick.toml",
                "schema-version = 1\n[package]\nname = \"host-beta\"\nversion = \"0.1.0\"\n\
                 [project]\nkind = \"lib\"\n[host]\nname = \"beta-host\"\nbackend = \"literal\"\n\
                 requires = \"beta\"\ntags = [\"web\"]\n",
            ),
            (
                "hosts/beta/src/Host/Beta.fl",
                "@when \"literal && web\"\nmodule Host.Beta exposing (greet)\n\
                 greet : Unit -> Unit\nlet greet () = ()\n",
            ),
            ("src/App.fl", app_src),
        ],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("write component");
    (root, sha)
}

#[test]
#[ignore = "requires just wasm-backends"]
fn fixture_backend_with_two_host_packages_selects_modules_and_checks_reachability() {
    let app_alpha = "module App exposing (main)\nimport Host.Alpha\nimport Host.Beta\n\
        main : Unit -> Unit\nlet main () = Host.Alpha.greet ()\n";
    let app_beta = "module App exposing (main)\nimport Host.Alpha\nimport Host.Beta\n\
        main : Unit -> Unit\nlet main () = Host.Beta.greet ()\n";
    let (root, sha) = setup_literal_host_project(app_alpha);

    // 1. `literal/alpha-host`: Host.Alpha is reachable, Host.Beta is unused -> passes.
    assert!(flick_in(&root, &["check"]).status.success());
    assert!(flick_in(&root, &["build"]).status.success());
    let emitted = fs::read_to_string(root.join("dist/_flick/App.js")).expect("App.js");
    assert!(
        emitted.starts_with(
            "// env: literal/alpha-host tags: alpha,alpha-host,console,literal host: host-alpha\n"
        ),
        "{emitted}"
    );

    // 2. `literal/beta-host` without `dialect = "beta"` -> host requires fails with FLICK-P065 (exit 2).
    fs::write(
        root.join("flick.toml"),
        literal_host_config(&sha, "literal/beta-host", ""),
    )
    .expect("beta toml");
    for args in [
        &["check", "--diagnostics", "json"][..],
        &["build", "--diagnostics", "json"][..],
    ] {
        let out = flick_in(&root, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("FLICK-P065") && stderr.contains("requires backend condition `beta`"),
            "{args:?}: {stderr}"
        );
    }

    // 3. `literal/beta-host` with `dialect = "beta"` while App calls Host.Alpha -> FLICK-P064.
    fs::write(
        root.join("flick.toml"),
        literal_host_config(
            &sha,
            "literal/beta-host",
            "\n[target.literal]\ndialect = \"beta\"\n",
        ),
    )
    .expect("beta dialect toml");
    let check_wrong = flick_in(&root, &["check", "--diagnostics", "json"]);
    let build_wrong = flick_in(&root, &["build", "--diagnostics", "json"]);
    assert_eq!(check_wrong.status.code(), Some(1), "{check_wrong:?}");
    assert_eq!(build_wrong.status.code(), Some(1), "{build_wrong:?}");
    let check_diag = find_json_diag(&check_wrong.stderr, "FLICK-P064");
    let build_diag = find_json_diag(&build_wrong.stderr, "FLICK-P064");
    assert_eq!(check_diag["message"], build_diag["message"]);
    assert_eq!(check_diag["range"], build_diag["range"]);
    assert_eq!(diag_span_slice(&check_diag, app_alpha), "Host.Alpha.greet");
    let msg = check_diag["message"].as_str().expect("message");
    assert!(
        msg.contains("Host.Alpha") && msg.contains("App.main") && msg.contains("Host.Alpha.greet"),
        "{msg}"
    );

    // 4. Switch App to call Host.Beta (Host.Alpha left as unused import) -> passes.
    fs::write(root.join("src/App.fl"), app_beta).expect("beta app");
    assert!(flick_in(&root, &["check"]).status.success());
    assert!(flick_in(&root, &["build"]).status.success());
    let emitted = fs::read_to_string(root.join("dist/_flick/App.js")).expect("App.js");
    assert!(
        emitted.starts_with(
            "// env: literal/beta-host tags: beta,beta-host,literal,web host: host-beta\n"
        ),
        "{emitted}"
    );

    // 5. Typo in `@when` tag -> FLICK-P066 at the misspelled tag in both check and build.
    let bad_mod = "@when \"literal && consle\"\nmodule Host.Alpha exposing (greet)\n\
        greet : Unit -> Unit\nlet greet () = ()\n";
    fs::write(root.join("hosts/alpha/src/Host/Alpha.fl"), bad_mod).expect("typo");
    for args in [
        &["check", "--diagnostics", "json"][..],
        &["build", "--diagnostics", "json"][..],
    ] {
        let out = flick_in(&root, args);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        let diag = find_json_diag(&out.stderr, "FLICK-P066");
        assert_eq!(diag_span_slice(&diag, bad_mod), "consle");
    }
}

fn write_cached_descriptor(dir: &Path, name: &str, descriptor: &Descriptor) {
    fs::create_dir_all(dir).expect("descriptors dir");
    let mut bytes = Vec::new();
    ciborium::into_writer(descriptor, &mut bytes).expect("cbor");
    fs::write(dir.join(format!("{name}.cbor")), bytes).expect("write cbor");
}

#[test]
fn global_descriptor_cache_does_not_pollute_check_or_override_official_backends() {
    let base = project("env_descriptor_cache_isolation", &[]);
    let store = base.join("store");
    let desc_dir = store.join("descriptors");

    // 1. Seed a cached third-party descriptor (`test/literal`) and a mutated
    //    `flick/js` descriptor (missing `browser`, adding `stale-cache-tag`).
    let mut third_party = flick_backend_js::Javascript.describe();
    third_party.identity = BackendId {
        id: "test/literal".into(),
        version: "1.0.0".into(),
    };
    third_party.tags = vec!["literal".into()];
    third_party.axes.clear();
    third_party.environments = vec![Environment {
        backend: "literal".into(),
        platform: "alpha-host".into(),
    }];
    write_cached_descriptor(&desc_dir, "aaaa", &third_party);

    let mut mutated_js = flick_backend_js::Javascript.describe();
    mutated_js.tags = vec!["js".into(), "stale-cache-tag".into()];
    mutated_js.environments = vec![Environment {
        backend: "js".into(),
        platform: "node".into(),
    }];
    write_cached_descriptor(&desc_dir, "bbbb", &mutated_js);

    let run_check = |dir: &Path| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["check", "--diagnostics", "json"])
            .current_dir(dir)
            .env("FLICK_STORE", &store)
            .env("NO_COLOR", "1")
            .output()
            .expect("flick check runs")
    };

    // Unpinned third-party tag and mutated cached official tag must both fail with FLICK-P066.
    for (name, tag) in [
        ("third_party_tag", "literal"),
        ("stale_tag", "stale-cache-tag"),
    ] {
        let dir = project(
            &format!("env_cache_{name}"),
            &[
                (
                    "flick.toml",
                    "schema-version = 1\n[project]\nkind = \"lib\"\n",
                ),
                (
                    "src/Lib.fl",
                    &format!("@when \"{tag}\"\nmodule Lib exposing (x)\nx : Int\nlet x = 1\n"),
                ),
            ],
        );
        let out = run_check(&dir);
        assert_eq!(out.status.code(), Some(1), "{name}: {out:?}");
        let diag = find_json_diag(&out.stderr, "FLICK-P066");
        assert!(
            diag["message"].as_str().is_some_and(|m| m.contains(tag)),
            "{diag}"
        );
    }

    // Official `js/browser` must remain intact despite the mutated cached `flick/js` descriptor.
    let browser_dir = project(
        "env_cache_browser_ok",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\nenvironment = \"js/browser\"\n",
            ),
            (
                "src/Lib.fl",
                "@when \"js && browser\"\nmodule Lib exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let ok = run_check(&browser_dir);
    assert!(ok.status.success(), "{ok:?}");

    // Unpinned `literal/alpha-host` must fail with FLICK-P059 even though `test/literal` is cached.
    let unpinned_dir = project(
        "env_cache_unpinned_env",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\nenvironment = \"literal/alpha-host\"\n",
            ),
            (
                "src/Lib.fl",
                "module Lib exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let err = run_check(&unpinned_dir);
    assert_eq!(err.status.code(), Some(2), "{err:?}");
    let _ = find_json_diag(&err.stderr, "FLICK-P059");
}

#[test]
fn own_host_library_without_package_checks_and_builds_offline() {
    let dir = project(
        "own_host_lib_no_package",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n\
                 [host]\nname = \"edge\"\nbackend = \"js\"\ntags = [\"worker\"]\n\
                 [build]\nenvironment = \"js/edge\"\n",
            ),
            (
                "src/Edge.fl",
                "@when \"js && worker\"\nmodule Edge exposing (answer)\n\
                 answer : Int\nlet answer = 42\n",
            ),
        ],
    );
    let checked = flick_in(&dir, &["check"]);
    assert!(checked.status.success(), "check failed: {checked:?}");
    let built = flick_in(&dir, &["build", "--offline"]);
    assert!(built.status.success(), "build failed: {built:?}");
    assert!(dir.join("dist/Edge.js").is_file());
}

#[test]
#[ignore = "requires Go"]
fn target_go_override_on_default_js_project_builds_and_tests_with_non_rt_stages() {
    // Default JS project (no `target = "go"` in flick.toml): `--target go` must
    // compile with `stages_by_class = false` to match Go builtins and request bundling.
    let dir = project(
        "target_go_override_default_js",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App\"\n\
                 [tools]\n\"flick/go\" = [\"go\"]\n",
            ),
            (
                "src/App.fl",
                "module App exposing (main, compute)\n\
                 compute : Int ->{Host.Console} Int\n\
                 let compute n =\n  Platform.stdout_line (show n)\n  n + 1\n\
                 main : Unit ->{Host.Console; AppError} Unit\n\
                 let main () =\n  let _ = compute 41\n  ()\n",
            ),
            (
                "test/Suite.fl",
                "module Suite exposing (tests)\n\
                 import App\n\
                 tests : List Test.Test\n\
                 let tests = [Test.test \"go stage\" (fun () -> Test.equal 42 (21 + 21))]\n",
            ),
        ],
    );
    let built = flick_in(&dir, &["build", "--target", "go", "--offline"]);
    assert!(
        built.status.success(),
        "build --target go failed: {built:?}"
    );
    let tested = flick_in(&dir, &["test", "--target", "go", "--offline"]);
    assert!(
        tested.status.success(),
        "test --target go failed: {tested:?}"
    );
}
