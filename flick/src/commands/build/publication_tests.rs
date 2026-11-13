use crate::backends::Selected;
use std::{env, fs, process};

use flick_backend::{Artifact as BackendArtifact, ArtifactKind};
use flick_core::build::Extras;
use flick_core::diagnostics::codes;

use super::{BuildPlan, Emission, write_artifacts};
use crate::artifact::Artifact;
use crate::progress::{Mode, Progress};

fn plan(name: &str) -> BuildPlan {
    let root = fs::canonicalize(env::temp_dir())
        .expect("canonical temporary directory")
        .join(format!("flick-publish-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("output")).expect("output directory");
    BuildPlan {
        purpose: super::Purpose::Build,
        backend: Selected::Native,
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

#[test]
fn binary_metadata_remains_opaque_to_text_emit_hooks() {
    let metadata = Artifact {
        path: "metadata.cbor".into(),
        contents: vec![0xa1, 0x61, 0x78, 0xf5],
        kind: ArtifactKind::Metadata,
    };
    assert_eq!(metadata.text(), Ok(None));
    assert!(
        metadata.is_binary(),
        "hook replacement retains opaque metadata"
    );
    let json = Artifact::source("metadata.json".into(), "{}".into());
    assert!(
        !json.is_binary(),
        "text metadata still participates in hooks"
    );
    assert_eq!(json.text().expect("text").expect("JSON").contents, "{}");
    let invalid_source = Artifact {
        kind: ArtifactKind::Source,
        ..metadata
    };
    assert!(invalid_source.text().is_err(), "source text must be UTF-8");
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

#[cfg(unix)]
#[test]
fn host_tool_outputs_are_checked_before_publishing_source_files() {
    use flick_core::build;
    use std::os::unix::fs::symlink;

    for go in [true, false] {
        let mut output = plan(if go {
            "go-tool-output"
        } else {
            "schema-tool-output"
        });
        output.go = go;
        output.extras.schema_aot = !go;
        output.entries.push(("main".into(), "Main".into()));
        let name = if go {
            build::go::executable("main")
        } else {
            build::schema_aot_file("main")
        };
        let outside = output.out_dir.parent().expect("root").join("kept.bin");
        fs::write(&outside, "owned elsewhere").expect("outside file");
        symlink(&outside, output.out_dir.join(name)).expect("output symlink");
        fs::write(output.out_dir.join("safe.js"), "previous build").expect("old output");
        let mut emitted = Emission {
            artifacts: vec![Artifact::source("safe.js".into(), "new build".into())],
            diagnostics: vec![],
        };
        write_artifacts(&output, &mut emitted, &Progress::new(Mode::Hidden));
        assert_eq!(
            fs::read(output.out_dir.join("safe.js")).expect("old remains"),
            b"previous build"
        );
        assert_eq!(emitted.diagnostics[0].code, codes::P013);
        assert_eq!(
            fs::read(outside).expect("outside remains"),
            b"owned elsewhere"
        );
        fs::remove_dir_all(output.out_dir.parent().expect("root")).expect("cleanup");
    }
}

#[cfg(unix)]
#[test]
fn redirected_output_roots_and_hardlinks_cannot_modify_unowned_files() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    for case in ["root-link", "parent-link", "hard-link"] {
        let mut output = plan(case);
        let root = output.out_dir.parent().expect("root").to_path_buf();
        let outside = root.join("outside");
        let target = if case == "parent-link" {
            outside.join("nested")
        } else {
            outside.clone()
        };
        fs::create_dir_all(&target).expect("outside directory");
        fs::write(target.join("kept.bin"), b"kept").expect("outside file");
        fs::set_permissions(target.join("kept.bin"), fs::Permissions::from_mode(0o600))
            .expect("permissions");
        fs::write(target.join("safe.js"), "previous build").expect("outside source");
        if case == "hard-link" {
            fs::hard_link(target.join("kept.bin"), output.out_dir.join("kept.bin"))
                .expect("hard link");
            fs::write(output.out_dir.join("safe.js"), "previous build").expect("old output");
        } else {
            fs::remove_dir(&output.out_dir).expect("empty output");
            symlink(&outside, &output.out_dir).expect("redirect output");
            if case == "parent-link" {
                output.out_dir.push("nested");
            }
        }
        let mut emitted = Emission {
            artifacts: vec![
                Artifact::source("safe.js".into(), "new build".into()),
                Artifact {
                    path: "kept.bin".into(),
                    contents: b"kept".to_vec(),
                    kind: ArtifactKind::Executable,
                },
            ],
            diagnostics: vec![],
        };
        write_artifacts(&output, &mut emitted, &Progress::new(Mode::Hidden));
        assert_eq!(emitted.diagnostics.len(), 1, "refuse {case}");
        assert_eq!(emitted.diagnostics[0].code, codes::P013);
        assert_eq!(
            fs::read(output.out_dir.join("safe.js")).expect("old remains"),
            b"previous build"
        );
        assert_eq!(
            fs::read(target.join("kept.bin")).expect("outside remains"),
            b"kept"
        );
        assert_eq!(
            fs::metadata(target.join("kept.bin"))
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
