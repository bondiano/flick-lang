use std::fs;

use super::common::{APP, flick_in, project};

const CRON: &str = r#"module App.Main exposing (main)

main : Unit ->{} Unit
let main () =
  match Cron.parse (DateTime.zone_make_offset 0) "0 9 * * *" with
  | Ok cron -> ignore (Schedule.cron cron)
  | Err _ -> ()
"#;

#[test]
fn rt_checks_the_selected_custom_runner() {
    let config = APP.replace(
        "entry = \"App.Main\"",
        "entry = \"App.Main\"\nrunner = \"App.Runner\"",
    );
    let runner = r#"module App.Runner exposing (Main, run)
type alias Main = Unit ->{} Unit
run : Main ->{} Unit
let run main =
  match Cron.parse (DateTime.zone_make_offset 0) "0 9 * * *" with
  | Ok cron -> ignore (Schedule.cron cron)
  | Err _ -> ()
  main ()
"#;
    let app = project(
        "capabilities/custom_runner",
        &[
            ("flick.toml", &config),
            ("src/App/Runner.fl", runner),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nmain : Unit ->{} Unit\nlet main () = ()\n",
            ),
        ],
    );
    let out = flick_in(&app, &["build", "--offline"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8(out.stderr).expect("UTF-8 diagnostics");
    assert!(stderr.contains("FLICK-P012"), "{stderr}");
    assert!(stderr.contains("Schedule.cron"), "{stderr}");
}

#[test]
fn rt_reports_missing_runtime_capabilities_before_writing_artifacts() {
    let app = project(
        "capabilities/missing_adapter",
        &[("flick.toml", APP), ("src/App/Main.fl", CRON)],
    );
    for profile in ["debug", "release"] {
        let out = flick_in(
            &app,
            &[
                "build",
                "--offline",
                "--profile",
                profile,
                "--diagnostics",
                "json",
            ],
        );
        assert_eq!(out.status.code(), Some(1), "{profile}: {out:?}");
        let stderr = String::from_utf8(out.stderr).expect("UTF-8 diagnostics");
        let events: Vec<serde_json::Value> = stderr
            .lines()
            .map(|line| serde_json::from_str(line).expect("one JSON diagnostic per line"))
            .collect();
        assert_eq!(events.len(), 3, "{stderr}");
        for diagnostic in events {
            assert_eq!(diagnostic["code"], "FLICK-P012");
            assert_eq!(diagnostic["severity"], "error");
            assert!(
                diagnostic["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("@flick-lang/effect-ts")),
                "{diagnostic}"
            );
        }
        assert!(!app.join("dist/flick-main.js").exists());
    }
}

#[test]
fn rt_can_build_native_schedule_operations_with_an_unused_cron_binding() {
    let source = r"module App.Main exposing (main)

unused : Cron.Cron ->{} Unit
let unused cron = ignore (Schedule.cron cron)

main : Unit ->{} Unit
let main () = ignore (Schedule.recurs 1)
";
    let app = project(
        "capabilities/unused_binding",
        &[("flick.toml", APP), ("src/App/Main.fl", source)],
    );
    for profile in ["debug", "release"] {
        let out = flick_in(&app, &["build", "--offline", "--profile", profile]);
        assert_eq!(out.status.code(), Some(0), "{profile}: {out:?}");
        assert!(app.join("dist/flick-main.js").is_file());
    }
}

#[test]
fn rt_can_build_cron_when_the_project_declares_the_effect_adapter() {
    let app = project(
        "capabilities/declared_adapter",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", CRON),
            (
                "package.json",
                r#"{"type":"module","dependencies":{"@flick-lang/effect-ts":"*"}}"#,
            ),
        ],
    );
    for profile in ["debug", "release"] {
        let out = flick_in(&app, &["build", "--offline", "--profile", profile]);
        assert_eq!(out.status.code(), Some(0), "{profile}: {out:?}");
        let js = fs::read_to_string(app.join("dist/_flick/Schedule.js")).expect("Schedule module");
        assert!(js.contains("@flick-lang/effect-ts/bridge"), "{js}");
    }
}

#[test]
fn rt_checks_a_reexported_entry_against_its_runtime_capabilities() {
    let source = CRON.replace("module App.Main", "module App.Program");
    let app = project(
        "capabilities/reexported_entry",
        &[
            ("flick.toml", APP),
            ("src/App/Program.fl", &source),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nimport App.Program exposing (main)\n",
            ),
        ],
    );
    let out = flick_in(&app, &["build", "--offline"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8(out.stderr).expect("UTF-8 diagnostics");
    assert!(stderr.contains("FLICK-P012"), "{stderr}");
    assert!(stderr.contains("Schedule.cron"), "{stderr}");
}

#[test]
fn unused_members_of_a_mixed_builtin_do_not_import_the_adapter() {
    let source = r"module App.Main exposing (main)

main : Unit ->{} Unit
let main () = ignore (DateTime.now ())
";
    let app = project(
        "capabilities/mixed_builtin",
        &[("flick.toml", APP), ("src/App/Main.fl", source)],
    );
    let out = flick_in(&app, &["build", "--offline"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let js = fs::read_to_string(app.join("dist/_flick/DateTime.js")).expect("DateTime module");
    assert!(
        !js.lines()
            .any(|line| line.starts_with("import ") && line.contains("@flick-lang/effect-ts")),
        "{js}"
    );
}

#[test]
fn an_unloaded_source_module_does_not_require_its_runtime_capabilities() {
    let initializer = r#"module App.Unused exposing (plan)

make_cron : Unit -> Cron.Cron
foreign pure make_cron from "./cron.mjs" "make"

plan : Schedule.Schedule none none Int Duration.Duration
let plan = Schedule.cron (make_cron ())
"#;
    let app = project(
        "capabilities/unloaded_module",
        &[
            ("flick.toml", APP),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nmain : Unit ->{} Unit\nlet main () = ()\n",
            ),
            ("src/App/Unused.fl", initializer),
        ],
    );
    let out = flick_in(&app, &["build", "--offline"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn changing_the_adapter_dependency_invalidates_the_cached_build() {
    let with_adapter = r#"{"type":"module","dependencies":{"@flick-lang/effect-ts":"*"}}"#;
    let app = project(
        "capabilities/adapter_cache",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", CRON),
            ("package.json", with_adapter),
        ],
    );
    let built = flick_in(&app, &["build", "--offline"]);
    assert_eq!(built.status.code(), Some(0), "{built:?}");
    fs::write(app.join("package.json"), r#"{"type":"module"}"#).expect("remove adapter dependency");
    let removed = flick_in(&app, &["build", "--offline"]);
    assert_eq!(removed.status.code(), Some(1), "{removed:?}");
    assert!(String::from_utf8_lossy(&removed.stderr).contains("FLICK-P012"));
    fs::write(app.join("package.json"), with_adapter).expect("restore adapter dependency");
    let restored = flick_in(&app, &["build", "--offline"]);
    assert_eq!(restored.status.code(), Some(0), "{restored:?}");
}

#[test]
fn reexported_modules_contribute_their_eager_initializers() {
    let app = project(
        "capabilities/eager_reexport",
        &[
            ("flick.toml", APP),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main, value)\nimport App.Other exposing (value)\nmain : Unit ->{} Unit\nlet main () = ()\n",
            ),
            (
                "src/App/Other.fl",
                "module App.Other exposing (value)\nlet zone = DateTime.zone_make_offset 0\nvalue : Int\nlet value = 1\n",
            ),
        ],
    );
    for profile in ["debug", "release"] {
        let out = flick_in(&app, &["build", "--offline", "--profile", profile]);
        assert_eq!(out.status.code(), Some(1), "{out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("FLICK-P012") && stderr.contains("DateTime.zone_make_offset"),
            "{stderr}"
        );
        assert!(!app.join("dist/flick-main.js").exists());
    }
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn supported_native_members_emit_typechecking_typescript() {
    use std::path::Path;
    use std::process::Command;
    let app = project(
        "capabilities/mixed_typescript",
        &[
            ("flick.toml", APP),
            ("package.json", "{\"type\":\"module\"}"),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main, Generator)\nservice Generator =\n  value : Unit ->{} Unit\nmain : Unit ->{} Unit\nlet main () = ignore (DateTime.now ())\n",
            ),
        ],
    );
    for profile in ["debug", "release"] {
        let out = flick_in(
            &app,
            &["build", "--offline", "--emit", "ts", "--profile", profile],
        );
        assert!(out.status.success(), "{out:?}");
        let checked =
            Command::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../node_modules/.bin/tsc"))
                .args([
                    "--noEmit",
                    "--allowImportingTsExtensions",
                    "--skipLibCheck",
                    "--target",
                    "ES2024",
                    "--module",
                    "nodenext",
                ])
                .arg(app.join("dist/flick-main.ts"))
                .output()
                .expect("tsc runs");
        assert!(checked.status.success(), "{profile}: {checked:?}");
    }
}

#[test]
fn a_reexported_main_still_initializes_its_declaring_entry_module() {
    let app = project(
        "capabilities/entry_eager_reexport",
        &[
            ("flick.toml", APP),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nimport App.Program exposing (main)\nlet _ = DateTime.zone_make_offset 0\n",
            ),
            (
                "src/App/Program.fl",
                "module App.Program exposing (main)\nmain : Unit ->{} Unit\nlet main () = ()\n",
            ),
        ],
    );
    let out = flick_in(&app, &["build", "--offline"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-P012"));
    assert!(!app.join("dist/flick-main.js").exists());
}
