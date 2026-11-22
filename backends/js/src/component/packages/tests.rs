use super::*;

#[test]
fn ancestor_walk_stops_at_windows_drive_and_unc_roots_and_boundary() {
    assert_eq!(
        ancestor_candidates("C:/Users/runner/app", "effect", None),
        [
            "C:/Users/runner/app/node_modules/effect/package.json",
            "C:/Users/runner/node_modules/effect/package.json",
            "C:/Users/node_modules/effect/package.json",
            "C:/node_modules/effect/package.json",
        ]
    );
    assert_eq!(
        ancestor_candidates("C:/", "effect", None),
        ["C:/node_modules/effect/package.json"]
    );
    assert_eq!(
        ancestor_candidates("//server/share/work/app", "effect", None),
        [
            "//server/share/work/app/node_modules/effect/package.json",
            "//server/share/work/node_modules/effect/package.json",
            "//server/share/node_modules/effect/package.json",
        ]
    );
    assert_eq!(
        ancestor_candidates(
            "C:/Users/runner/work/repo/app",
            "effect",
            Some("C:/Users/runner/work/repo"),
        ),
        [
            "C:/Users/runner/work/repo/app/node_modules/effect/package.json",
            "C:/Users/runner/work/repo/node_modules/effect/package.json",
        ]
    );

    // `resolve` also stops cleanly at Windows drive roots without emitting `/node_modules/...`.
    let mut known = Known::new(&[]);
    assert!(resolve(&mut known, "C:/Users/runner/app", "effect", true, None).is_err());
    assert!(
        known.needs.iter().all(|p| p.starts_with("C:/")),
        "{:?}",
        known.needs
    );
    assert!(
        !known.needs.iter().any(|p| p.starts_with("/node_modules")),
        "{:?}",
        known.needs
    );
}

#[test]
fn source_roots_rejects_escaping_source_dir() {
    let project_file = ProjectFile {
        path: "package.json".into(),
        real: "/work/app/package.json".into(),
        bytes: br#"{"name":"app","dependencies":{"evil":"1.0.0","good":"1.0.0"}}"#.to_vec(),
    };
    let evil = ProjectFile {
        path: "node_modules/evil/package.json".into(),
        real: "/work/app/node_modules/evil/package.json".into(),
        bytes: br#"{"name":"evil","version":"1.0.0","flick":{"source-dir":"../../../src"}}"#
            .to_vec(),
    };
    let good = ProjectFile {
        path: "node_modules/good/package.json".into(),
        real: "/work/app/node_modules/good/package.json".into(),
        bytes: br#"{"name":"good","version":"1.0.0","flick":{"source-dir":"./src/lib"}}"#.to_vec(),
    };
    let files = [project_file, evil, good];
    let report = packages(
        &files,
        &[],
        &[
            OptionValue {
                key: "root".into(),
                value: "/work/app".into(),
            },
            OptionValue {
                key: "skip-runtime".into(),
                value: "true".into(),
            },
        ],
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "P005" && d.message.contains("flick.source-dir")),
        "{:?}",
        report.diagnostics
    );
    assert!(
        !report.sources.iter().any(|s| s.package == "evil"),
        "{:?}",
        report.sources
    );
    let good_root = report
        .sources
        .iter()
        .find(|s| s.package == "good")
        .expect("valid source root");
    assert_eq!(good_root.root, "node_modules/good/src/lib");
}
