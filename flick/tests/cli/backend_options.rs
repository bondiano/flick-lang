//! Backend-declared options, `--target`, `[target.<name>]`, per-verb `--help`,
//! `FLICK-P057` / `FLICK-P058` / `FLICK-P059`, option cache invalidation, and
//! `inspect --emit output` / `inspect --emit iface`.

#[path = "backend_options/inspect_and_cache.rs"]
mod inspect_and_cache;

use std::fs;
use std::process::Output;

use crate::common::{APP, flick, flick_in, project};

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn code(out: &Output) -> Option<i32> {
    out.status.code()
}

const MAIN: &str = "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n";

const LIB: &str = "module Lib.Math exposing (inc)\n\ninc : Int -> Int\nlet inc x = x + 1\n";

#[test]
fn help_shows_only_js_options_for_each_verb() {
    let js_dir = project(
        "backend_opts_help_js",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );

    // JS `build --help` and `help build` match and list only JS build options.
    let js_build = flick_in(&js_dir, &["build", "--help"]);
    assert!(js_build.status.success(), "{js_build:?}");
    let js_build_text = stdout(&js_build);
    assert!(
        js_build_text.contains("Options of `flick/js`:"),
        "{js_build_text}"
    );
    for expected in [
        "--target",
        "--typescript",
        "--declarations",
        "--promise-facade",
        "--schema-aot",
        "--runtime",
        "--ui",
    ] {
        assert!(
            js_build_text.contains(expected),
            "missing {expected} in:\n{js_build_text}"
        );
    }
    for forbidden in ["--devtools", "--cgo", "--emit-dts", "--rt", "backend-opt:"] {
        assert!(
            !js_build_text.contains(forbidden),
            "unexpected {forbidden} in:\n{js_build_text}"
        );
    }
    // `help build` matches `build --help`.
    let js_help_build = flick_in(&js_dir, &["help", "build"]);
    assert!(js_help_build.status.success(), "{js_help_build:?}");
    assert_eq!(stdout(&js_help_build), js_build_text);

    // JS `run --help` and `dev --help` show runtime/devtools/ui, not build-only flags.
    for verb in ["run", "dev"] {
        let out = flick_in(&js_dir, &[verb, "--help"]);
        assert!(out.status.success(), "{verb}: {out:?}");
        let text = stdout(&out);
        assert!(text.contains("Options of `flick/js`:"), "{verb}:\n{text}");
        for expected in ["--target", "--runtime", "--devtools", "--ui"] {
            assert!(
                text.contains(expected),
                "{verb}: missing {expected}\n{text}"
            );
        }
        for forbidden in [
            "--typescript",
            "--declarations",
            "--promise-facade",
            "--schema-aot",
            "--cgo",
        ] {
            assert!(
                !text.contains(forbidden),
                "{verb}: unexpected {forbidden}\n{text}"
            );
        }
    }

    // JS `test --help` shows runtime and ui, not typescript/devtools/cgo.
    let js_test = flick_in(&js_dir, &["test", "--help"]);
    assert!(js_test.status.success(), "{js_test:?}");
    let js_test_text = stdout(&js_test);
    assert!(
        js_test_text.contains("Options of `flick/js`:"),
        "{js_test_text}"
    );
    for expected in ["--target", "--runtime", "--ui"] {
        assert!(
            js_test_text.contains(expected),
            "missing {expected} in:\n{js_test_text}"
        );
    }
    for forbidden in ["--typescript", "--declarations", "--devtools", "--cgo"] {
        assert!(
            !js_test_text.contains(forbidden),
            "unexpected {forbidden} in:\n{js_test_text}"
        );
    }

    // JS `inspect --help` shows typescript and runtime, not declarations/devtools/cgo.
    let js_inspect = flick_in(&js_dir, &["inspect", "--help"]);
    assert!(js_inspect.status.success(), "{js_inspect:?}");
    let js_inspect_text = stdout(&js_inspect);
    assert!(
        js_inspect_text.contains("Options of `flick/js`:"),
        "{js_inspect_text}"
    );
    for expected in ["--target", "--typescript", "--runtime"] {
        assert!(
            js_inspect_text.contains(expected),
            "missing {expected} in:\n{js_inspect_text}"
        );
    }
    for forbidden in ["--declarations", "--devtools", "--cgo"] {
        assert!(
            !js_inspect_text.contains(forbidden),
            "unexpected {forbidden} in:\n{js_inspect_text}"
        );
    }
}

#[test]
fn help_shows_only_go_options_for_each_verb() {
    let js_dir = project(
        "backend_opts_help_go_from_js",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let go_dir = project(
        "backend_opts_help_go",
        &[
            ("flick.toml", &format!("{APP}\n[build]\ntarget = \"go\"\n")),
            ("src/App/Main.fl", MAIN),
        ],
    );

    // Go project (`[build] target = "go"`) and `--target go` show only `flick/go` options.
    for verb in ["build", "run", "dev", "test"] {
        let from_project = flick_in(&go_dir, &[verb, "--help"]);
        assert!(from_project.status.success(), "go {verb}: {from_project:?}");
        let text = stdout(&from_project);
        assert!(
            text.contains("Options of `flick/go`:"),
            "go {verb}:\n{text}"
        );
        assert!(text.contains("--cgo"), "go {verb}:\n{text}");
        for forbidden in [
            "Options of `flick/js`:",
            "--typescript",
            "--declarations",
            "--promise-facade",
            "--schema-aot",
            "--runtime",
            "--devtools",
            "--ui",
        ] {
            assert!(
                !text.contains(forbidden),
                "go {verb}: unexpected {forbidden}\n{text}"
            );
        }

        let from_flag = flick_in(&js_dir, &[verb, "--target", "go", "--help"]);
        assert!(
            from_flag.status.success(),
            "--target go {verb}: {from_flag:?}"
        );
        assert_eq!(stdout(&from_flag), text, "{verb} --target go vs project");
    }

    // `flick --project <go_dir> help build` shows Go options.
    let help_go = flick(&["--project", go_dir.to_str().expect("utf8"), "help", "build"]);
    assert!(help_go.status.success(), "{help_go:?}");
    assert!(stdout(&help_go).contains("Options of `flick/go`:"));
    assert!(stdout(&help_go).contains("--cgo"));

    // Go `inspect --help` has no backend options at all.
    let go_inspect = flick_in(&go_dir, &["inspect", "--help"]);
    assert!(go_inspect.status.success(), "{go_inspect:?}");
    let go_inspect_text = stdout(&go_inspect);
    for forbidden in [
        "Options of `flick/js`:",
        "Options of `flick/go`:",
        "--cgo",
        "--typescript",
        "--runtime",
    ] {
        assert!(
            !go_inspect_text.contains(forbidden),
            "go inspect: unexpected {forbidden}\n{go_inspect_text}"
        );
    }
}

#[test]
fn removed_cli_flags_fail_with_p057_and_replacement() {
    let dir = project(
        "backend_opts_p057_cli",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let cases: &[(&[&str], &str)] = &[
        (&["build", "--emit", "js"], "--target js"),
        (&["build", "--emit=js"], "--target js"),
        (&["build", "--emit", "ts"], "--target js --typescript"),
        (&["build", "--emit=ts"], "--target js --typescript"),
        (&["build", "--emit", "go"], "--target go"),
        (&["build", "--emit=go"], "--target go"),
        (&["build", "--emit-dts"], "--declarations"),
        (&["build", "--rt"], "--runtime rt"),
        (&["test", "--emit", "js"], "--target js"),
        (&["test", "--emit=js"], "--target js"),
        (&["test", "--emit", "go"], "--target go"),
        (&["test", "--emit=go"], "--target go"),
        (&["test", "--rt"], "--runtime rt"),
        (&["run", "--rt"], "--runtime rt"),
        (&["dev", "--rt"], "--runtime rt"),
        (
            &["inspect", "App.Main", "--emit", "js"],
            "--emit output --target js",
        ),
        (
            &["inspect", "App.Main", "--emit=js"],
            "--emit output --target js",
        ),
        (
            &["inspect", "App.Main", "--emit", "ts"],
            "--emit output --target js --typescript",
        ),
        (
            &["inspect", "App.Main", "--emit=ts"],
            "--emit output --target js --typescript",
        ),
    ];
    for (args, expected) in cases {
        let out = flick_in(&dir, args);
        assert_eq!(code(&out), Some(2), "{args:?}: {out:?}");
        let err = stderr(&out);
        assert!(err.contains("FLICK-P057"), "{args:?}: {err}");
        assert!(
            err.contains(expected),
            "{args:?}: expected `{expected}` in:\n{err}"
        );
    }
}

#[test]
fn removed_build_manifest_keys_fail_with_p057_and_replacement() {
    let cases: &[(&str, &str)] = &[
        ("emit = \"js\"", "[build] target = \"js\""),
        ("emit = \"ts\"", "[target.js]"),
        ("emit = \"go\"", "[build] target = \"go\""),
        ("emit-dts = true", "[target.js]"),
        ("promise-facade = true", "[target.js]"),
        ("schema-aot = true", "[target.js]"),
        ("effect = \"rt\"", "[target.js]"),
        ("effect = \"effect\"", "runtime = \"effect\""),
        ("ui = \"./renderer.mjs\"", "[target.js]"),
    ];
    for (idx, (line, expected)) in cases.iter().enumerate() {
        let dir = project(
            &format!("backend_opts_p057_toml_{idx}"),
            &[
                ("flick.toml", &format!("{APP}\n[build]\n{line}\n")),
                ("src/App/Main.fl", MAIN),
            ],
        );
        let out = flick_in(&dir, &["check"]);
        assert_eq!(code(&out), Some(2), "{line}: {out:?}");
        let err = stderr(&out);
        assert!(err.contains("FLICK-P057"), "{line}: {err}");
        assert!(err.contains("flick.toml"), "{line}: {err}");
        assert!(
            err.contains(expected),
            "{line}: expected `{expected}` in:\n{err}"
        );
    }
}

#[test]
fn passthrough_arguments_after_double_dash_are_not_rejected_as_p057() {
    let dir = project(
        "backend_opts_passthrough",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n[scripts]\necho = \"printf '%s|' \\\"$@\\\"\"\n",
            ),
            ("src/Lib/Math.fl", LIB),
        ],
    );
    let out = flick_in(
        &dir,
        &["run", "echo", "--", "--rt", "--emit", "go", "--emit-dts"],
    );
    assert!(out.status.success(), "{out:?}");
    let printed = stdout(&out);
    assert!(
        printed.contains("--rt") && printed.contains("--emit") && printed.contains("--emit-dts"),
        "{printed}"
    );
}

#[test]
fn unknown_invalid_wrong_verb_and_cross_backend_options_fail_with_p058() {
    let js_dir = project(
        "backend_opts_p058_js",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let go_dir = project(
        "backend_opts_p058_go",
        &[
            ("flick.toml", &format!("{APP}\n[build]\ntarget = \"go\"\n")),
            ("src/App/Main.fl", MAIN),
        ],
    );

    // Typo suggests closest option.
    let typo = flick_in(&js_dir, &["build", "--typscript"]);
    assert_eq!(code(&typo), Some(2), "{typo:?}");
    let err = stderr(&typo);
    assert!(
        err.contains("FLICK-P058") && err.contains("--typescript"),
        "{err}"
    );

    // Wrong verb names the verbs that accept the option.
    let wrong_verb = flick_in(&js_dir, &["build", "--devtools"]);
    assert_eq!(code(&wrong_verb), Some(2), "{wrong_verb:?}");
    let err = stderr(&wrong_verb);
    assert!(
        err.contains("FLICK-P058") && err.contains("flick run") && err.contains("flick dev"),
        "{err}"
    );

    // Cross-backend hints.
    let cgo_on_js = flick_in(&js_dir, &["build", "--cgo"]);
    assert_eq!(code(&cgo_on_js), Some(2), "{cgo_on_js:?}");
    let err = stderr(&cgo_on_js);
    assert!(
        err.contains("FLICK-P058") && err.contains("flick/go") && err.contains("--target go"),
        "{err}"
    );

    let ts_on_go = flick_in(&go_dir, &["build", "--typescript"]);
    assert_eq!(code(&ts_on_go), Some(2), "{ts_on_go:?}");
    let err = stderr(&ts_on_go);
    assert!(
        err.contains("FLICK-P058") && err.contains("flick/js") && err.contains("--target js"),
        "{err}"
    );

    // Invalid choice value.
    let bad_choice = flick_in(&js_dir, &["build", "--runtime", "nope"]);
    assert_eq!(code(&bad_choice), Some(2), "{bad_choice:?}");
    let err = stderr(&bad_choice);
    assert!(
        err.contains("FLICK-P058") && err.contains("rt") && err.contains("effect"),
        "{err}"
    );

    // Invalid boolean flag value.
    let bad_bool = flick_in(&js_dir, &["build", "--typescript=maybe"]);
    assert_eq!(code(&bad_bool), Some(2), "{bad_bool:?}");
    let err = stderr(&bad_bool);
    assert!(err.contains("FLICK-P058"), "{err}");

    // TOML errors in `[target.js]` carry `FLICK-P058` and spans.
    let toml_cases: &[(&str, &str)] = &[
        ("typscript = true", "typescript"),
        ("typescript = \"yes\"", "boolean"),
        ("runtime = \"efect\"", "effect"),
        ("cgo = true", "[target.go]"),
    ];
    for (idx, (line, hint)) in toml_cases.iter().enumerate() {
        let dir = project(
            &format!("backend_opts_p058_toml_{idx}"),
            &[
                ("flick.toml", &format!("{APP}\n[target.js]\n{line}\n")),
                ("src/App/Main.fl", MAIN),
            ],
        );
        let out = flick_in(&dir, &["build"]);
        assert_eq!(code(&out), Some(2), "{line}: {out:?}");
        let err = stderr(&out);
        assert!(
            err.contains("FLICK-P058") && err.contains("flick.toml"),
            "{line}: {err}"
        );
        assert!(err.contains(hint), "{line}: expected `{hint}` in:\n{err}");
    }
}

#[test]
fn unknown_target_fails_with_p059_on_cli_and_in_manifest() {
    let dir = project(
        "backend_opts_p059",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let cli = flick_in(&dir, &["build", "--target", "gp"]);
    assert_eq!(code(&cli), Some(2), "{cli:?}");
    let err = stderr(&cli);
    assert!(
        err.contains("FLICK-P059") && err.contains("--target go"),
        "{err}"
    );

    let bad_manifest = project(
        "backend_opts_p059_toml",
        &[
            (
                "flick.toml",
                &format!("{APP}\n[build]\ntarget = \"unknown\"\n"),
            ),
            ("src/App/Main.fl", MAIN),
        ],
    );
    let from_toml = flick_in(&bad_manifest, &["build"]);
    assert_eq!(code(&from_toml), Some(2), "{from_toml:?}");
    let err = stderr(&from_toml);
    assert!(
        err.contains("FLICK-P059") && err.contains("unknown"),
        "{err}"
    );
}

#[test]
fn cli_flag_overrides_toml_and_option_changes_invalidate_cache() {
    let dir = project(
        "backend_opts_precedence_cache",
        &[
            (
                "flick.toml",
                &format!("{APP}\n[target.js]\ntypescript = true\n"),
            ),
            ("src/App/Main.fl", MAIN),
        ],
    );

    let count_cache = || {
        fs::read_dir(dir.join(".flick/cache"))
            .expect("cache dir")
            .count()
    };

    // Default from `[target.js]` emits `.ts`.
    let ts_build = flick_in(&dir, &["build"]);
    assert!(ts_build.status.success(), "{ts_build:?}");
    assert!(dir.join("dist/App/Main.ts").is_file());
    assert!(!dir.join("dist/App/Main.js").exists());
    assert_eq!(count_cache(), 1);

    // `--typescript=false` overrides `[target.js] typescript = true`, removes stale `.ts`, and adds a new cache entry.
    let js_build = flick_in(&dir, &["build", "--typescript=false"]);
    assert!(js_build.status.success(), "{js_build:?}");
    assert!(dir.join("dist/App/Main.js").is_file());
    assert!(!dir.join("dist/App/Main.ts").exists());
    assert_eq!(count_cache(), 2);

    // Adding `--declarations` invalidates cache again and emits `.d.ts`.
    let dts_build = flick_in(&dir, &["build", "--typescript=false", "--declarations"]);
    assert!(dts_build.status.success(), "{dts_build:?}");
    assert!(dir.join("dist/App/Main.d.ts").is_file());
    assert_eq!(count_cache(), 3);
}
