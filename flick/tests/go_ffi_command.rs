//! End-to-end tests for `flick go:ffi import` and `flick go:ffi check`
//! against Go stdlib (`strings`, `math`), an offline fixture Go module, and a
//! local `--dir` Go module. Needs `go` on `PATH`: run with `--ignored` (part of
//! `just e2e-go` and `just e2e-tests`).

mod common;
#[path = "go_ffi_command/fixtures.rs"]
mod fixtures;
#[path = "go_direct_ffi/proxy.rs"]
mod proxy;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::project;
use fixtures::{
    FIXTURE_MAIN_FL, FIXTURE_POLICY, FIXTURE_V1_1_GO, FIXTURE_V1_GO, GO_APP, MATH_MAIN_FL,
    STRINGS_MAIN_FL, assert_fixture_v1_report, provenance_of, reviewed_fixture_source, text,
};
use flick_core::build::go::executable;

fn flick(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .output()
        .expect("flick runs")
}

fn flick_with_proxy(dir: &Path, proxy_dir: &Path, gomodcache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .env("GOPROXY", proxy::file_url(proxy_dir))
        .env("GONOSUMDB", "*")
        .env("GOSUMDB", "off")
        .env("GOMODCACHE", gomodcache)
        .output()
        .expect("flick runs")
}

fn go_project(name: &str, extra_toml: &str) -> PathBuf {
    let dir = project(
        name,
        &[(
            "flick.toml",
            &format!("{GO_APP}\n[tools]\n\"flick/go\" = [\"go\"]\n{extra_toml}"),
        )],
    );
    fs::create_dir_all(dir.join("src/App")).expect("app dir");
    fs::create_dir_all(dir.join("src/Ffi")).expect("ffi dir");
    dir
}

#[test]
#[ignore = "needs Go"]
fn import_and_run_stdlib_strings() {
    let dir = go_project("go_ffi_stdlib_strings", "");
    fs::write(
        dir.join("src/App/Main.fl"),
        "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("stub main");

    // Flat `flick ffi import` in a Go project resolves to `flick/go`.
    let flat = flick(&dir, &["ffi", "import", "strings", "--out", "src/Ffi"]);
    assert!(
        flat.status.success(),
        "flat ffi import strings failed:\n{}",
        text(&flat.stderr)
    );
    let generated_path = dir.join("src/Ffi/Strings/Generated.fl");
    let first = fs::read_to_string(&generated_path).expect("Strings/Generated.fl");

    // Qualified `flick go:ffi import` produces the exact same module and report.
    fs::remove_file(&generated_path).expect("remove");
    let qualified = flick(&dir, &["go:ffi", "import", "strings", "--out", "src/Ffi"]);
    assert!(
        qualified.status.success(),
        "go:ffi import strings failed:\n{}",
        text(&qualified.stderr)
    );
    assert_eq!(text(&qualified.stdout), text(&flat.stdout));
    let generated = fs::read_to_string(&generated_path).expect("Strings/Generated.fl");
    assert_eq!(generated, first);

    let report = text(&qualified.stdout);
    // Independently expected supported statuses in `strings`.
    for expected in [
        "supported    to_upper — String ->{} String",
        "supported    cut_prefix — String -> String ->{} Option String",
        "supported    split — String -> String ->{} List String",
        "supported    join — List String -> String ->{} String",
        "supported    new_replacer — List String ->{} Replacer",
        "supported    replacer_replace — Replacer -> String ->{} String",
        "supported    new_reader — String ->{} Reader",
        "supported    reader_reset — Reader -> String ->{} Unit",
        "supported    builder_string — Builder ->{} String",
        "supported    builder_reset — Builder ->{} Unit",
    ] {
        assert!(
            report.contains(expected),
            "missing `{expected}` in:\n{report}"
        );
    }
    // Independently expected unsupported statuses with clear reasons (present in Go 1.23+).
    for (name, reason_fragment) in [
        ("count", "Go `int` is outside `flick.go-ffi`"),
        ("repeat", "Go `int` is outside `flick.go-ffi`"),
        ("builder_write_byte", "Go `byte` is outside `flick.go-ffi`"),
        ("contains_func", "Go function type"),
        ("cut", "returns 3 values"),
    ] {
        assert!(
            report.lines().any(|l| l.contains("unsupported")
                && l.contains(name)
                && l.contains(reason_fragment)),
            "expected unsupported `{name}` with `{reason_fragment}` in:\n{report}"
        );
    }

    assert!(generated.contains("@go from \"strings\" \"*Replacer\"\ntype Replacer\n"));
    assert!(generated.contains(
        "foreign replacer_replace from \"strings\" \"Replacer.Replace\"\n  arity 2\n  this\n"
    ));

    // Generated module checks cleanly with both `flick check` and `flick go:ffi check`.
    let ffi_check = flick(&dir, &["go:ffi", "check"]);
    assert!(ffi_check.status.success(), "{}", text(&ffi_check.stderr));

    // Compile and run a real Flick program using the generated `strings` bindings.
    fs::write(dir.join("src/App/Main.fl"), STRINGS_MAIN_FL).expect("write main");

    let build = flick(&dir, &["build"]);
    assert!(build.status.success(), "{}", text(&build.stderr));
    let run = Command::new(dir.join("dist").join(executable("main")))
        .current_dir(&dir)
        .output()
        .expect("run compiled binary");
    assert!(run.status.success(), "{}", text(&run.stderr));
    assert_eq!(text(&run.stdout), "FLICK-GO|world|a-b-c|bar and qux\n");
}

#[test]
#[ignore = "needs Go"]
fn import_and_run_stdlib_math() {
    let dir = go_project("go_ffi_stdlib_math", "");
    fs::write(
        dir.join("src/App/Main.fl"),
        "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("stub main");

    let out = flick(&dir, &["go:ffi", "import", "math", "--out", "src/Ffi"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let report = text(&out.stdout);

    for expected in [
        "supported    hypot — Float -> Float ->{} Float",
        "supported    abs — Float ->{} Float",
        "supported    sqrt — Float ->{} Float",
        "supported    is_na_n — Float ->{} Bool",
        "supported    na_n — Unit ->{} Float",
        "supported    pi — Float",
    ] {
        assert!(
            report.contains(expected),
            "missing `{expected}` in:\n{report}"
        );
    }
    for (name, reason_fragment) in [
        ("ldexp", "Go `int` is outside `flick.go-ffi`"),
        ("float64bits", "Go `uint64` is outside `flick.go-ffi`"),
        ("sincos", "returns 2 values"),
        ("max_int", "overflows 32-bit `Int`"),
    ] {
        assert!(
            report.lines().any(|l| l.contains("unsupported")
                && l.contains(name)
                && l.contains(reason_fragment)),
            "expected unsupported `{name}` with `{reason_fragment}` in:\n{report}"
        );
    }

    let check = flick(&dir, &["go:ffi", "check"]);
    assert!(check.status.success(), "{}", text(&check.stderr));

    fs::write(dir.join("src/App/Main.fl"), MATH_MAIN_FL).expect("write main");

    let build = flick(&dir, &["build"]);
    assert!(build.status.success(), "{}", text(&build.stderr));
    let run = Command::new(dir.join("dist").join(executable("main")))
        .current_dir(&dir)
        .output()
        .expect("run math binary");
    assert!(run.status.success(), "{}", text(&run.stderr));
    assert_eq!(text(&run.stdout), "5|7.5|9|true|true\n");
}

fn verify_fixture_drift_and_policies(
    app_dir: &Path,
    proxy_dir: &Path,
    gomodcache: &Path,
    toml_v1: &str,
    reviewed_path: &Path,
    generated_path: &Path,
) {
    // Upgrade fixture to v1.1.0 (changed `Divide` signature, removed `CheckPositive`):
    // `go:ffi check` reports F006 on stale Generated.fl and F007 at the reviewed binding.
    proxy::write_module(
        proxy_dir,
        "example.com/flicktest/fix",
        "v1.1.0",
        &[("fix.go", FIXTURE_V1_1_GO)],
    );
    let toml_v1_1 = toml_v1.replace("v1.0.0", "v1.1.0");
    fs::write(app_dir.join("flick.toml"), &toml_v1_1).expect("flick.toml v1.1.0");
    fs::write(
        app_dir.join("src/Ffi/Fix/policy.toml"),
        "[exports.Add]\nmode = \"pure\"\n",
    )
    .expect("policy v1.1.0");

    let drift = flick_with_proxy(app_dir, proxy_dir, gomodcache, &["go:ffi", "check"]);
    assert!(!drift.status.success());
    let drift_err = text(&drift.stderr);
    assert!(drift_err.contains("FLICK-F006"), "{drift_err}");
    assert!(drift_err.contains("FLICK-F007"), "{drift_err}");
    assert!(
        drift_err.contains(
            "the declaration of `Divide` changed since `divide` was reviewed against example.com/flicktest/fix@v1.0.0"
        ),
        "{drift_err}"
    );
    assert!(
        drift_err.contains("no longer exports `CheckPositive`"),
        "{drift_err}"
    );
    assert!(
        drift_err.replace('\\', "/").contains("src/Ffi/Fix.fl:"),
        "expected F007 span at reviewed binding file:\n{drift_err}"
    );

    // Re-importing updates Generated.fl without touching reviewed Fix.fl; F006 clears, F007 remains.
    let reviewed_before = fs::read_to_string(reviewed_path).expect("reviewed");
    let reimport = flick_with_proxy(
        app_dir,
        proxy_dir,
        gomodcache,
        &[
            "go:ffi",
            "import",
            "example.com/flicktest/fix",
            "--out",
            "src/Ffi",
        ],
    );
    assert!(reimport.status.success(), "{}", text(&reimport.stderr));
    assert_eq!(
        fs::read_to_string(reviewed_path).expect("reviewed after"),
        reviewed_before
    );
    let after = flick_with_proxy(app_dir, proxy_dir, gomodcache, &["go:ffi", "check"]);
    assert!(!after.status.success());
    let after_err = text(&after.stderr);
    assert!(!after_err.contains("FLICK-F006"), "{after_err}");
    assert!(after_err.contains("FLICK-F007"), "{after_err}");

    // `--select` requesting a blocked export reports F005 while writing the rest.
    fs::write(app_dir.join("flick.toml"), toml_v1).expect("back to v1.0.0");
    let select = flick_with_proxy(
        app_dir,
        proxy_dir,
        gomodcache,
        &[
            "go:ffi",
            "import",
            "example.com/flicktest/fix",
            "--out",
            "src/Ffi",
            "--select",
            "Add,LogAny",
        ],
    );
    assert!(!select.status.success());
    let sel_err = text(&select.stderr);
    assert!(sel_err.contains("FLICK-F005"), "{sel_err}");
    assert!(sel_err.contains("`LogAny` is blocked"), "{sel_err}");
    let sel_gen = fs::read_to_string(generated_path).expect("selected Generated.fl");
    assert!(sel_gen.contains("add : Int -> Int -> Int"), "{sel_gen}");
    assert!(sel_gen.contains("foreign blocked log_any"), "{sel_gen}");

    // Missing `[tools]` grant for `go` reports FLICK-P051.
    fs::write(app_dir.join("flick.toml"), GO_APP).expect("no tools");
    let denied = flick(app_dir, &["go:ffi", "import", "math", "--out", "src/Ffi"]);
    assert!(!denied.status.success());
    let denied_err = text(&denied.stderr);
    assert!(denied_err.contains("FLICK-P051"), "{denied_err}");
    assert!(denied_err.contains("flick/go"), "{denied_err}");
}

#[test]
#[ignore = "needs Go"]
fn import_run_and_check_drift_on_fixture_module() {
    let base = project("go_ffi_fixture", &[]);
    let proxy_dir = base.join("proxy");
    let gomodcache = base.join("gomodcache");
    proxy::remove_readonly_tree(&gomodcache);
    fs::create_dir_all(&gomodcache).expect("gomodcache");

    proxy::write_module(
        &proxy_dir,
        "example.com/flicktest/fix",
        "v1.0.0",
        &[("fix.go", FIXTURE_V1_GO)],
    );

    let app_dir = base.join("app");
    fs::create_dir_all(app_dir.join("src/App")).expect("app dir");
    fs::create_dir_all(app_dir.join("src/Ffi/Fix")).expect("ffi fix dir");

    let toml_v1 = format!(
        "{GO_APP}\n[tools]\n\"flick/go\" = [\"go\"]\n\n\
         [target.go.dependencies]\n\"example.com/flicktest/fix\" = \"v1.0.0\"\n"
    );
    fs::write(app_dir.join("flick.toml"), &toml_v1).expect("flick.toml");
    fs::write(app_dir.join("src/Ffi/Fix/policy.toml"), FIXTURE_POLICY).expect("policy.toml");
    fs::write(
        app_dir.join("src/App/Main.fl"),
        "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("stub main");

    let imp = flick_with_proxy(
        &app_dir,
        &proxy_dir,
        &gomodcache,
        &[
            "go:ffi",
            "import",
            "example.com/flicktest/fix",
            "--out",
            "src/Ffi",
        ],
    );
    assert!(imp.status.success(), "{}", text(&imp.stderr));
    assert_fixture_v1_report(&text(&imp.stdout));

    let generated_path = app_dir.join("src/Ffi/Fix/Generated.fl");
    let generated = fs::read_to_string(&generated_path).expect("Fix/Generated.fl");
    let divide_prov = provenance_of(&generated, "divide");
    let check_pos_prov = provenance_of(&generated, "check_positive");
    let reviewed_path = app_dir.join("src/Ffi/Fix.fl");
    fs::write(
        &reviewed_path,
        reviewed_fixture_source(&divide_prov, &check_pos_prov, true),
    )
    .expect("Fix.fl");

    let check_ok = flick_with_proxy(&app_dir, &proxy_dir, &gomodcache, &["go:ffi", "check"]);
    assert!(check_ok.status.success(), "{}", text(&check_ok.stderr));

    // Compile and execute the generated + reviewed bindings.
    fs::write(app_dir.join("src/App/Main.fl"), FIXTURE_MAIN_FL).expect("Main.fl");
    let build = flick_with_proxy(&app_dir, &proxy_dir, &gomodcache, &["build"]);
    assert!(build.status.success(), "{}", text(&build.stderr));
    let run = Command::new(app_dir.join("dist").join(executable("main")))
        .current_dir(&app_dir)
        .output()
        .expect("run fixture binary");
    assert!(run.status.success(), "{}", text(&run.stderr));
    assert_eq!(
        text(&run.stdout),
        "52|bytes-ok|10|found,miss|42,none\n5|err:divide by zero|err:non-positive|12,12|hi, flick|echo:async\n"
    );

    // Using a `foreign blocked` export fails `flick check` with FLICK-F005.
    fs::write(
        app_dir.join("src/App/Main.fl"),
        "module App.Main exposing (main)\nimport Ffi.Fix.Generated as G\nmain : Unit -> Unit\nlet main () = let _ = G.generic_id\n  ()\n",
    )
    .expect("blocked main");
    let blocked_check = flick_with_proxy(&app_dir, &proxy_dir, &gomodcache, &["check"]);
    assert!(!blocked_check.status.success());
    assert!(
        text(&blocked_check.stderr).contains("FLICK-F005"),
        "{}",
        text(&blocked_check.stderr)
    );
    fs::write(app_dir.join("src/App/Main.fl"), FIXTURE_MAIN_FL).expect("restore main");

    // Reviewed binding missing required `catch` fails `go:ffi check` with FLICK-F007.
    fs::write(
        &reviewed_path,
        reviewed_fixture_source(&divide_prov, &check_pos_prov, false),
    )
    .expect("missing catch");
    let missing_catch = flick_with_proxy(&app_dir, &proxy_dir, &gomodcache, &["go:ffi", "check"]);
    assert!(!missing_catch.status.success());
    let mc_err = text(&missing_catch.stderr);
    assert!(mc_err.contains("FLICK-F007"), "{mc_err}");
    assert!(mc_err.contains("needs `catch`"), "{mc_err}");
    fs::write(
        &reviewed_path,
        reviewed_fixture_source(&divide_prov, &check_pos_prov, true),
    )
    .expect("restore reviewed");

    verify_fixture_drift_and_policies(
        &app_dir,
        &proxy_dir,
        &gomodcache,
        &toml_v1,
        &reviewed_path,
        &generated_path,
    );
    proxy::remove_readonly_tree(&gomodcache);
}

#[test]
#[ignore = "needs Go"]
fn import_and_check_local_module_via_dir_flag() {
    let dir = go_project("go_ffi_local_dir", "");
    let mod_dir = dir.join("localmod");
    fs::create_dir_all(&mod_dir).expect("localmod dir");
    fs::write(
        mod_dir.join("go.mod"),
        "module example.com/localmod\n\ngo 1.23\n",
    )
    .expect("go.mod");
    fs::write(
        mod_dir.join("local.go"),
        "package localmod\n\nfunc Triple(n int32) int32 { return n * 3 }\n",
    )
    .expect("local.go");
    fs::write(
        dir.join("src/App/Main.fl"),
        "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("stub main");

    let imp = flick(
        &dir,
        &[
            "go:ffi",
            "import",
            "example.com/localmod",
            "--out",
            "src/Ffi",
            "--dir",
            "localmod",
        ],
    );
    assert!(imp.status.success(), "{}", text(&imp.stderr));
    let generated = fs::read_to_string(dir.join("src/Ffi/Localmod/Generated.fl")).expect("gen");
    assert!(
        generated.contains(
            "triple : Int ->{} Int\nforeign triple from \"example.com/localmod\" \"Triple\""
        ),
        "{generated}"
    );

    let check = flick(&dir, &["go:ffi", "check", "--dir", "localmod"]);
    assert!(check.status.success(), "{}", text(&check.stderr));

    // Change the local Go function signature and verify `go:ffi check --dir` catches drift.
    fs::write(
        mod_dir.join("local.go"),
        "package localmod\n\nfunc Triple(n float64) float64 { return n * 3 }\n",
    )
    .expect("changed local.go");
    let drifted = flick(&dir, &["go:ffi", "check", "--dir", "localmod"]);
    assert!(!drifted.status.success());
    assert!(
        text(&drifted.stderr).contains("FLICK-F006"),
        "{}",
        text(&drifted.stderr)
    );

    // `--dir .` (project root) is rejected with FLICK-F008 instead of pointing at temp staging.
    let root_import = flick(
        &dir,
        &[
            "go:ffi",
            "import",
            "example.com/localmod",
            "--out",
            "src/Ffi",
            "--dir",
            ".",
        ],
    );
    assert!(!root_import.status.success());
    let root_import_err = text(&root_import.stderr);
    assert!(root_import_err.contains("FLICK-F008"), "{root_import_err}");
    assert!(
        root_import_err.contains("subdirectory"),
        "{root_import_err}"
    );

    let root_check = flick(&dir, &["go:ffi", "check", "--dir", "."]);
    assert!(!root_check.status.success());
    let root_check_err = text(&root_check.stderr);
    assert!(root_check_err.contains("FLICK-F008"), "{root_check_err}");
}
