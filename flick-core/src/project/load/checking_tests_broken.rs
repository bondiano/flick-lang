/// `A.Base` with a class over a superclass, an instance and a value that
/// check, then `tail`.
#[cfg(feature = "ide")]
fn base_then(tail: &str) -> String {
    format!(
        "module A.Base exposing (x, Sized(..))\n\
         class Show a => Sized a =\n  size : a -> Float\n\
         instance Sized Float =\n  let size n = n\n\
         x : Float\nlet x = 1\n{tail}"
    )
}

#[cfg(feature = "ide")]
fn with_use(base: &str, user: &str) -> Memory {
    Memory(BTreeMap::from([
        (PathBuf::from("/p/src/A/Base.fl"), base.to_owned()),
        (PathBuf::from("/p/src/A/Use.fl"), user.to_owned()),
    ]))
}

#[cfg(feature = "ide")]
const SIZED_USE: &str = "module A.Use exposing (y, described)\nimport A.Base exposing (x, Sized(..))\n\
                         described : forall a. Sized a => a -> String\n\
                         let described v = show v ++ Float.to_string (size v)\n\
                         y : String\nlet y = described x\n";

#[cfg(feature = "ide")]
#[test]
fn an_error_in_a_module_is_not_repeated_by_its_importers() {
    for (tail, code) in [
        // a body of the wrong type, beside the exports
        ("wrong : Float\nlet wrong = \"text\"\n", "FLICK-T003"),
        // text the parser cannot read, after them
        ("let broken = )\n", "FLICK-S007"),
    ] {
        let (_, diagnostics) = analyzed(&with_use(&base_then(tail), SIZED_USE), None);
        let errors: Vec<_> = (diagnostics.iter())
            .filter(|d| !d.starts_with("FLICK-R014"))
            .collect();
        assert_eq!(errors.len(), 1, "{tail}: {diagnostics:?}");
        assert!(errors[0].starts_with(code), "{tail}: {diagnostics:?}");
    }
}

#[cfg(feature = "ide")]
#[test]
fn an_importer_is_checked_against_what_a_module_with_errors_finished() {
    let misuse = "module A.Use exposing (y)\nimport A.Base exposing (x)\n\
                  y : String\nlet y = x\n";
    let (_, diagnostics) = analyzed(&with_use(&base_then("let broken = )\n"), misuse), None);
    assert!(
        diagnostics.iter().any(|d| d.starts_with("FLICK-T003")),
        "`x` is still a Float: {diagnostics:?}"
    );
}

#[cfg(feature = "ide")]
#[test]
fn an_export_left_unfinished_is_taken_on_trust() {
    // `guess` has no signature, and its body names nothing known
    let base = "module A.Base exposing (guess)\nlet guess = missing\n";
    let user = "module A.Use exposing (y)\nimport A.Base exposing (guess)\n\
                y : String\nlet y = guess\n";
    let (_, diagnostics) = analyzed(&with_use(base, user), None);
    assert!(!diagnostics.is_empty(), "the body of `guess`");
    assert!(
        diagnostics.iter().all(|d| !d.contains("`y`") && !d.contains("String")),
        "{diagnostics:?}"
    );
}
