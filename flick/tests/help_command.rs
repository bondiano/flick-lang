mod common;

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
        for option in ["--quiet", "--silent", "--verbose"] {
            assert!(help.contains(option), "{help}");
        }
        if command == Some("build") {
            for option in ["--inputs", "--emit", "js", "ts", "go"] {
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
