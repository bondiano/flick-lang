mod common;

use common::{flick, stdout};

#[test]
fn version() {
    insta::assert_snapshot!(stdout(&["--version"]), @"flick 0.1.0 (Flick 0.7)");
}

#[test]
fn help() {
    insta::assert_snapshot!(stdout(&["help"]));
    assert_eq!(stdout(&["--help"]), stdout(&["help"]));
}

#[test]
fn help_check() {
    insta::assert_snapshot!(stdout(&["help", "check"]));
}

#[test]
fn help_build() {
    insta::assert_snapshot!(stdout(&["help", "build"]));
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

#[test]
fn help_fmt() {
    insta::assert_snapshot!(stdout(&["help", "fmt"]));
}
