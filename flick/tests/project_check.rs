//! `flick check` over projects: discovery, roots, transitive imports and exit codes.

mod common;

use std::{env, process};

use common::{flick, flick_in};

const PROJECTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/check/projects");

/// Exit code and the diagnostic codes of a JSON run.
fn codes(out: &process::Output) -> (Option<i32>, Vec<String>) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let codes = stderr
        .lines()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).expect("NDJSON line");
            v["code"].as_str().expect("code").to_owned()
        })
        .collect();
    (out.status.code(), codes)
}

#[test]
fn finds_the_project_upwards_and_checks_both_roots() {
    let ok = flick_in(format!("{PROJECTS}/app/src/App"), &["check"]);
    assert_eq!(
        ok.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let ok = flick(&["check", "--project", "tests/check/projects/app/flick.toml"]);
    assert_eq!(ok.status.code(), Some(0));

    let out = flick(&[
        "check",
        "--diagnostics",
        "json",
        "--project",
        "tests/check/projects/paths",
    ]);
    let (status, codes) = codes(&out);
    assert_eq!(status, Some(1));
    assert_eq!(
        codes,
        ["FLICK-R001", "FLICK-R001", "FLICK-R002"],
        "the test root is checked too"
    );
}

#[test]
fn named_paths_still_check_transitive_imports() {
    let out = flick_in(
        format!("{PROJECTS}/cycle"),
        &["check", "--diagnostics", "json", "src/App/A.fl"],
    );
    assert_eq!(codes(&out), (Some(1), vec!["FLICK-R006".to_owned()]));
    let shown = flick_in(format!("{PROJECTS}/cycle"), &["check", "src/App/A.fl"]);
    let stderr = String::from_utf8_lossy(&shown.stderr);
    assert!(
        stderr.contains("import cycle: App.A -> App.B -> App.C -> App.A"),
        "{stderr}"
    );
    assert!(
        stderr.contains("src/App/A.fl"),
        "paths are shown relative to the working directory: {stderr}"
    );
}

#[test]
fn configuration_and_missing_dependencies_exit_with_2() {
    for case in [
        "config_unknown_key",
        "config_kind",
        "config_no_entry",
        "config_version",
        "config_sql_dialect",
        "config_sql_unknown",
        "entry",
    ] {
        let out = flick(&[
            "check",
            "--project",
            &format!("tests/check/projects/{case}"),
        ]);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{case}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let out = flick(&[
        "check",
        "--diagnostics",
        "json",
        "--project",
        "tests/check/projects/packages",
    ]);
    let (status, codes) = codes(&out);
    assert_eq!(status, Some(2));
    assert_eq!(codes, ["FLICK-P009", "FLICK-P009", "FLICK-P010"]);
    let human = String::from_utf8_lossy(
        &flick(&["check", "--project", "tests/check/projects/packages"]).stderr,
    )
    .into_owned();
    assert!(human.contains("flick install"), "{human}");
}

#[test]
fn without_a_project_check_needs_paths() {
    let dir = env::temp_dir();
    let out = flick_in(&dir, &["check", "--diagnostics", "json"]);
    assert_eq!(codes(&out), (Some(2), vec!["FLICK-P008".to_owned()]));
    let out = flick(&["check", "--project", "tests/check/projects/nowhere"]);
    assert_eq!(out.status.code(), Some(2));
}
