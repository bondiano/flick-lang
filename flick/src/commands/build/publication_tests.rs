use std::{env, fs, process};

use flick_backend::{Artifact as BackendArtifact, ArtifactKind};
use flick_core::build::Extras;
use flick_core::diagnostics::codes;

use super::{BuildPlan, Emission, write_artifacts};
use crate::artifact::Artifact;
use crate::progress::{Mode, Progress};

fn plan(name: &str) -> BuildPlan {
    let root = env::temp_dir().join(format!("flick-publish-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("output")).expect("output directory");
    BuildPlan {
        out_dir: root.join("output"),
        entries: vec![],
        typescript: false,
        go: false,
        suites: vec![],
        extras: Extras::default(),
        release: false,
    }
}

#[test]
fn invalid_paths_are_refused_before_any_file_is_published() {
    let output = plan("escape");
    fs::write(output.out_dir.join("old.js"), "previous build").expect("old output");
    let mut emitted = Emission {
        artifacts: vec![
            Artifact::source("safe.js".into(), "new build".into()),
            Artifact::source("../outside.bin".into(), "escaped".into()),
        ],
        diagnostics: vec![],
    };
    write_artifacts(&output, &mut emitted, &Progress::new(Mode::Hidden));
    assert_eq!(emitted.diagnostics.len(), 1, "invalid output path");
    assert_eq!(emitted.diagnostics[0].code, codes::P013);
    assert_eq!(
        fs::read(output.out_dir.join("old.js")).expect("old output"),
        b"previous build"
    );
    assert!(
        !output.out_dir.join("safe.js").exists(),
        "no partial output"
    );
    assert!(!output.out_dir.join("../outside.bin").exists(), "no escape");
    fs::remove_dir_all(output.out_dir.parent().expect("root")).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn a_symlink_inside_output_cannot_redirect_a_build_to_an_unowned_file() {
    use std::os::unix::fs::symlink;

    let output = plan("symlink");
    let outside = output.out_dir.parent().expect("root").join("outside");
    fs::create_dir(&outside).expect("outside directory");
    fs::write(outside.join("kept.bin"), "owned elsewhere").expect("outside file");
    symlink(&outside, output.out_dir.join("redirect")).expect("symlink");
    let mut emitted = Emission {
        artifacts: vec![
            Artifact::source("safe.js".into(), "new build".into()),
            Artifact::source("redirect/kept.bin".into(), "escaped".into()),
        ],
        diagnostics: vec![],
    };
    write_artifacts(&output, &mut emitted, &Progress::new(Mode::Hidden));
    assert_eq!(emitted.diagnostics.len(), 1, "refuse the symlink");
    assert_eq!(emitted.diagnostics[0].code, codes::P013);
    assert_eq!(
        fs::read(outside.join("kept.bin")).expect("outside file"),
        b"owned elsewhere"
    );
    assert!(
        !output.out_dir.join("safe.js").exists(),
        "no partial output"
    );
    fs::remove_dir_all(output.out_dir.parent().expect("root")).expect("cleanup");
}

#[test]
fn binary_component_artifacts_reach_the_filesystem_without_utf8_conversion() {
    let output = plan("binary");
    let mut emitted = Emission {
        artifacts: vec![
            BackendArtifact {
                id: "data".into(),
                path: "data.bin".into(),
                kind: ArtifactKind::Resource,
                bytes: vec![0, 255, 128, 10],
            }
            .into(),
        ],
        diagnostics: vec![],
    };
    write_artifacts(&output, &mut emitted, &Progress::new(Mode::Hidden));
    assert!(emitted.diagnostics.is_empty(), "{:?}", emitted.diagnostics);
    assert_eq!(
        fs::read(output.out_dir.join("data.bin")).expect("binary output"),
        [0, 255, 128, 10]
    );
    fs::remove_dir_all(output.out_dir.parent().expect("root")).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn cached_executable_artifacts_can_be_launched_after_restoration() {
    use crate::cache::{Signed, load, store};
    use flick_core::project::cache::Key;
    use std::process::Command;

    let output = plan("executable");
    let root = output.out_dir.parent().expect("root");
    let key = Signed::from(Key::default());
    let files = vec![
        BackendArtifact {
            id: "program".into(),
            path: "program".into(),
            kind: ArtifactKind::Executable,
            bytes: b"#!/bin/sh\nexit 7\n".to_vec(),
        }
        .into(),
    ];
    store(root, &key, &files);
    let mut emitted = Emission {
        artifacts: load(root, &key).expect("cached executable"),
        diagnostics: vec![],
    };
    write_artifacts(&output, &mut emitted, &Progress::new(Mode::Hidden));
    assert!(emitted.diagnostics.is_empty(), "{:?}", emitted.diagnostics);
    assert_eq!(
        Command::new(output.out_dir.join("program"))
            .status()
            .expect("launch executable")
            .code(),
        Some(7)
    );
    fs::remove_dir_all(root).expect("cleanup");
}
