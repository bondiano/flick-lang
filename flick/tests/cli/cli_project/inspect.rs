use super::*;
use flick_core::iface;

#[test]
fn inspect_prints_each_representation() {
    let dir = project(
        "cli_inspect",
        &[("flick.toml", LIB), ("src/Lib/Api.fl", API)],
    );
    for (args, expected) in [
        (&["inspect", "Lib.Api", "--emit", "ast"][..], "SourceFile"),
        (
            &["inspect", "Lib.Api", "--emit", "typed-ir"],
            "module Lib.Api",
        ),
        (
            &["inspect", "Lib.Api", "--emit", "output"],
            "export const measure",
        ),
        (
            &["inspect", "Lib.Api", "--emit", "output", "--typescript"],
            "export const measure",
        ),
        (
            &["inspect", "Lib.Api", "--emit", "iface"],
            "\"module\": \"Lib.Api\"",
        ),
        (&["inspect", "Lib.Api", "--emit", "mapping"], "\"mappings\""),
    ] {
        let out = flick_in(&dir, args);
        assert_eq!(code(&out), Some(0), "{args:?}: {out:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains(expected), "{args:?}: `{expected}` in\n{text}");
    }
    assert_eq!(
        code(&flick_in(
            &dir,
            &["inspect", "Lib.Nope", "--emit", "output"]
        )),
        Some(2)
    );
}

#[test]
fn inspect_and_build_do_not_publish_a_broken_interface() {
    let broken = "module Lib.Api exposing (value)\nvalue : Int\nlet value = missingValue\n";
    let dir = project(
        "cli_inspect_broken_interface",
        &[("flick.toml", LIB), ("src/Lib/Api.fl", broken)],
    );
    for args in [&["inspect", "Lib.Api", "--emit", "iface"][..], &["build"]] {
        let out = flick_in(&dir, args);
        assert_ne!(code(&out), Some(0), "{args:?}: {out:?}");
        assert!(out.stdout.is_empty(), "{args:?}: {out:?}");
        let errors = String::from_utf8_lossy(&out.stderr);
        assert!(
            errors.contains("FLICK-T001") && errors.contains("missingValue"),
            "{errors}"
        );
        assert!(!dir.join("dist/Lib/Api.iface").exists());
    }
}

#[test]
fn inspect_writes_a_readable_builtin_interface() {
    let dir = project(
        "cli_inspect_builtin_interface",
        &[("flick.toml", LIB), ("src/Lib/Api.fl", API)],
    );
    let output = flick_in(&dir, &["inspect", "Prelude", "--emit", "iface"]);
    assert_eq!(code(&output), Some(0), "{output:?}");
    let interface = iface::read(&output.stdout, "Prelude").expect("valid builtin");
    assert!(!interface.exports.is_empty());
}
