use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};

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
    // a hit that writes its artifacts again is no miss
    fs::remove_dir_all(dir.join("dist")).expect("remove the output");
    let restored = stderr(&["build", "--timings"]);
    assert!(
        restored.contains(" ms  Restoring cached artifacts\n"),
        "{restored}"
    );
    assert!(!restored.contains("missed"), "{restored}");

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
    let phases: Vec<&str> = timings["phases"]
        .as_array()
        .expect("phases")
        .iter()
        .filter_map(|p| p["label"].as_str())
        .collect();
    for expected in [
        "backend component init (wasmtime load)",
        "backend prepare: program CBOR encode",
        "backend host input snapshot",
        "backend host request validation (decode #1)",
        "backend check-target (guest decode #2)",
        "backend generate #1 (guest decode #3)",
        "backend result validation #1",
        "backend determinism replay generate #2 (guest decode #4)",
        "backend replay validation #2",
        "backend external tool steps",
        "backend emission residual",
        "atomic output publication",
    ] {
        assert!(
            phases.contains(&expected),
            "missing attributed phase `{expected}` in {phases:?}"
        );
    }
    let metrics = timings["metrics"].as_array().expect("metrics");
    assert!(
        metrics
            .iter()
            .any(|m| m["label"] == "program_bytes" && m["bytes"].as_u64().unwrap_or(0) > 0),
        "expected non-zero program_bytes in {metrics:?}"
    );
    assert!(
        metrics
            .iter()
            .any(|m| m["label"] == "inputs_bytes" && m["bytes"].as_u64().unwrap_or(0) > 0),
        "expected non-zero inputs_bytes in {metrics:?}"
    );
}

#[test]
fn a_project_keeps_the_builds_stored_last() {
    let dir = project(
        "build_cache_bound",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    assert!(flick_in(&dir, &["build"]).status.success());
    let cache = dir.join(".flick/cache");
    let long_ago = SystemTime::now() - Duration::from_hours(1);
    for n in 0..20 {
        let stale = cache.join(format!("stale{n}.json"));
        fs::write(&stale, "[]").expect("a stale build");
        File::options()
            .write(true)
            .open(&stale)
            .and_then(|file| file.set_modified(long_ago))
            .expect("a time of writing");
    }
    let source = dir.join("src/App/Main.fl");
    fs::write(&source, MAIN.replace("hi", "ho")).expect("an edit");
    assert!(flick_in(&dir, &["build"]).status.success());
    let kept = fs::read_dir(&cache).expect("the cache").count();
    assert_eq!(kept, 16, "the two builds and the newest of the stale ones");

    // the build stored last is still the one an unchanged build takes
    let module = dir.join("dist/App/Main.js");
    let written = |path: &Path| {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .expect("mtime")
    };
    let before = written(&module);
    assert!(flick_in(&dir, &["build"]).status.success());
    assert_eq!(written(&module), before);
}

/// In release, a module reads the fields of a projection another module
/// exports where it uses it, from that module's body: the build cache keys
/// on every source, so an edit of that body alone, its type the same,
/// builds the module that inlined it again.
#[test]
fn an_edited_projection_builds_again_the_module_that_inlined_it() {
    let lib = |label: &str| {
        format!(
            "module App.Lib exposing (User, get)\n\n\
             type alias User = {{ name : String; nick : String }}\n\n\
             get : User -> String\nlet get = .{label}\n"
        )
    };
    let main = "module App.Main exposing (main)\n\n\
        import App.Lib exposing (User)\n\n\
        main : Unit ->{Host.Arguments, Host.Console; AppError} Unit\n\
        let main () =\n  \
        let users = List.map (fun a -> { name = a; nick = a ++ \"!\" }) (Platform.args ())\n  \
        Platform.stdout_line (show (List.map App.Lib.get users))\n";
    let dir = project(
        "build_projection",
        &[
            ("flick.toml", APP),
            ("src/App/Lib.fl", &lib("name")),
            ("src/App/Main.fl", main),
        ],
    );
    let built = |dir: &Path| {
        let out = flick_in(dir, &["build", "--profile", "release"]);
        assert!(out.status.success(), "{out:?}");
        fs::read_to_string(dir.join("dist/App/Main.js")).expect("the module")
    };
    let read = |js: &str, label: &str| js.contains(&format!(".{label});"));
    let before = built(&dir);
    assert!(read(&before, "name") && !before.contains("get"), "{before}");
    fs::write(dir.join("src/App/Lib.fl"), lib("nick")).expect("an edit");
    let after = built(&dir);
    assert!(read(&after, "nick") && !read(&after, "name"), "{after}");
}
