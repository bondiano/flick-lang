//! Backend-delegated dependency verbs, cross-target isolation, and span
//! diagnostics for target package dependencies.

use super::*;

#[test]
fn legacy_js_dependencies_table_is_rejected_with_p057_migration_hint() {
    let dir = project(
        "package_js_legacy_table",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[js.dependencies]\nleft-pad = \"^1.3.0\"\n",
            ),
            ("src/Main.fl", MAIN),
        ],
    );
    let out = flick_in(&dir, &["check"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let text = stderr(&out);
    assert!(text.contains("FLICK-P057"), "{text}");
    assert!(text.contains("[target.js.dependencies]"), "{text}");
}

#[test]
fn js_dependency_verbs_require_pnpm_grant_in_tools() {
    let dir = project(
        "package_js_missing_tool_grant",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"Main\"\n",
            ),
            ("src/Main.fl", MAIN),
            ("package.json", "{ \"name\": \"ungranted\" }\n"),
        ],
    );
    let out = flick_in(&dir, &["add", "left-pad"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let text = stderr(&out);
    assert!(text.contains("FLICK-P051"), "{text}");
    assert!(text.contains("\"flick/js\" = [\"pnpm\"]"), "{text}");
}

#[test]
fn go_project_add_without_source_reports_p063_with_source_options() {
    let dir = project(
        "package_go_add_p063",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/go\" = [\"go\"]\n\n[project]\nkind = \"app\"\nentry = \"Main\"\n\n[build]\ntarget = \"go\"\n",
            ),
            ("src/Main.fl", MAIN),
        ],
    );
    let out = flick_in(&dir, &["add", "some-pkg"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    let text = stderr(&out);
    assert!(text.contains("FLICK-P063"), "{text}");
    assert!(text.contains("flick/go"), "{text}");
    assert!(
        text.contains("--path") && text.contains("--git") && text.contains("--url"),
        "{text}"
    );

    let why = flick_in(&dir, &["deps", "--why", "some-pkg"]);
    assert_eq!(why.status.code(), Some(2), "{why:?}");
    assert!(stderr(&why).contains("FLICK-P063"), "{}", stderr(&why));
}

#[cfg(unix)]
#[test]
fn go_project_install_with_flick_dependency_works_without_node_or_pnpm() {
    let dir = with_library("package_go_install_no_node", "go");
    let app = dir.join("app");
    fs::write(
        dir.join("lib/flick.toml"),
        format!("{LIB}\n[target.js.dependencies]\nleft-pad = \"^1.3.0\"\n"),
    )
    .expect("write");
    let empty_bin = dir.join("empty-bin");
    fs::create_dir_all(&empty_bin).expect("dir");
    let install = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["install", "--offline"])
        .current_dir(&app)
        .env("PATH", &empty_bin)
        .output()
        .expect("flick install");
    assert!(install.status.success(), "{install:?}");
    assert!(!app.join("package.json").exists());
    assert!(!app.join("node_modules").exists());
}

#[cfg(unix)]
#[test]
#[ignore = "needs Go"]
fn go_project_with_flick_libraries_builds_and_tests_without_node_or_pnpm() {
    use std::os::unix::fs::symlink;

    let go_path = Command::new("which")
        .arg("go")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
        .expect("go on PATH");
    let dir = with_library("package_go_no_node_build_test", "go");
    let app = dir.join("app");
    fs::write(
        dir.join("lib/flick.toml"),
        format!("{LIB}\n[target.js.dependencies]\nleft-pad = \"^1.3.0\"\n"),
    )
    .expect("write");
    fs::write(
        dir.join("lib/src/Lib/Math.fl"),
        "module Lib.Math exposing (double)\n\ndouble : Int -> Int\nlet double n = n * 2\n",
    )
    .expect("lib math");
    fs::create_dir_all(app.join("test")).expect("test dir");
    fs::write(
        app.join("test/MathTest.fl"),
        "module MathTest exposing (tests)\n\nimport Lib.Math\n\ntests : List Test.Test\nlet tests = [Test.test \"doubles\" (fun () -> Test.equal 42 (Lib.Math.double 21))]\n",
    )
    .expect("test");

    // Build an isolated PATH with `go` and essential unix utilities, explicitly excluding `node` and `pnpm`.
    let isolated_bin = dir.join("isolated-bin");
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

    for args in [&["install", "--offline"][..], &["build"][..], &["test"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(args)
            .current_dir(&app)
            .env("PATH", &isolated_bin)
            .output()
            .expect("flick runs");
        assert!(out.status.success(), "{args:?} failed: {out:?}");
    }
    assert!(!app.join("package.json").exists());
    assert!(!app.join("node_modules").exists());
}

#[test]
fn js_project_compiles_npm_source_package_via_backend_reported_roots() {
    let dir = project(
        "package_js_npm_source_root",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
            ),
            (
                "package.json",
                r#"{"name":"app","dependencies":{"@acme/npm-lib":"1.0.0"}}"#,
            ),
            (
                "node_modules/@acme/npm-lib/package.json",
                r#"{"name":"@acme/npm-lib","version":"1.0.0","flick":{"source-dir":"src"}}"#,
            ),
            (
                "node_modules/@acme/npm-lib/src/NpmLib.fl",
                "module NpmLib exposing (answer)\n\nanswer : Float\nlet answer = 7\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nimport NpmLib\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show NpmLib.answer)\n",
            ),
        ],
    );
    let build = flick_in(&dir, &["build"]);
    assert!(build.status.success(), "{build:?}");
    let lib_js = fs::read_to_string(dir.join("dist/NpmLib.js")).expect("compiled npm source");
    assert!(lib_js.contains("answer"), "{lib_js}");
    let main_js = fs::read_to_string(dir.join("dist/App/Main.js")).expect("compiled main");
    assert!(main_js.contains("NpmLib.js"), "{main_js}");
}

#[cfg(unix)]
#[test]
fn workspace_install_handles_mixed_js_and_go_members() {
    let dir = project(
        "package_workspace_mixed_install",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[workspace]\nmembers = [\"lib\", \"go_app\", \"js_app\"]\n",
            ),
            (
                "lib/flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[project]\nkind = \"lib\"\n\n[target.js.dependencies]\nleft-pad = \"^1.3.0\"\n",
            ),
            (
                "lib/src/Lib/Core.fl",
                "module Lib.Core exposing (x)\n\nx : Float\nlet x = 1\n",
            ),
            (
                "lib/package.json",
                "{\n  \"name\": \"lib\",\n  \"packageManager\": \"pnpm@9.9.9\"\n}\n",
            ),
            (
                "go_app/flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/go\" = [\"go\"]\n\n[project]\nkind = \"app\"\nentry = \"Main\"\n\n[build]\ntarget = \"go\"\n\n[dependencies]\nlib = { path = \"../lib\" }\n",
            ),
            ("go_app/src/Main.fl", MAIN),
            (
                "js_app/flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[project]\nkind = \"app\"\nentry = \"Main\"\n\n[dependencies]\nlib = { path = \"../lib\" }\n",
            ),
            ("js_app/src/Main.fl", MAIN),
            (
                "js_app/package.json",
                "{\n  \"name\": \"js-app\",\n  \"packageManager\": \"pnpm@9.9.9\"\n}\n",
            ),
        ],
    );
    let bin = fake_pnpm(&dir);
    let out = with_pnpm(&bin, &dir, &["install"]);
    assert!(out.status.success(), "{out:?}");
    let js_pkg = fs::read_to_string(dir.join("js_app/package.json")).expect("js manifest");
    assert!(js_pkg.contains("\"left-pad\": \"^1.3.0\""), "{js_pkg}");
    assert!(!dir.join("go_app/package.json").exists());
}

#[test]
fn core_host_and_project_do_not_read_package_json_or_node_modules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo");
    let mut files = Vec::new();
    for dir in [root.join("flick/src"), root.join("flick-core/src/project")] {
        let mut stack = vec![dir];
        while let Some(at) = stack.pop() {
            for entry in fs::read_dir(&at).expect("dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
    }
    for file in files {
        let rel = file.strip_prefix(root).expect("rel").to_string_lossy();
        // `dependencies.rs` and `dependencies/graph.rs` are `#[cfg(test)]`-only parity fixtures
        if rel.starts_with("flick/src/backends/dependencies")
            || rel.ends_with("_tests.rs")
            || rel.ends_with("/tests.rs")
        {
            continue;
        }
        let text = fs::read_to_string(&file).expect("read");
        // Ignore `#[cfg(test)]` tail sections
        let prod = text.split("#[cfg(test)]").next().unwrap_or(&text);
        for (idx, line) in prod.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            assert!(
                !trimmed.contains("package.json"),
                "{rel}:{} reads or names `package.json`: {line}",
                idx + 1
            );
            // Skipping `node_modules` while walking user `.fl` trees is allowed; reading into it is not.
            assert!(
                !trimmed.contains("node_modules/") && !trimmed.contains(".join(\"node_modules\")"),
                "{rel}:{} reads `node_modules`: {line}",
                idx + 1
            );
        }
    }
}

#[test]
fn install_and_update_report_target_dependency_diagnostics_without_panicking() {
    // 1. No `[dependencies]` libraries: invalid spec (`P032`) and manifest conflict (`P040`)
    //    must render cleanly against `flick.toml` instead of panicking on an empty SourceMap.
    let invalid_no_libs = project(
        "package_js_install_invalid_no_libs",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[project]\nkind = \"lib\"\n\n[target.js.dependencies]\nleft-pad = \"file:../secret\"\n",
            ),
            (
                "src/Lib.fl",
                "module Lib exposing (x)\n\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    for verb in ["install", "update"] {
        let out = flick_in(&invalid_no_libs, &[verb]);
        assert!(!out.status.success(), "{verb}: {out:?}");
        assert_ne!(out.status.code(), Some(101), "{verb} panicked: {out:?}");
        let err = stderr(&out);
        assert!(
            err.contains("FLICK-P032") && err.contains("flick.toml"),
            "{verb}: {err}"
        );
    }

    let conflict_no_libs = project(
        "package_js_install_conflict_no_libs",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[project]\nkind = \"lib\"\n\n[target.js.dependencies]\nleft-pad = \"^1.3.0\"\n",
            ),
            (
                "package.json",
                "{\"name\":\"app\",\"dependencies\":{\"left-pad\":\"^2.0.0\"}}\n",
            ),
            (
                "src/Lib.fl",
                "module Lib exposing (x)\n\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let out = flick_in(&conflict_no_libs, &["install"]);
    assert!(!out.status.success(), "{out:?}");
    assert_ne!(out.status.code(), Some(101), "install panicked: {out:?}");
    let err = stderr(&out);
    assert!(
        err.contains("FLICK-P040") && err.contains("flick.toml"),
        "{err}"
    );

    // 2. With a path library whose manifest is shorter than the app's `flick.toml`:
    //    `FileId(0)` must stay the app's `flick.toml` (not collide with the library manifest).
    let with_lib = project(
        "package_js_install_invalid_with_lib",
        &[
            (
                "app/flick.toml",
                "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[project]\nkind = \"app\"\nentry = \"Main\"\n\n[dependencies]\nlib = { path = \"../lib\" }\n\n[target.js.dependencies]\nleft-pad = \"file:../secret\"\n",
            ),
            ("app/src/Main.fl", MAIN),
            (
                "lib/flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            (
                "lib/src/Lib.fl",
                "module Lib exposing (x)\n\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let out = flick_in(with_lib.join("app"), &["install"]);
    assert!(!out.status.success(), "{out:?}");
    assert_ne!(out.status.code(), Some(101), "install panicked: {out:?}");
    let err = stderr(&out);
    assert!(
        err.contains("FLICK-P032")
            && err.contains("flick.toml")
            && !err.contains("../lib/flick.toml"),
        "{err}"
    );
}

#[test]
fn schema_export_reports_target_dependency_span_on_flick_toml_not_source_file() {
    let dir = project(
        "package_js_schema_export_span",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[target.js.dependencies]\nleft-pad = \"file:../secret\"\n",
            ),
            (
                "src/App/Types.fl",
                "module App.Types exposing (User, user_json)\n\ntype alias User = { name : String }\n  deriving Schema\n\nuser_json : PureCodec Json User\nlet user_json = Codec.json User.schema\n",
            ),
        ],
    );
    let out = flick_in(
        &dir,
        &[
            "schema",
            "export",
            "--target",
            "json-schema-2020-12",
            "--out",
            "out.json",
            "App.Types.user_json",
        ],
    );
    assert!(!out.status.success(), "{out:?}");
    let err = stderr(&out);
    assert!(
        err.contains("FLICK-P032") && err.contains("flick.toml") && !err.contains("App/Types.fl"),
        "{err}"
    );
}
