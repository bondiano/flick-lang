use std::fs;

use super::{APP, MAIN, flick_in, project};

#[test]
fn unchanged_build_keeps_artifact_and_restores_a_missing_one() {
    let dir = project(
        "build_noop",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let module = dir.join("dist/App/Main.js");
    assert!(flick_in(&dir, &["build"]).status.success());
    let before = fs::metadata(&module)
        .expect("module")
        .modified()
        .expect("mtime");
    assert!(flick_in(&dir, &["build"]).status.success());
    assert_eq!(
        fs::metadata(&module)
            .expect("module")
            .modified()
            .expect("mtime"),
        before
    );
    fs::remove_file(&module).expect("remove module");
    assert!(flick_in(&dir, &["build"]).status.success());
    assert!(module.is_file());
}

#[test]
fn timings_name_the_phases_and_the_input_that_missed_the_cache() {
    let dir = project(
        "build_timings",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let stderr = |args: &[&str]| {
        let out = flick_in(&dir, args);
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stderr).expect("utf-8 stderr")
    };
    let first = stderr(&["build", "--timings"]);
    assert!(first.contains(" ms  Compiling Flick modules\n"), "{first}");
    assert!(first.contains(" ms  App.Main\n"), "{first}");
    assert!(
        first.contains("Build cache missed: no earlier build is recorded\n"),
        "{first}"
    );
    // a hit compiles nothing and reports no miss
    let hit = stderr(&["build", "--timings"]);
    assert!(hit.contains(" ms  Checking build cache\n"), "{hit}");
    assert!(
        !hit.contains("Compiling") && !hit.contains("missed"),
        "{hit}"
    );
    assert!(stderr(&["build"]).find("Timings").is_none());

    fs::write(dir.join("src/App/Main.fl"), format!("{MAIN}\n")).expect("edit");
    let events = stderr(&["build", "--timings", "--diagnostics", "json"]);
    let timings: serde_json::Value = (events.lines())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("an event"))
        .find(|event| event["type"] == "timings")
        .expect("a timings event");
    assert_eq!(
        timings["cache-misses"][0],
        "the last cached build differs in `input src/App/Main.fl`"
    );
    assert_eq!(timings["modules"][0]["module"], "App.Main");
}
