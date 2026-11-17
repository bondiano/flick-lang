use std::fs;

use super::common::{flick_in, project};

const LIB: &str = r#"schema-version = 1

[project]
kind = "lib"

[build]
environment = "js/browser"
"#;

const LIB_NODE: &str = r#"schema-version = 1

[project]
kind = "lib"

[build]
environment = "js/node"
"#;

const DB_EXPORT: &str = r"module App.Main exposing (run)

service Db =
  query : String ->{} String

run : String ->{Db} String
let run key = Db.query key
";

const PURE: &str = r"module App.Main exposing (run)

run : String -> String
let run key = key
";

#[test]
fn check_rejects_reachable_db_on_js_browser_before_generation() {
    let app = project(
        "target_preflight/browser_db",
        &[("flick.toml", LIB), ("src/App/Main.fl", DB_EXPORT)],
    );
    let out = flick_in(&app, &["check", "--diagnostics", "json"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8(out.stderr).expect("UTF-8");
    let events: Vec<serde_json::Value> = stderr
        .lines()
        .map(|line| serde_json::from_str(line).expect("json diagnostic"))
        .collect();
    let found = events
        .iter()
        .find(|d| d["code"] == "FLICK-E020")
        .expect("E020");
    assert_eq!(found["message"], "unhandled effect Db on js/browser");
    let start =
        usize::try_from(found["range"]["start"].as_u64().expect("start")).expect("start fits");
    let end = usize::try_from(found["range"]["end"].as_u64().expect("end")).expect("end fits");
    assert_eq!(&DB_EXPORT[start..end], "Db.query");
    assert!(!app.join("dist").exists());
}

#[test]
fn node_neighbor_accepts_the_same_db_export() {
    let app = project(
        "target_preflight/node_db",
        &[("flick.toml", LIB_NODE), ("src/App/Main.fl", DB_EXPORT)],
    );
    let out = flick_in(&app, &["check"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn browser_accepts_a_program_without_gated_effects() {
    let app = project(
        "target_preflight/browser_pure",
        &[("flick.toml", LIB), ("src/App/Main.fl", PURE)],
    );
    let out = flick_in(&app, &["check"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn build_refuses_browser_db_before_writing_artifacts() {
    let app = project(
        "target_preflight/browser_build",
        &[("flick.toml", LIB), ("src/App/Main.fl", DB_EXPORT)],
    );
    let out = flick_in(&app, &["build", "--offline"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FLICK-E020") && stderr.contains("unhandled effect Db on js/browser"),
        "{stderr}"
    );
    assert!(!app.join("dist/App/Main.js").exists());
}

#[test]
fn changing_environment_invalidates_a_cached_preflight_decision() {
    let app = project(
        "target_preflight/env_cache",
        &[("flick.toml", LIB_NODE), ("src/App/Main.fl", DB_EXPORT)],
    );
    let ok = flick_in(&app, &["build", "--offline"]);
    assert_eq!(ok.status.code(), Some(0), "{ok:?}");
    assert!(
        app.join("dist/App/Main.js").is_file()
            || app.join("dist/_flick/App/Main.js").is_file()
            || app.join("dist").exists()
    );
    fs::write(app.join("flick.toml"), LIB).expect("flip to browser");
    let rejected = flick_in(&app, &["build", "--offline"]);
    assert_eq!(rejected.status.code(), Some(1), "{rejected:?}");
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("FLICK-E020"), "{stderr}");
}
