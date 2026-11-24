//! End-to-end tests for direct Go package bindings, the Go value ABI table
//! (`flick.go-ffi@0.1.0`), and `[target.go.dependencies]` (`go.mod` / `go.sum`).
//!
//! Ignored by default because building and executing Go binaries needs `go` on `PATH`;
//! runs in `just e2e-go` and `just e2e-tests`.

mod common;
#[path = "go_direct_ffi/fixtures.rs"]
mod fixtures;
#[path = "go_direct_ffi/proxy.rs"]
mod proxy;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{APP, flick_in, project};
use fixtures::{ABI_GO_SRC, MAIN_DIRECT_FL, V2_GO_SRC};
use flick_core::build::go::executable;

fn flick_go(dir: &Path, proxy_dir: &Path, gomodcache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env_remove("NO_COLOR")
        .env("GOPROXY", proxy::file_url(proxy_dir))
        .env("GONOSUMDB", "*")
        .env("GOSUMDB", "off")
        .env("GOMODCACHE", gomodcache)
        .output()
        .expect("flick runs")
}

fn setup_direct_abi_fixture(base: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let proxy_dir = base.join("proxy");
    let gomodcache = base.join("gomodcache");
    proxy::remove_readonly_tree(&gomodcache);
    fs::create_dir_all(&gomodcache).expect("gomodcache");

    proxy::write_module(
        &proxy_dir,
        "example.com/flicktest/abi",
        "v1.0.0",
        &[("abi.go", ABI_GO_SRC)],
    );
    proxy::write_module(
        &proxy_dir,
        "example.com/flicktest/triple/v2",
        "v2.0.0",
        &[("triple.go", V2_GO_SRC)],
    );

    let lib_dir = base.join("helper");
    fs::create_dir_all(lib_dir.join("src")).expect("lib src");
    fs::write(
        lib_dir.join("flick.toml"),
        "schema-version = 1\n\
         [project]\n\
         kind = \"lib\"\n\
         [package]\n\
         name = \"helper\"\n\
         version = \"0.1.0\"\n\
         [package.abi]\n\
         \"flick.go-ffi\" = \"^0.1.0\"\n\
         [target.go.dependencies]\n\
         \"example.com/flicktest/triple/v2\" = \"v2.0.0\"\n",
    )
    .expect("lib toml");
    fs::write(
        lib_dir.join("src/Helper.fl"),
        "module Helper exposing (triple)\n\
         triple : Int -> Int\n\
         foreign pure triple from \"example.com/flicktest/triple/v2\" \"Triple\"\n",
    )
    .expect("helper fl");

    let app_dir = base.join("app");
    fs::create_dir_all(app_dir.join("src/App")).expect("app src");
    fs::write(
        app_dir.join("flick.toml"),
        format!(
            "{APP}\n\
             [build]\n\
             target = \"go\"\n\n\
             [dependencies]\n\
             helper = {{ path = \"../helper\" }}\n\n\
             [target.go.dependencies]\n\
             \"example.com/flicktest/abi\" = \"v1.0.0\"\n"
        ),
    )
    .expect("app toml");
    fs::write(app_dir.join("src/App/Main.fl"), MAIN_DIRECT_FL).expect("main fl");
    fs::write(app_dir.join("sample.txt"), "hello-from-disk").expect("sample.txt");
    (proxy_dir, gomodcache, app_dir)
}

#[test]
#[ignore = "needs Go"]
fn direct_go_bindings_cover_full_abi_table_cancellation_and_go_mod_sum() {
    let base = project("go_direct_abi_full", &[]);
    let (proxy_dir, gomodcache, app_dir) = setup_direct_abi_fixture(&base);

    let build = flick_go(&app_dir, &proxy_dir, &gomodcache, &["build"]);
    assert!(
        build.status.success(),
        "build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr),
    );

    // Generated go.mod and go.sum contain both root and library dependencies.
    let gen_mod =
        fs::read_to_string(app_dir.join("dist/go/main/go.mod")).expect("generated go.mod");
    assert!(
        gen_mod.contains("example.com/flicktest/abi v1.0.0"),
        "{gen_mod}"
    );
    assert!(
        gen_mod.contains("example.com/flicktest/triple/v2 v2.0.0"),
        "{gen_mod}"
    );
    let gen_sum =
        fs::read_to_string(app_dir.join("dist/go/main/go.sum")).expect("generated go.sum");
    assert!(
        gen_sum.contains("example.com/flicktest/abi v1.0.0 h1:"),
        "{gen_sum}"
    );
    assert!(
        gen_sum.contains("example.com/flicktest/triple/v2 v2.0.0 h1:"),
        "{gen_sum}"
    );

    let run = Command::new(app_dir.join("dist").join(executable("main")))
        .current_dir(&app_dir)
        .output()
        .expect("run compiled Go binary");
    assert!(run.status.success(), "{run:?}");
    let out = String::from_utf8(run.stdout).expect("utf-8 stdout");
    let expected = [
        "hypot=5",
        "upper=FLICK-GO",
        "cut=ok",
        "env=none",
        "read_ok=hello-from-disk",
        "read_err=caught-missing",
        "int=42",
        "bool=true,false",
        "unit=2,unit-boom",
        "bytes=hello|orig-bytes",
        "list=[1, 2, 3]|[10, 20, 30]",
        "nested_list=[[1, 2], [3, 4]]|[[2, 4, 3], [6, 8, 7]]",
        "variadic=42",
        "opt_int_ptr=None|Some 1|Some 42",
        "opt_bool_ptr=None|Some true|Some false",
        "opt_ok=None|Some 42",
        "nested_opt=None|Some(None)|Some(Some(42))",
        "opaque=7|none|42",
        "json_opt=none|\"json-ok\"|None|Some(None)|Some(Some(\"json-ok\"))",
        "async=completed|err:negative-delay|timed-out|1",
        "v2_lib=42",
    ]
    .join("\n");
    assert_eq!(out.trim_end(), expected, "unexpected stdout:\n{out}");

    // Supplying project root go.sum carries checksums into the build cleanly.
    fs::write(app_dir.join("go.sum"), &gen_sum).expect("write project go.sum");
    let clean_rebuild = flick_go(
        &app_dir,
        &proxy_dir,
        &gomodcache,
        &["build", "--out-dir", "dist2"],
    );
    assert!(
        clean_rebuild.status.success(),
        "rebuild with project go.sum failed: {}",
        String::from_utf8_lossy(&clean_rebuild.stderr)
    );
    let gen_sum_2 = fs::read_to_string(app_dir.join("dist2/go/main/go.sum")).expect("dist2 go.sum");
    assert!(
        gen_sum_2.contains("example.com/flicktest/abi v1.0.0 h1:"),
        "{gen_sum_2}"
    );
    proxy::remove_readonly_tree(&gomodcache);
}

#[test]
#[ignore = "needs Go"]
fn out_of_table_go_types_fail_with_f013_and_legacy_sibling_go_keeps_working() {
    for (name, source, expected_reason, line_no) in [
        (
            "go_direct_f013_record",
            "module App.Main exposing (main)\n\
             bad : { x : Int } -> Int\n\
             foreign pure bad from \"example.com/pkg\" \"Bad\"\n\
             main : Unit -> Int\n\
             let main () = bad { x = 1 }\n",
            "structural records have no stable Go struct layout",
            3,
        ),
        (
            "go_direct_f013_untagged_union",
            "module App.Main exposing (main)\n\
             untagged type Key = ByNum Float | ByName String\n\
             bad : Key -> String\n\
             foreign pure bad from \"example.com/pkg\" \"Bad\"\n\
             main : Unit -> String\n\
             let main () = bad (ByName \"k\")\n",
            "user-defined ADTs have no stable Go layout",
            4,
        ),
    ] {
        let bad_dir = project(
            name,
            &[
                ("flick.toml", &format!("{APP}\n[build]\ntarget = \"go\"\n")),
                ("src/App/Main.fl", source),
            ],
        );
        let check = flick_in(&bad_dir, &["check"]);
        assert!(!check.status.success(), "{name}: expected F013 failure");
        let stderr = String::from_utf8_lossy(&check.stderr);
        assert!(stderr.contains("FLICK-F013"), "{name}: {stderr}");
        assert!(stderr.contains(expected_reason), "{name}: {stderr}");
        assert!(
            stderr.contains(&format!("App/Main.fl:{line_no}:")),
            "{name}: {stderr}"
        );
    }

    // Legacy sibling `.go` binding keeps working with `flickrt.Value` for records.
    let legacy_dir = project(
        "go_legacy_sibling",
        &[
            ("flick.toml", &format!("{APP}\n[build]\ntarget = \"go\"\n")),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\
                 read_x : { x : Int } -> Int\n\
                 foreign pure read_x from \"./legacy.mjs\" \"ReadX\"\n\
                 main : Unit ->{Host.Console; AppError} Unit\n\
                 let main () = Platform.stdout_line (Int.to_string (read_x { x = 77 }))\n",
            ),
            (
                "src/App/legacy.mjs",
                "export function ReadX(r) { return r.x; }\n",
            ),
            (
                "src/App/legacy.go",
                "package main\nimport \"flick.app/flickrt\"\n\
                 func ReadX(_ *flickrt.Fiber, r flickrt.Value) flickrt.Value {\n\
                   return flickrt.Field(r, \"x\")\n\
                 }\n",
            ),
        ],
    );
    let run = flick_in(&legacy_dir, &["run"]);
    assert!(
        run.status.success(),
        "legacy sibling .go run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout), "77\n");
}

#[test]
#[ignore = "needs Go"]
fn unimported_go_dep_in_string_literal_builds_without_go_sum_failure() {
    let dir = project(
        "go_unused_dep_literal",
        &[
            (
                "flick.toml",
                &format!(
                    "{APP}\n[build]\ntarget = \"go\"\n\
                     [target.go.dependencies]\n\
                     \"example.com/flicktest/unused\" = \"v1.0.0\"\n"
                ),
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\
                 main : Unit ->{Host.Console; AppError} Unit\n\
                 let main () = Platform.stdout_line \"example.com/flicktest/unused\"\n",
            ),
        ],
    );
    let run = flick_in(&dir, &["run"]);
    assert!(
        run.status.success(),
        "build/run failed when string literal mentioned unimported Go dep:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        "example.com/flicktest/unused\n"
    );
}

#[test]
#[ignore = "needs Go"]
fn target_go_dependencies_reject_invalid_versions_and_conflicts() {
    let bad_ver = project(
        "go_deps_bad_ver",
        &[
            (
                "flick.toml",
                &format!(
                    "{APP}\n[build]\ntarget = \"go\"\n\
                     [target.go.dependencies]\n\
                     \"example.com/mod\" = \"1.0.0\"\n"
                ),
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let out = flick_in(&bad_ver, &["install"]);
    assert!(!out.status.success(), "expected P032 for missing v prefix");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-P032"), "{stderr}");

    let conflict = project(
        "go_deps_conflict",
        &[
            (
                "flick.toml",
                &format!(
                    "{APP}\n[build]\ntarget = \"go\"\n\
                     [target.go.dependencies]\n\
                     \"example.com/mod\" = \"v1.2.0\"\n"
                ),
            ),
            (
                "go.mod",
                "module app\n\ngo 1.23\n\nrequire example.com/mod v1.1.0\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let out = flick_in(&conflict, &["install"]);
    assert!(!out.status.success(), "expected P040 for go.mod conflict");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-P040"), "{stderr}");
}
