use super::*;

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
