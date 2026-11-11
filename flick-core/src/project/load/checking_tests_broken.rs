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

#[cfg(feature = "ide")]
#[test]
fn an_error_beside_the_exports_checks_no_other_module_again() {
    // `A.Other` is checked after `A.Base` and imports none of it
    let project = |base: &str| {
        let user = "module A.Use exposing (y)\nimport A.Base exposing (x)\nimport A.Other exposing (z)\n\
                    y : Float\nlet y = x + z\n";
        let mut host = with_use(base, user);
        let other = "module A.Other exposing (z)\nimport A.Leaf exposing (leaf)\nz : Float\nlet z = leaf\n";
        let leaf = "module A.Leaf exposing (leaf)\nleaf : Float\nlet leaf = 1\n";
        host.0.insert(PathBuf::from("/p/src/A/Other.fl"), other.to_owned());
        host.0.insert(PathBuf::from("/p/src/A/Leaf.fl"), leaf.to_owned());
        host
    };
    let mut cache = ModuleCache::default();
    let (clean, _) = analyzed(&project(&base_then("")), Some(&mut cache));
    let broken = project(&base_then("wrong : Float\nlet wrong = \"text\"\n"));
    let (warm, diagnostics) = analyzed(&broken, Some(&mut cache));
    assert!(Arc::ptr_eq(&clean["A.Other"], &warm["A.Other"]));
    assert!(Arc::ptr_eq(&clean["A.Use"], &warm["A.Use"]));
    assert!(!Arc::ptr_eq(&clean["A.Base"], &warm["A.Base"]));
    assert_eq!(diagnostics, analyzed(&broken, None).1);
    // nor does the fix of the error
    let (fixed, _) = analyzed(&project(&base_then("")), Some(&mut cache));
    assert!(Arc::ptr_eq(&clean["A.Other"], &fixed["A.Other"]));
    assert!(Arc::ptr_eq(&clean["A.Use"], &fixed["A.Use"]));
}

#[cfg(feature = "ide")]
#[test]
fn what_the_parser_lost_of_a_module_its_importers_take_on_trust() {
    // the stray `)` costs the class its body, the method `lift` with it
    let base = "module A.Base exposing (Trans(..), x)\n\
                class Trans (t : Type -> Type)) =\n  lift : forall a. a -> t a\n\
                x : Float\nlet x = 1\n";
    // `w` asks for a `Make` that only the type of `lift` would choose
    let user = "module A.Use exposing (Box, y, z, w)\n\
                import A.Base exposing (Trans(..), x, lift)\nimport A.Base as B\n\
                type Box a = Box a\n\
                instance Trans Box =\n  let lift v = Box v\n\
                class Make a =\n  make : a\ninstance Make Float =\n  let make = 1\n\
                y : Box Float\nlet y = lift x\n\
                z : Box Float\nlet z = B.lift x\n\
                w : Box Float\nlet w = lift make\n";
    let (_, diagnostics) = analyzed(&with_use(base, user), None);
    assert_eq!(diagnostics, ["FLICK-S007 unexpected `)`"]);
}

#[cfg(feature = "ide")]
#[test]
fn an_importer_re_exports_on_trust_a_name_the_parser_lost() {
    let base = "module A.Base exposing (..)\n\
                class Trans (t : Type -> Type)) =\n  lift : forall a. a -> t a\n";
    let user = "module A.Use exposing (lift)\nimport A.Base exposing (..)\n";
    let (_, diagnostics) = analyzed(&with_use(base, user), None);
    // a re-export of a name taken on trust does not count as a use of the import
    let errors: Vec<_> = (diagnostics.iter())
        .filter(|d| !d.starts_with("FLICK-R012"))
        .collect();
    assert_eq!(errors, ["FLICK-S007 unexpected `)`"]);
}
