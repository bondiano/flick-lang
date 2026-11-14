use super::*;
use std::str;

#[test]
fn cache_identity_keeps_selected_roots_separate_from_available_sources() {
    let a = "module A exposing (answer)\nanswer : Float\nlet answer = 1\n";
    let b = "module B exposing (answer)\nanswer : Float\nlet answer = 2\n";
    let dir = project(
        "cli_cache_selected_roots",
        &[("flick.toml", LIB), ("src/A.fl", a), ("src/B.fl", b)],
    );
    let first = flick_in(&dir, &["build", "src/A.fl"]);
    assert_eq!(code(&first), Some(0), "{first:?}");
    assert!(dir.join("dist/A.js").exists());
    let same = flick_in(&dir, &["build", "./src/../src/A.fl", "src/A.fl"]);
    assert_eq!(code(&same), Some(0), "{same:?}");
    assert_eq!(cached(&dir), 1, "normalized selection is a set");
    let second = flick_in(&dir, &["build", "src/B.fl"]);
    assert_eq!(code(&second), Some(0), "{second:?}");
    assert!(dir.join("dist/B.js").exists(), "B is not an A cache hit");
    assert_eq!(cached(&dir), 2);

    let dir = project(
        "cli_cache_partial_then_invalid_full",
        &[
            ("flick.toml", LIB),
            ("src/A.fl", a),
            (
                "src/B.fl",
                "module B exposing (answer)\nanswer : Float\nlet answer = \"wrong\"\n",
            ),
        ],
    );
    let partial = flick_in(&dir, &["build", "src/A.fl"]);
    assert_eq!(code(&partial), Some(0), "{partial:?}");
    assert_eq!(cached(&dir), 1);
    let full = flick_in(&dir, &["build"]);
    assert_eq!(code(&full), Some(1), "{full:?}");
    assert!(stderr(&full).contains("FLICK-T003"), "{full:?}");
    assert!(
        dir.join("dist/A.js").exists(),
        "failure keeps the last publication"
    );
    assert_eq!(cached(&dir), 1, "a failed full build is not cached");
}

#[test]
fn publication_reconciles_emit_modes_on_cache_misses_and_hits() {
    let dir = project(
        "cli_emit_reconciliation",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("dist/notes.txt", "mine"),
        ],
    );
    for _ in 0..2 {
        let js = flick_in(&dir, &["build", "--emit", "js", "--emit-dts"]);
        assert_eq!(code(&js), Some(0), "{js:?}");
        assert!(dir.join("dist/App/Main.js").exists());
        assert!(dir.join("dist/App/Main.d.ts").exists());
        assert!(!dir.join("dist/App/Main.ts").exists());
        let ts = flick_in(&dir, &["build", "--emit", "ts"]);
        assert_eq!(code(&ts), Some(0), "{ts:?}");
        assert!(dir.join("dist/App/Main.ts").exists());
        for stale in ["App/Main.js", "App/Main.js.map", "App/Main.d.ts"] {
            assert!(!dir.join("dist").join(stale).exists(), "stale {stale}");
        }
        assert_eq!(
            fs::read_to_string(dir.join("dist/notes.txt")).expect("notes"),
            "mine"
        );
    }
    assert_eq!(
        cached(&dir),
        2,
        "the second cycle restores cached artifacts"
    );
    assert_eq!(code(&flick_in(&dir, &["clean"])), Some(0));
    assert_eq!(
        dist(&dir),
        vec![("dist/notes.txt".to_owned(), "mine".to_owned())]
    );
}

#[test]
fn publication_removes_renamed_modules_without_claiming_user_files() {
    let original = "module Original exposing (answer)\nlet answer = 1\n";
    let renamed = "module Renamed exposing (answer)\nlet answer = 2\n";
    let dir = project(
        "cli_module_reconciliation",
        &[
            ("flick.toml", LIB),
            ("src/Original.fl", original),
            ("dist/notes.txt", "mine"),
        ],
    );
    assert_eq!(code(&flick_in(&dir, &["build"])), Some(0));
    fs::remove_file(dir.join("src/Original.fl")).expect("remove original");
    fs::write(dir.join("src/Renamed.fl"), renamed).expect("rename");
    assert_eq!(code(&flick_in(&dir, &["build"])), Some(0));
    for stale in ["Original.js", "Original.js.map", "Original.iface"] {
        assert!(!dir.join("dist").join(stale).exists(), "stale {stale}");
    }
    assert!(dir.join("dist/Renamed.js").exists());
    fs::remove_file(dir.join("src/Renamed.fl")).expect("remove renamed");
    fs::write(dir.join("src/Original.fl"), original).expect("restore original");
    assert_eq!(code(&flick_in(&dir, &["build"])), Some(0));
    assert_eq!(
        cached(&dir),
        2,
        "returning to the original source is a cache hit"
    );
    assert!(!dir.join("dist/Renamed.js").exists());
    assert!(dir.join("dist/Original.js").exists());
    assert_eq!(code(&flick_in(&dir, &["clean"])), Some(0));
    assert_eq!(
        dist(&dir),
        vec![("dist/notes.txt".to_owned(), "mine".to_owned())]
    );
}

#[test]
fn a_failed_publication_does_not_forget_the_previous_manifest() {
    let dir = project(
        "cli_publication_failure",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("dist/notes.txt", "mine"),
        ],
    );
    assert_eq!(code(&flick_in(&dir, &["build", "--emit", "js"])), Some(0));
    fs::create_dir(dir.join("dist/App/Main.ts")).expect("blocked output");
    let failed = flick_in(&dir, &["build", "--emit", "ts"]);
    assert_eq!(code(&failed), Some(1), "{failed:?}");
    assert!(stderr(&failed).contains("FLICK-P013"), "{failed:?}");
    assert!(
        dir.join("dist/App/Main.js").exists(),
        "stale removal waits for successful writes"
    );
    assert_eq!(code(&flick_in(&dir, &["clean"])), Some(0));
    assert!(
        !dir.join("dist/App/Main.js").exists(),
        "the old manifest is still owned"
    );
    assert_eq!(
        fs::read_to_string(dir.join("dist/notes.txt")).expect("notes"),
        "mine"
    );
}

#[test]
fn failed_generation_builds_release_their_reservations() {
    let dir = project(
        "cli_failed_generation",
        &[
            ("flick.toml", APP),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\nlet main () = missing\n",
            ),
        ],
    );
    let failed = flick_in(&dir, &["run"]);
    assert_eq!(code(&failed), Some(1), "{failed:?}");
    assert_eq!(
        fs::read_dir(dir.join(".flick/run"))
            .expect("reservation root")
            .count(),
        0,
        "a typechecking failure cannot leak a reserved generation"
    );
}

#[test]
fn build_watch_follows_changes_creation_and_deletion_with_a_future_file() {
    use std::thread;
    use std::time::{Duration, Instant, UNIX_EPOCH};

    struct Watcher(process::Child);
    impl Drop for Watcher {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn until(child: &mut process::Child, ready: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_mins(1);
        while !ready() {
            assert!(
                child.try_wait().expect("watch status").is_none(),
                "watch exited"
            );
            assert!(
                Instant::now() < deadline,
                "watch did not publish the change"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
    let dir = project(
        "cli_watch_future",
        &[
            ("flick.toml", LIB),
            (
                "src/Main.fl",
                "module Main exposing (answer)\nlet answer = 1\n",
            ),
            (
                "src/Future.fl",
                "module Future exposing (answer)\nlet answer = 0\n",
            ),
        ],
    );
    let set_time = |path: &Path, seconds| {
        fs::File::open(path)
            .expect("source")
            .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
            .expect("timestamp");
    };
    set_time(&dir.join("src/Future.fl"), 4_102_444_800);
    set_time(&dir.join("src/Main.fl"), 1_000);
    let mut watch = Watcher(
        process::Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["build", "--watch"])
            .current_dir(&dir)
            .stdout(Stdio::null())
            .spawn()
            .expect("watch"),
    );
    until(&mut watch.0, || cached(&dir) == 1);
    let original = fs::read(dir.join("dist/Main.js")).expect("initial artifact");
    fs::write(
        dir.join("src/Main.fl"),
        "module Main exposing (answer)\nlet answer = 2\n",
    )
    .expect("change");
    set_time(&dir.join("src/Main.fl"), 2_000);
    until(&mut watch.0, || cached(&dir) == 2);
    assert_ne!(
        fs::read(dir.join("dist/Main.js")).expect("changed artifact"),
        original
    );
    fs::write(
        dir.join("src/New.fl"),
        "module New exposing (answer)\nlet answer = 3\n",
    )
    .expect("create");
    until(&mut watch.0, || cached(&dir) == 3);
    assert!(dir.join("dist/New.js").exists(), "new paths are built");
    fs::remove_file(dir.join("src/New.fl")).expect("delete");
    until(&mut watch.0, || !dir.join("dist/New.js").exists());
    assert_eq!(
        cached(&dir),
        3,
        "deletion restored the previous cached selection"
    );
}

#[cfg(unix)]
#[test]
fn ir_output_is_checked_with_the_entire_publication_before_writing() {
    use std::os::unix::fs::symlink;

    let dir = project(
        "cli_ir_publication_redirect",
        &[
            ("flick.toml", LIB),
            (
                "src/Main.fl",
                "module Main exposing (answer)\nlet answer = 1\n",
            ),
        ],
    );
    fs::create_dir_all(dir.join("dist")).expect("output directory");
    fs::write(dir.join("kept.json"), "owned elsewhere").expect("outside file");
    symlink(dir.join("kept.json"), dir.join("dist/program.ir.json")).expect("IR output symlink");
    let failed = flick_in(&dir, &["build", "--emit-ir", "json"]);
    assert_eq!(code(&failed), Some(1), "{failed:?}");
    assert!(stderr(&failed).contains("FLICK-P013"), "{failed:?}");
    assert_eq!(
        fs::read(dir.join("kept.json")).expect("outside remains"),
        b"owned elsewhere"
    );
    assert!(!dir.join("dist/Main.js").exists(), "no partial publication");
}

#[test]
fn ir_files_are_owned_outputs_without_polluting_the_normal_build_cache() {
    let dir = project(
        "cli_ir_publication_owned",
        &[
            ("flick.toml", LIB),
            (
                "src/Main.fl",
                "module Main exposing (answer)\nlet answer = 1\n",
            ),
        ],
    );
    for (format, name) in [("json", "program.ir.json"), ("cbor", "program.ir.cbor")] {
        let result = flick_in(&dir, &["build", "--emit-ir", format]);
        assert_eq!(code(&result), Some(0), "{result:?}");
        let bytes = fs::read(dir.join("dist").join(name)).expect("IR output");
        let ir = if format == "json" {
            flick_ir::Program::from_json(str::from_utf8(&bytes).expect("JSON"))
        } else {
            flick_ir::Program::from_cbor(&bytes)
        }
        .expect("portable IR");
        assert_eq!(ir.modules.len(), 1);
        assert_eq!(ir.modules[0].name, "Main");
        let result = flick_in(&dir, &["clean"]);
        assert_eq!(code(&result), Some(0), "{result:?}");
        assert!(!dir.join("dist").join(name).exists(), "clean owns IR");
    }
    let result = flick_in(&dir, &["build"]);
    assert_eq!(code(&result), Some(0), "{result:?}");
    assert!(!dir.join("dist/program.ir.json").exists());
    assert!(!dir.join("dist/program.ir.cbor").exists());
}
