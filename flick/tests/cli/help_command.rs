use crate::common;

use common::{flick, stdout};

#[test]
fn version() {
    insta::assert_snapshot!(stdout(&["--version"]), @"flick 0.1.0");
}

#[test]
fn help_describes_global_and_build_options() {
    for command in [None, Some("check"), Some("build"), Some("fmt")] {
        let (help, flag) = match command {
            Some(command) => (stdout(&["help", command]), stdout(&[command, "--help"])),
            None => (stdout(&["help"]), stdout(&["--help"])),
        };
        assert_eq!(help, flag);
        for option in ["--quiet", "--silent", "--verbose", "--timings"] {
            assert!(help.contains(option), "{help}");
        }
        if command == Some("build") {
            for option in [
                "--inputs",
                "--target",
                "Options of `flick/js`",
                "--typescript",
                "--declarations",
                "--runtime",
            ] {
                assert!(help.contains(option), "{help}");
            }
        }
    }
    assert_eq!(
        flick(&["--silent", "--verbose", "build"]).status.code(),
        Some(2)
    );
}

#[test]
fn help_lists_std_schema_export_as_contributed_and_not_core() {
    let help = stdout(&["--help"]);
    let core_section = help
        .split("Commands of `std`:")
        .next()
        .expect("core help before contributed commands");
    assert!(
        !core_section
            .lines()
            .any(|line| line.trim_start().starts_with("schema")),
        "schema must not remain a core command:\n{core_section}"
    );
    assert!(
        help.contains("Commands of `std`:") && help.contains("schema export"),
        "expected contributed std commands in --help:\n{help}"
    );

    for args in [
        &["schema", "export", "--help"][..],
        &["std:schema", "export", "--help"][..],
    ] {
        let out = flick(args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("--target") && text.contains("--out"),
            "{args:?}: {text}"
        );
    }

    let doc_help = stdout(&["doc", "--help"]);
    assert!(
        doc_help.contains("--check-examples") && doc_help.contains("--target"),
        "{doc_help}"
    );
}

#[test]
fn bad_invocation_exits_2() {
    assert_eq!(flick(&["frobnicate"]).status.code(), Some(2));
    assert_eq!(flick(&["check"]).status.code(), Some(2));
    assert_eq!(
        flick(&["--color", "sometimes", "check", "x.fl"])
            .status
            .code(),
        Some(2)
    );
}
