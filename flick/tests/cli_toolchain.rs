//! Installed runtime and adapter versions guard the build and its cache.

mod common;

use std::path::Path;
use std::process::Output;
use std::{env, fs};

use common::{APP, flick_in, project};

const MAIN: &str = "module App.Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"hi\"\n";
const EFFECT_APP: &str = concat!(
    "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
    "[build]\neffect = \"effect\"\n"
);

fn code(out: &Output) -> Option<i32> {
    out.status.code()
}
fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}
fn cached(dir: &Path) -> usize {
    fs::read_dir(dir.join(".flick/cache")).map_or(0, Iterator::count)
}

#[test]
fn an_incompatible_runtime_blocks_the_build() {
    let dir = project(
        "cli_runtime",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/@flick-lang/runtime/package.json",
                "{ \"name\": \"@flick-lang/runtime\", \"version\": \"0.0.9\" }",
            ),
        ],
    );
    let out = flick_in(&dir, &["build"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    assert!(stderr(&out).contains("FLICK-P015"));
    assert!(!dir.join("dist").exists());
}

#[test]
fn an_incompatible_effect_blocks_build_and_run() {
    let dir = project(
        "cli_effect_version",
        &[
            ("flick.toml", EFFECT_APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/@flick-lang/runtime/package.json",
                "{ \"name\": \"@flick-lang/runtime\", \"version\": \"0.1.0\" }",
            ),
            (
                "node_modules/@flick-lang/effect-ts/package.json",
                r#"{"version":"0.1.0","dependencies":{"effect":"4.1.2"}}"#,
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/effect/package.json",
                "{ \"name\": \"effect\", \"version\": \"3.0.0\" }",
            ),
        ],
    );
    for command in ["build", "run"] {
        let out = flick_in(&dir, &[command]);
        assert_eq!(code(&out), Some(2), "{command}: {out:?}");
        assert!(stderr(&out).contains("FLICK-P015"), "{command}: {out:?}");
    }
    assert!(!dir.join("dist").exists());
    assert!(
        fs::read_dir(dir.join(".flick/run")).map_or(0, Iterator::count) == 0,
        "no generation was written"
    );
}

#[test]
fn the_installed_runtime_and_effect_are_in_the_cache_key() {
    let dir = project(
        "cli_cache_runtime",
        &[("flick.toml", EFFECT_APP), ("src/App/Main.fl", MAIN)],
    );
    let build = || {
        let out = flick_in(&dir, &["build"]);
        assert_eq!(code(&out), Some(0), "{out:?}");
    };
    build();
    let runtime = dir.join("node_modules/@flick-lang/runtime");
    let adapter = dir.join("node_modules/@flick-lang/effect-ts");
    fs::create_dir_all(&runtime).expect("dir");
    fs::write(
        runtime.join("package.json"),
        "{ \"name\": \"@flick-lang/runtime\", \"version\": \"0.1.0\" }",
    )
    .expect("write");
    build();
    assert_eq!(cached(&dir), 2, "the runtime version is in the key");
    fs::create_dir_all(adapter.join("node_modules/effect")).expect("dir");
    fs::write(
        adapter.join("package.json"),
        "{ \"name\": \"@flick-lang/effect-ts\", \"version\": \"0.1.0\" }",
    )
    .expect("write");
    fs::write(
        adapter.join("package.json"),
        r#"{"version":"0.1.0","dependencies":{"effect":"4.1.2"}}"#,
    )
    .expect("write");
    fs::write(
        adapter.join("node_modules/effect/package.json"),
        r#"{"version":"4.1.2"}"#,
    )
    .expect("write");
    build();
    assert_eq!(cached(&dir), 3, "the adapter contract is in the key");
    fs::write(
        adapter.join("node_modules/effect/package.json"),
        "{ \"name\": \"effect\", \"version\": \"4.1.2\" }",
    )
    .expect("write");
    build();
    assert_eq!(
        cached(&dir),
        4,
        "dependency manifest contents are in the key"
    );
    build();
    assert_eq!(cached(&dir), 4, "unchanged manifests reuse the cache");
}

#[test]
fn rt_build_does_not_require_or_cache_effect() {
    let dir = project(
        "cli_cache_rt_only",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let build = || {
        let out = flick_in(&dir, &["build"]);
        assert_eq!(code(&out), Some(0), "{out:?}");
    };
    build();
    assert_eq!(cached(&dir), 1);
    let adapter = dir.join("node_modules/@flick-lang/effect-ts");
    fs::create_dir_all(adapter.join("node_modules/effect")).expect("dir");
    fs::write(
        adapter.join("package.json"),
        "{ \"name\": \"@flick-lang/effect-ts\", \"version\": \"0.0.9\" }",
    )
    .expect("write");
    fs::write(
        adapter.join("node_modules/effect/package.json"),
        "{ \"name\": \"effect\", \"version\": \"3.0.0\" }",
    )
    .expect("write");
    build();
    assert_eq!(cached(&dir), 1, "Effect does not affect an RT build");
}

#[test]
fn adapter_owns_its_effect_requirement() {
    let dir = project(
        "cli_package_owned_adapter",
        &[
            ("flick.toml", EFFECT_APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/@flick-lang/effect-ts/package.json",
                r#"{"name":"@flick-lang/effect-ts","version":"0.2.0","dependencies":{"effect":"4.2.0"}}"#,
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/effect/package.json",
                r#"{"version":"4.2.0"}"#,
            ),
        ],
    );
    let out = flick_in(&dir, &["build"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let manifest = dir.join("node_modules/@flick-lang/effect-ts/package.json");
    fs::write(
        &manifest,
        r#"{"name":"@flick-lang/effect-ts","version":"0.2.0","dependencies":{"effect":"4.3.0"}}"#,
    )
    .expect("write manifest");
    let out = flick_in(&dir, &["build"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    assert!(stderr(&out).contains("FLICK-P015"));
    assert!(stderr(&out).contains("4.3.0"));
}

#[test]
fn contract_contents_invalidate_cache_without_a_version_change() {
    let dir = project(
        "cli_adapter_contract_cache",
        &[
            ("flick.toml", EFFECT_APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/@flick-lang/effect-ts/package.json",
                r#"{"version":"0.2.0","dependencies":{"effect":"4.2.0"}}"#,
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/effect/package.json",
                r#"{"version":"4.2.0"}"#,
            ),
        ],
    );
    assert_eq!(code(&flick_in(&dir, &["build"])), Some(0));
    let manifest = dir.join("node_modules/@flick-lang/effect-ts/package.json");
    fs::write(
        &manifest,
        r#"{"version":"0.2.0","dependencies":{"effect":"4.2.0"},"contractRevision":2}"#,
    )
    .expect("write");
    assert_eq!(code(&flick_in(&dir, &["build"])), Some(0));
    assert_eq!(cached(&dir), 2);
    let inputs = flick_in(&dir, &["build", "--inputs"]);
    assert_eq!(code(&inputs), Some(0), "{inputs:?}");
    assert!(String::from_utf8_lossy(&inputs.stdout).contains("effect-ts/package.json"));
    fs::write(manifest, "{bad json").expect("write");
    assert!(stderr(&flick_in(&dir, &["build"])).contains("FLICK-P015"));
}

#[cfg(unix)]
#[test]
fn adapter_resolves_dependencies_beside_the_real_pnpm_package() {
    use std::os::unix::fs::symlink;
    let dir = project(
        "cli_pnpm_adapter",
        &[
            ("flick.toml", EFFECT_APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/.pnpm/adapter/node_modules/@flick-lang/effect-ts/package.json",
                r#"{"version":"0.2.0","dependencies":{"effect":"4.2.0"}}"#,
            ),
            (
                "node_modules/.pnpm/adapter/node_modules/effect/package.json",
                r#"{"version":"4.2.0"}"#,
            ),
            ("node_modules/effect/package.json", r#"{"version":"3.0.0"}"#),
        ],
    );
    fs::create_dir_all(dir.join("node_modules/@flick-lang")).expect("dir");
    symlink(
        "../.pnpm/adapter/node_modules/@flick-lang/effect-ts",
        dir.join("node_modules/@flick-lang/effect-ts"),
    )
    .expect("symlink");
    let out = flick_in(&dir, &["build"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
}

#[cfg(unix)]
#[test]
fn locked_offline_install_checks_the_adapter_without_changing_manifests() {
    use std::process::Command;
    let package = r#"{"name":"locked-adapter","packageManager":"pnpm@12.16.1"}"#;
    let lock = "lockfileVersion: '9.0'\n";
    let dir = project(
        "cli_adapter_locked_offline",
        &[
            ("flick.toml", EFFECT_APP),
            ("package.json", package),
            ("pnpm-lock.yaml", lock),
            (
                "node_modules/@flick-lang/effect-ts/package.json",
                r#"{"version":"0.2.0","dependencies":{"effect":"4.2.0"}}"#,
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/effect/package.json",
                r#"{"version":"4.2.0"}"#,
            ),
        ],
    );
    fs::create_dir_all(dir.join("bin")).expect("dir");
    common::write_script(
        &dir.join("bin/pnpm"),
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 12.16.1; else printf '%s\\n' \"$@\" > pnpm-args; fi\n",
    );
    let mut paths = vec![dir.join("bin")];
    paths.extend(env::split_paths(&env::var_os("PATH").expect("PATH")));
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["install", "--locked", "--offline"])
            .current_dir(&dir)
            .env("PATH", env::join_paths(&paths).expect("paths"))
            .output()
            .expect("install")
    };
    let out = run();
    assert_eq!(code(&out), Some(0), "{out:?}");
    assert_eq!(
        fs::read_to_string(dir.join("pnpm-args")).expect("args"),
        "install\n--frozen-lockfile\n--offline\n"
    );
    fs::write(
        dir.join("node_modules/@flick-lang/effect-ts/node_modules/effect/package.json"),
        r#"{"version":"3.0.0"}"#,
    )
    .expect("write");
    let out = run();
    assert_eq!(code(&out), Some(2), "{out:?}");
    assert!(stderr(&out).contains("FLICK-P015"));
    assert_eq!(
        fs::read_to_string(dir.join("package.json")).expect("manifest"),
        package
    );
    assert_eq!(
        fs::read_to_string(dir.join("pnpm-lock.yaml")).expect("lock"),
        lock
    );
}

#[test]
fn adapter_cannot_select_another_kernel_abi() {
    let dir = project(
        "cli_adapter_kernel_abi",
        &[
            ("flick.toml", EFFECT_APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/@flick-lang/effect-ts/package.json",
                r#"{"version":"0.2.0","dependencies":{"effect":"4.2.0","@flick-lang/runtime":"0.0.9"}}"#,
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/effect/package.json",
                r#"{"version":"4.2.0"}"#,
            ),
            (
                "node_modules/@flick-lang/effect-ts/node_modules/@flick-lang/runtime/package.json",
                r#"{"version":"0.0.9"}"#,
            ),
        ],
    );
    let out = flick_in(&dir, &["build"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    assert!(stderr(&out).contains("FLICK-P015"));
    assert!(!dir.join("dist").exists());
}

#[test]
fn doctor_reports_effect_without_an_adapter_owned_requirement() {
    let dir = project(
        "cli_doctor_effect_only",
        &[
            ("flick.toml", APP),
            ("node_modules/effect/package.json", r#"{"version":"4.2.0"}"#),
        ],
    );
    let out = flick_in(&dir, &["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("note effect: 4.2.0"), "{text}");
    assert!(!text.contains("FAIL effect"), "{text}");
}

#[cfg(unix)]
#[test]
fn externally_linked_adapter_cannot_borrow_project_dependencies() {
    use std::os::unix::fs::symlink;
    use std::process;
    let dir = project(
        "cli_external_adapter",
        &[
            ("flick.toml", EFFECT_APP),
            ("src/App/Main.fl", MAIN),
            ("node_modules/effect/package.json", r#"{"version":"4.2.0"}"#),
        ],
    );
    let external = env::temp_dir().join(format!("flick-external-adapter-{}", process::id()));
    fs::create_dir(&external).expect("fresh external adapter");
    fs::write(
        external.join("package.json"),
        r#"{"version":"0.2.0","dependencies":{"effect":"4.2.0"}}"#,
    )
    .expect("manifest");
    fs::create_dir_all(dir.join("node_modules/@flick-lang")).expect("dir");
    symlink(&external, dir.join("node_modules/@flick-lang/effect-ts")).expect("link");
    let out = flick_in(&dir, &["build"]);
    fs::remove_dir_all(external).expect("cleanup");
    assert_eq!(code(&out), Some(2), "{out:?}");
    assert!(stderr(&out).contains("FLICK-P015"));
    assert!(!dir.join("dist").exists());
}

#[test]
fn adapter_does_not_search_nested_node_modules_directories() {
    let dir = project(
        "cli_nested_node_modules",
        &[
            ("flick.toml", EFFECT_APP),
            ("src/App/Main.fl", MAIN),
            (
                "node_modules/@flick-lang/effect-ts/package.json",
                r#"{"version":"0.2.0","dependencies":{"effect":"4.2.0"}}"#,
            ),
            (
                "node_modules/node_modules/effect/package.json",
                r#"{"version":"4.2.0"}"#,
            ),
        ],
    );
    let out = flick_in(&dir, &["build"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    assert!(stderr(&out).contains("FLICK-P015"));
    assert!(!dir.join("dist").exists());
}
