//! AC #3 & AC #4: Go projects packing without Node or pnpm, `FLICK-P024`,
//! `FLICK-P032`, API-diff warnings, stale `.flick/pack/out/` isolation, and
//! native Go round-trip e2e builds.

use std::fs;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use flick_core::build::go::executable;

#[cfg(unix)]
use super::{git, serve_tarball, stderr, stdout, tar_entries};
use crate::common::{flick_in, project};

/// AC #3 & AC #4: In a Go project (lib and app), `flick pack` writes only the
/// deterministic Flick source tarball without Node, pnpm, or `go` on PATH.
#[cfg(unix)]
#[test]
fn go_projects_pack_without_node_or_pnpm_and_enforce_checks() {
    let root = project("pack_go_no_node", &[]);
    let empty_bin = root.join("empty-bin");
    fs::create_dir_all(&empty_bin).expect("empty bin");
    let store = root.join("store");

    let flick_no_path = |dir: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(dir)
            .env("PATH", &empty_bin)
            .env("FLICK_STORE", &store)
            .output()
            .expect("flick runs")
    };

    // 1. Go library with `[target.js.dependencies]` (ignored on Go) packs cleanly with empty PATH.
    let go_lib = project(
        "pack_go_lib",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"golib\"\nversion = \"0.1.0\"\n\n[project]\nkind = \"lib\"\n\n[build]\ntarget = \"go\"\n\n[target.js.dependencies]\nleft-pad = \"^1.3.0\"\n",
            ),
            (
                "src/GoLib/Calc.fl",
                "module GoLib.Calc exposing (inc)\n\ninc : Int -> Int\nlet inc x = x + 1\n",
            ),
            ("LICENSE", "MIT\n"),
        ],
    );
    let out1 = flick_no_path(&go_lib, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(out1.status.code(), Some(0), "{out1:?}");
    let out2 = flick_no_path(&go_lib, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(out2.status.code(), Some(0), "{out2:?}");
    assert_eq!(stdout(&out1), stdout(&out2), "deterministic sha256");
    let (sha, tarball_path) = stdout(&out1)
        .trim()
        .split_once("  ")
        .map(|(s, p)| (s.to_owned(), PathBuf::from(p)))
        .expect("sha and path");
    assert_eq!(
        tarball_path.file_name().and_then(|n| n.to_str()),
        Some("golib-0.1.0.tgz")
    );
    assert_eq!(
        tar_entries(&tarball_path),
        [
            "package/LICENSE",
            "package/flick.toml",
            "package/src/GoLib/Calc.fl",
        ]
    );
    assert!(!go_lib.join("package.json").exists());
    assert!(!go_lib.join("node_modules").exists());

    // Round-trip the Go library tarball into a Go app via `flick add --url --sha256` without Node/pnpm.
    let url = serve_tarball(fs::read(&tarball_path).expect("go lib bytes"));
    let go_app = project(
        "pack_go_app",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"goapp\"\nversion = \"1.0.0\"\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nimport GoLib.Calc\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (GoLib.Calc.inc 41))\n",
            ),
        ],
    );
    let added = flick_no_path(&go_app, &["add", "golib", "--url", &url, "--sha256", &sha]);
    assert_eq!(added.status.code(), Some(0), "{added:?}");
    let checked = flick_no_path(&go_app, &["check"]);
    assert_eq!(checked.status.code(), Some(0), "{checked:?}");

    // 2. Go app with `[package]` packs only its Flick source tarball with empty PATH.
    let packed_app = flick_no_path(&go_app, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(packed_app.status.code(), Some(0), "{packed_app:?}");
    let files: Vec<PathBuf> = fs::read_dir(go_app.join("artifacts"))
        .expect("artifacts")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(files.len(), 1, "only Flick source tarball: {files:?}");
    assert_eq!(
        files[0].file_name().and_then(|n| n.to_str()),
        Some("goapp-1.0.0.tgz")
    );
}

/// AC #3 & AC #4: Go pack enforces `FLICK-P032` (not `FLICK-P063`), `FLICK-P024`,
/// cross-target `--target go`, and unknown `--target` (`FLICK-P059`) without Node/pnpm.
#[cfg(unix)]
#[test]
fn go_pack_enforces_p024_p032_and_target_flags_without_node_or_pnpm() {
    let root = project("pack_go_checks_root", &[]);
    let empty_bin = root.join("empty-bin");
    fs::create_dir_all(&empty_bin).expect("empty bin");
    let flick_no_path = |dir: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(dir)
            .env("PATH", &empty_bin)
            .output()
            .expect("flick runs")
    };

    // 1. Go app without `[package]` fails with FLICK-P032 (not FLICK-P063) and leaves out_dir absent.
    let no_pkg_go_app = project(
        "pack_go_app_no_pkg",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );
    let err_no_pkg = flick_no_path(&no_pkg_go_app, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(err_no_pkg.status.code(), Some(1), "{err_no_pkg:?}");
    let text = stderr(&err_no_pkg);
    assert!(text.contains("FLICK-P032"), "{text}");
    assert!(!text.contains("FLICK-P063"), "{text}");
    assert!(!no_pkg_go_app.join("artifacts").exists());

    // 2. `todo`/`echo` check (`FLICK-P024`) behaves identically on Go without Node/pnpm.
    let go_debug = project(
        "pack_go_debug_left",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"godebug\"\nversion = \"0.1.0\"\n\n[project]\nkind = \"lib\"\n\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/GoLib/Calc.fl",
                "module GoLib.Calc exposing (inc)\n\ninc : Int -> Int\nlet inc x = echo (todo \"inc\")\n",
            ),
        ],
    );
    let debug_left = flick_no_path(&go_debug, &["pack", "--out-dir", "artifacts_bad"]);
    assert_eq!(debug_left.status.code(), Some(1), "{debug_left:?}");
    let text = stderr(&debug_left);
    for code in ["FLICK-T022", "FLICK-T023", "FLICK-P024"] {
        assert!(text.contains(code), "{code} missing from {text}");
    }
    assert!(!go_debug.join("artifacts_bad").exists());

    // 3. Cross-target `--target go` on a default-JS project works without Node/pnpm;
    //    unknown `--target nope` reports FLICK-P059 (exit 2).
    let cross = project(
        "pack_cross_target",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"cross\"\nversion = \"0.1.0\"\n\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/Cross.fl",
                "module Cross exposing (x)\n\nx : Int\nlet x = 7\n",
            ),
        ],
    );
    let cross_go = flick_no_path(
        &cross,
        &["pack", "--target", "go", "--out-dir", "artifacts"],
    );
    assert_eq!(cross_go.status.code(), Some(0), "{cross_go:?}");
    assert!(cross.join("artifacts/cross-0.1.0.tgz").is_file());

    let unknown = flick_no_path(&cross, &["pack", "--target", "nope", "--out-dir", "bad"]);
    assert_eq!(unknown.status.code(), Some(2), "{unknown:?}");
    assert!(
        stderr(&unknown).contains("FLICK-P059"),
        "{}",
        stderr(&unknown)
    );
    assert!(!cross.join("bad").exists());
}

/// AC #3 & AC #4: API-diff breaking-change warning works on a Go project with
/// PATH containing only `git` (no `node`, no `pnpm`, no `go`).
#[cfg(unix)]
#[test]
fn go_pack_emits_api_diff_warning_without_node_or_pnpm() {
    use std::os::unix::fs::symlink;

    let git_path = Command::new("which")
        .arg("git")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        .expect("git on PATH");

    let dir = project(
        "pack_go_api_diff",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"goapi\"\nversion = \"0.2.0\"\n\n[project]\nkind = \"lib\"\n\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/GoApi.fl",
                "module GoApi exposing (greet)\n\ngreet : String -> String\nlet greet s = s\n",
            ),
        ],
    );
    git(&dir, &["init", "--quiet"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "--quiet", "-m", "v0.1.0"]);
    git(&dir, &["tag", "v0.1.0"]);

    // Breaking change: signature changed from `String -> String` to `Int -> Int`.
    fs::write(
        dir.join("src/GoApi.fl"),
        "module GoApi exposing (greet)\n\ngreet : Int -> Int\nlet greet n = n\n",
    )
    .expect("breaking edit");

    let git_only_bin = dir.join("git-only-bin");
    fs::create_dir_all(&git_only_bin).expect("bin");
    symlink(&git_path, git_only_bin.join("git")).expect("symlink git");
    assert!(!git_only_bin.join("node").exists());
    assert!(!git_only_bin.join("pnpm").exists());
    assert!(!git_only_bin.join("go").exists());

    let packed = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["pack", "--out-dir", "artifacts"])
        .current_dir(&dir)
        .env("PATH", &git_only_bin)
        .output()
        .expect("flick pack");
    assert_eq!(packed.status.code(), Some(0), "{packed:?}");
    let err = stderr(&packed);
    assert!(
        err.contains("warning: the public API breaks importers since v0.1.0; `flick diff v0.1.0` lists the changes"),
        "{err}"
    );
    let line = stdout(&packed);
    assert!(line.trim().ends_with("goapi-0.2.0.tgz"), "{line}");
}

/// Regression: stale files left in `.flick/pack/out/` from an earlier interrupted
/// run are ignored when packing for a backend without a `pack` command (such as Go).
#[test]
fn go_pack_ignores_stale_stage_out_artifacts() {
    let dir = project(
        "pack_go_stale_stage",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"gostale\"\nversion = \"0.1.0\"\n\n[project]\nkind = \"lib\"\n\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/GoStale.fl",
                "module GoStale exposing (v)\n\nv : Int\nlet v = 1\n",
            ),
            (".flick/pack/out/stale.tgz", "stale-artifact"),
        ],
    );
    let out = flick_in(&dir, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let names: Vec<String> = fs::read_dir(dir.join("artifacts"))
        .expect("artifacts")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["gostale-0.1.0.tgz"]);
}

/// AC #1 & AC #3 (real Go build): A Go library packed without Node/pnpm installs
/// via `flick add --url --sha256` into a Go app that builds and runs without Node/pnpm.
#[cfg(unix)]
#[test]
#[ignore = "needs Go"]
fn go_packed_tarball_installs_via_add_url_and_builds_native_go_without_node() {
    use std::os::unix::fs::symlink;

    let go_path = Command::new("which")
        .arg("go")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        .expect("go on PATH");

    let root = project("pack_go_e2e_root", &[]);
    let store = root.join("store");
    let isolated_bin = root.join("isolated-bin");
    fs::create_dir_all(&isolated_bin).expect("bin");
    symlink(&go_path, isolated_bin.join("go")).expect("symlink go");
    for util in [
        "sh", "env", "uname", "dirname", "basename", "mkdir", "rm", "cp", "mv", "cat",
    ] {
        for base in ["/bin", "/usr/bin"] {
            let candidate = Path::new(base).join(util);
            if candidate.is_file() {
                let _ = symlink(&candidate, isolated_bin.join(util));
                break;
            }
        }
    }
    assert!(!isolated_bin.join("node").exists());
    assert!(!isolated_bin.join("pnpm").exists());

    let run_isolated = |dir: &Path, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(dir)
            .env("PATH", &isolated_bin)
            .env("FLICK_STORE", &store)
            .output()
            .expect("flick runs")
    };

    let lib = project(
        "pack_go_e2e_lib",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"goutil\"\nversion = \"0.1.0\"\n\n[project]\nkind = \"lib\"\n\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/GoUtil.fl",
                "module GoUtil exposing (answer)\n\nanswer : Int\nlet answer = 99\n",
            ),
        ],
    );
    let packed = run_isolated(&lib, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(packed.status.code(), Some(0), "{packed:?}");
    let line = stdout(&packed);
    let (sha, tgz) = line.trim().split_once("  ").expect("sha and path");
    let url = serve_tarball(fs::read(tgz).expect("tarball bytes"));

    let app = project(
        "pack_go_e2e_app",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/go\" = [\"go\"]\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nimport GoUtil\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show GoUtil.answer)\n",
            ),
        ],
    );
    let added = run_isolated(&app, &["add", "goutil", "--url", &url, "--sha256", sha]);
    assert_eq!(added.status.code(), Some(0), "{added:?}");
    let built = run_isolated(&app, &["build"]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    let exe = app.join("dist").join(executable("main"));
    let ran = Command::new(&exe).output().expect("run go binary");
    assert!(ran.status.success(), "{ran:?}");
    assert_eq!(stdout(&ran).trim(), "99");
}
