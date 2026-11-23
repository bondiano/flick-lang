//! AC #2: JS backend optional `pack` capability contracts, collision isolation,
//! and real `pnpm pack` tarball content parity.

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::process::Command;

use super::tar_entries;
#[cfg(unix)]
use super::{stderr, stdout};
#[cfg(unix)]
use crate::common::write_script;
use crate::common::{flick_in, project};

/// AC #2 (fast non-ignored JS pack verification via fake pnpm):
/// verifies `[package]` source-only JS apps without `package.json`, `[tools]`
/// grant enforcement (`P051`), `typescript = true` refusal (`P020`), missing
/// `exports` (`P020`), and `packageManager` pin mismatch (`P016`).
#[cfg(unix)]
#[test]
fn js_app_pack_enforces_contracts_and_isolates_tarball_collisions() {
    let dir = project(
        "pack_js_contracts",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[package]\nname = \"demo\"\nversion = \"1.0.0\"\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
        ],
    );

    // 1. JS app with `[package]` and NO `package.json` writes only the Flick source tarball without pnpm.
    let empty_bin = dir.join("empty-bin");
    fs::create_dir_all(&empty_bin).expect("empty");
    let source_only = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["pack", "--out-dir", "out_source_only"])
        .current_dir(&dir)
        .env("PATH", &empty_bin)
        .output()
        .expect("flick pack");
    assert_eq!(source_only.status.code(), Some(0), "{source_only:?}");
    assert!(dir.join("out_source_only/demo-1.0.0.tgz").is_file());

    // Fake pnpm that verifies `--dir .flick/pack pack --pack-destination out` and writes a tarball into `out/`.
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("bin");
    write_script(
        &bin.join("pnpm"),
        "#!/bin/sh
if [ \"$1\" = --version ]; then echo 9.9.9; exit 0; fi
if [ \"$1\" = --dir ] && [ \"$3\" = pack ] && [ \"$4\" = --pack-destination ]; then
  printf 'fake-npm-tarball' > \"$2/$5/demo-1.0.0.tgz\"
  exit 0
fi
exit 1
",
    );
    let run_with_fake_pnpm = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(&dir)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .expect("flick pack")
    };

    // 2. Adding `package.json` without `[tools] "flick/js" = ["pnpm"]` reports FLICK-P051.
    fs::write(
        dir.join("package.json"),
        r#"{"name":"demo","version":"1.0.0","packageManager":"pnpm@9.9.9"}"#,
    )
    .expect("manifest");
    let no_grant = run_with_fake_pnpm(&["pack", "--out-dir", "out_no_grant"]);
    assert_eq!(no_grant.status.code(), Some(1), "{no_grant:?}");
    assert!(
        stderr(&no_grant).contains("FLICK-P051"),
        "{}",
        stderr(&no_grant)
    );
    assert!(!dir.join("out_no_grant").exists());

    // 3. Grant pnpm, but set `[target.js] typescript = true` -> FLICK-P020.
    let toml_with_tools = "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[package]\nname = \"demo\"\nversion = \"1.0.0\"\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n";
    fs::write(
        dir.join("flick.toml"),
        format!("{toml_with_tools}\n[target.js]\ntypescript = true\n"),
    )
    .expect("ts toml");
    let ts_err = run_with_fake_pnpm(&["pack", "--out-dir", "out_ts"]);
    assert_eq!(ts_err.status.code(), Some(1), "{ts_err:?}");
    assert!(
        stderr(&ts_err).contains("FLICK-P020"),
        "{}",
        stderr(&ts_err)
    );
    assert!(!dir.join("out_ts").exists());

    // 4. Restore JS output; test missing explicit `exports` -> FLICK-P020.
    fs::write(dir.join("flick.toml"), toml_with_tools).expect("js toml");
    fs::write(
        dir.join("package.json"),
        r#"{"name":"demo","version":"1.0.0","exports":{".":"./dist/Missing.js"}}"#,
    )
    .expect("bad exports");
    let bad_exp = run_with_fake_pnpm(&["pack", "--out-dir", "out_exp"]);
    assert_eq!(bad_exp.status.code(), Some(1), "{bad_exp:?}");
    assert!(
        stderr(&bad_exp).contains("FLICK-P020"),
        "{}",
        stderr(&bad_exp)
    );
    assert!(!dir.join("out_exp").exists());

    // 5. Wrong `packageManager` pin -> FLICK-P016 (exit code 2).
    fs::write(
        dir.join("package.json"),
        r#"{"name":"demo","version":"1.0.0","packageManager":"pnpm@99.0.0"}"#,
    )
    .expect("wrong pin");
    let bad_pin = run_with_fake_pnpm(&["pack", "--out-dir", "out_pin"]);
    assert_eq!(bad_pin.status.code(), Some(2), "{bad_pin:?}");
    assert!(
        stderr(&bad_pin).contains("FLICK-P016"),
        "{}",
        stderr(&bad_pin)
    );
    assert!(!dir.join("out_pin").exists());
}

/// AC #2: When a JS app declares both `[package]` and `package.json` with the
/// same name/version, the Flick source tarball stays at `out/<name>-<ver>.tgz`
/// (matching stdout) and the JS npm tarball is isolated at `out/js/<name>-<ver>.tgz`.
#[cfg(unix)]
#[test]
fn js_app_pack_isolates_colliding_npm_tarball_and_stages_manifest() {
    let dir = project(
        "pack_js_collision",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[package]\nname = \"demo\"\nversion = \"1.0.0\"\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            (
                "package.json",
                r#"{"name":"demo","version":"1.0.0","packageManager":"pnpm@9.9.9","devDependencies":{"x":"1.0.0"},"scripts":{"build":"echo"}}"#,
            ),
        ],
    );
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).expect("bin");
    write_script(
        &bin.join("pnpm"),
        "#!/bin/sh
if [ \"$1\" = --version ]; then echo 9.9.9; exit 0; fi
if [ \"$1\" = --dir ] && [ \"$3\" = pack ] && [ \"$4\" = --pack-destination ]; then
  printf 'fake-npm-tarball' > \"$2/$5/demo-1.0.0.tgz\"
  exit 0
fi
exit 1
",
    );
    let both = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["pack", "--out-dir", "out_both"])
        .current_dir(&dir)
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .output()
        .expect("flick pack");
    assert_eq!(both.status.code(), Some(0), "{both:?}");
    let line = stdout(&both);
    let (_, source_path) = line.trim().split_once("  ").expect("sha and path");
    let source_entries = tar_entries(Path::new(source_path));
    assert!(
        source_entries.contains(&"package/flick.toml".to_owned())
            && source_entries.contains(&"package/src/App/Main.fl".to_owned()),
        "{source_entries:?}"
    );
    assert_eq!(
        fs::read(dir.join("out_both/js/demo-1.0.0.tgz")).expect("isolated npm tarball"),
        b"fake-npm-tarball"
    );
    let staged: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join(".flick/pack/package.json")).expect("staged manifest"),
    )
    .expect("json");
    assert_eq!(staged["type"], "module");
    assert_eq!(staged["files"], serde_json::json!(["dist"]));
    assert_eq!(staged["bin"]["demo"], "./dist/flick-main.js");
    assert!(staged.get("devDependencies").is_none());
    assert!(staged.get("scripts").is_none());
}

/// AC #2 (real pnpm): In a JS project, the npm tarball produced via the JS
/// backend's optional `pack` capability has the exact same contents as today.
#[test]
#[ignore = "needs Node and pnpm"]
fn js_app_pack_real_pnpm_tarball_has_expected_contents() {
    let dir = project(
        "pack_js_real_pnpm",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[target.js]\ndeclarations = true\n",
            ),
            (
                "package.json",
                "{\n  \"name\": \"@acme/real-app\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": { \"typescript\": \"5.0.0\" },\n  \"scripts\": { \"test\": \"flick test\" }\n}\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main, value)\n\nvalue : Int\nlet value = 42\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show value)\n",
            ),
        ],
    );
    let out = flick_in(&dir, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let tgz = dir.join("artifacts/acme-real-app-1.0.0.tgz");
    assert!(tgz.is_file(), "expected npm tarball at {}", tgz.display());
    let entries = tar_entries(&tgz);
    for expected in [
        "package/package.json",
        "package/dist/flick-main.js",
        "package/dist/App/Main.js",
        "package/dist/App/Main.iface",
        "package/dist/App/Main.d.ts",
    ] {
        assert!(
            entries.iter().any(|e| e == expected),
            "missing `{expected}` in {entries:?}"
        );
    }
    assert!(
        !entries.iter().any(|e| e.starts_with("package/out/")),
        "staging out/ leaked into npm tarball: {entries:?}"
    );
}
