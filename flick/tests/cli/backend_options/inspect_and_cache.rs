use std::fs;

use super::{LIB, stderr, stdout};
use crate::common::{flick_in, project};

#[test]
fn inspect_output_supports_js_typescript_and_go_and_iface_loads_no_backend() {
    let dir = project(
        "backend_opts_inspect",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            ("src/Lib/Math.fl", LIB),
        ],
    );

    let js = flick_in(&dir, &["inspect", "Lib.Math", "--emit", "output"]);
    assert!(js.status.success(), "{js:?}");
    let js_out = stdout(&js);
    assert!(js_out.contains("export const inc"), "{js_out}");

    let ts = flick_in(
        &dir,
        &["inspect", "Lib.Math", "--emit", "output", "--typescript"],
    );
    assert!(ts.status.success(), "{ts:?}");
    let ts_out = stdout(&ts);
    assert!(
        ts_out.contains("export const inc") && ts_out.contains("number"),
        "{ts_out}"
    );

    // Go `inspect --emit output` works even on a library without `[tools]` or `go`!
    let go = flick_in(
        &dir,
        &["inspect", "Lib.Math", "--emit", "output", "--target", "go"],
    );
    assert!(go.status.success(), "{go:?}");
    let go_out = stdout(&go);
    assert!(
        go_out.contains("package main") && go_out.contains("func "),
        "{go_out}"
    );

    // `inspect --emit iface` never loads any backend — even if `[build.backend]` points to a missing file.
    let broken_backend = project(
        "backend_opts_inspect_iface_no_backend",
        &[
            (
                "flick.toml",
                &format!(
                    "schema-version = 1\n[project]\nkind = \"lib\"\n\n[build.backend]\nid = \"example/missing\"\nversion = \"1.0.0\"\nsha256 = \"{}\"\ncomponent = \"missing.wasm\"\n",
                    "a".repeat(64)
                ),
            ),
            ("src/Lib/Math.fl", LIB),
        ],
    );
    let iface = flick_in(&broken_backend, &["inspect", "Lib.Math", "--emit", "iface"]);
    assert!(iface.status.success(), "{iface:?}");
    assert!(stdout(&iface).contains("Lib.Math"));
}

#[test]
fn inspect_iface_on_environment_project_emits_no_false_e020() {
    let source = concat!(
        "module App.Store exposing (fetch)\n\n",
        "service Db =\n",
        "  read : Unit ->{} String\n\n",
        "fetch : Unit ->{Db} String\n",
        "let fetch () = Db.read ()\n",
    );
    for (name, env, extra) in [
        ("inspect_iface_js_node", "js/node", ""),
        ("inspect_iface_go_native", "go/native", "target = \"go\"\n"),
    ] {
        let dir = project(
            name,
            &[
                (
                    "flick.toml",
                    &format!(
                        "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\n{extra}environment = \"{env}\"\n"
                    ),
                ),
                ("src/App/Store.fl", source),
            ],
        );
        for emit in ["iface", "typed-ir"] {
            let out = flick_in(&dir, &["inspect", "App.Store", "--emit", emit]);
            assert!(out.status.success(), "{name} --emit {emit} failed: {out:?}");
            assert!(
                !stderr(&out).contains("FLICK-E020"),
                "{name} --emit {emit} emitted false E020:\n{}",
                stderr(&out)
            );
        }
    }
}

#[test]
fn cross_target_override_tracks_foreign_files_and_invalidates_cache() {
    let dir = project(
        "cross_target_cache_override",
        &[
            (
                "flick.toml",
                concat!(
                    "schema-version = 1\n",
                    "[project]\nkind = \"lib\"\n",
                    "[build]\ntarget = \"go\"\n",
                    "[tools]\n\"flick/go\" = [\"go\"]\n",
                ),
            ),
            (
                "package.json",
                r#"{"name":"cross-target","version":"0.1.0"}"#,
            ),
            (
                "src/App/Lib.fl",
                concat!(
                    "module App.Lib exposing (greet)\n\n",
                    "greet : Unit ->{} String\n",
                    "foreign greet from \"./helper.mjs\" \"greet\"\n",
                ),
            ),
            ("src/App/helper.mjs", "export const greet = () => \"v1\";\n"),
            (
                "src/App/helper.go",
                "package app\nfunc Greet() string { return \"v1\" }\n",
            ),
        ],
    );

    // `--inputs --target js` in a default-Go project tracks `.mjs` and `package.json`.
    let js_inputs = flick_in(&dir, &["build", "--inputs", "--target", "js"]);
    assert!(js_inputs.status.success(), "{js_inputs:?}");
    let js_inputs_out = stdout(&js_inputs);
    assert!(
        js_inputs_out.contains("helper.mjs") && js_inputs_out.contains("package.json"),
        "expected JS foreign + package inputs:\n{js_inputs_out}"
    );
    assert!(
        !js_inputs_out.contains("helper.go"),
        "JS inputs should not include `.go`:\n{js_inputs_out}"
    );

    // `--inputs --target go` tracks `.go` without `package.json`.
    let go_inputs = flick_in(&dir, &["build", "--inputs", "--target", "go"]);
    assert!(go_inputs.status.success(), "{go_inputs:?}");
    let go_inputs_out = stdout(&go_inputs);
    assert!(
        go_inputs_out.contains("helper.go"),
        "expected `.go` input:\n{go_inputs_out}"
    );
    assert!(
        !go_inputs_out.contains("package.json"),
        "Go inputs should not include `package.json`:\n{go_inputs_out}"
    );

    // Editing `.mjs` invalidates `flick build --target js` cache in a default-Go project.
    let first = flick_in(&dir, &["build", "--target", "js"]);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(
        fs::read_dir(dir.join(".flick/cache"))
            .expect("cache")
            .count(),
        1
    );
    fs::write(
        dir.join("src/App/helper.mjs"),
        "export const greet = () => \"v2\";\n",
    )
    .expect("edit helper.mjs");
    let second = flick_in(&dir, &["build", "--target", "js"]);
    assert!(second.status.success(), "{second:?}");
    assert!(
        !stderr(&second).contains("Restoring cached artifacts"),
        "editing `.mjs` under `--target js` must miss cache:\n{}",
        stderr(&second)
    );
    assert_eq!(
        fs::read_dir(dir.join(".flick/cache"))
            .expect("cache")
            .count(),
        2
    );
}
