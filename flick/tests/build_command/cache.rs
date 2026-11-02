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
