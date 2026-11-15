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
    // the import may give what the parser lost, so it is no unused one
    assert_eq!(diagnostics, ["FLICK-S007 unexpected `)`"]);
}

#[cfg(feature = "ide")]
const PARAMETERIZED_RECOVERY_MAKE: &str = "module A.Make(K : A.Contracts.Source) : A.Contracts.Result exposing (answer)\n\
     \nanswer : Int\nlet answer = K.value\n\
     \nlet _lost = )\n";

#[cfg(feature = "ide")]
const PARAMETERIZED_RECOVERY_USE: &str = "module A.Use exposing (y, bad, health, use_lost)\n\
     import A.Collections.Names exposing (answer, lost)\nimport A.Other exposing (z)\n\
     y : Int\nlet y = answer\n\
     bad : String\nlet bad = answer\n\
     health : Float\nlet health = z\n\
     use_lost : Int\nlet use_lost = lost\n";

#[cfg(feature = "ide")]
const PARAMETERIZED_RECOVERY_FIXED_MAKE: &str = "module A.Make(K : A.Contracts.Source) : A.Contracts.Result exposing (answer)\n\
     answer : Int\nlet answer = K.value\n";

#[cfg(feature = "ide")]
const PARAMETERIZED_RECOVERY_FIXED_USE: &str = "module A.Use exposing (y, health)\n\
     import A.Collections.Names exposing (answer)\nimport A.Other exposing (z)\n\
     y : Int\nlet y = answer\nhealth : Float\nlet health = z\n";

/// A parameterized module, its named application and importer, and a valid
/// neighbouring module. `A.Make`'s `lost` is lost to the stray `)` in its
/// body, so only an incomplete interface lets the importer use it.
#[cfg(feature = "ide")]
fn parameterized_recovery(make: &str, use_text: &str) -> Memory {
    Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/A/Contracts.fl"),
            "module A.Contracts exposing (Source, Result)\n\
             type module Source =\n  value : Int\n\
             type module Result =\n  answer : Int\n"
                .to_owned(),
        ),
        (
            PathBuf::from("/p/src/A/Argument.fl"),
            "module A.Argument exposing (value)\nvalue : Int\nlet value = 41\n".to_owned(),
        ),
        (
            PathBuf::from("/p/src/A/Other.fl"),
            "module A.Other exposing (z)\nz : Float\nlet z = 2\n".to_owned(),
        ),
        (PathBuf::from("/p/src/A/Make.fl"), make.to_owned()),
        (
            PathBuf::from("/p/src/A/Collections.fl"),
            "module A.Collections exposing (Names)\nmodule Names = A.Make(A.Argument)\n"
                .to_owned(),
        ),
        (PathBuf::from("/p/src/A/Use.fl"), use_text.to_owned()),
    ]))
}

#[cfg(feature = "ide")]
#[test]
fn a_malformed_parameterized_module_is_taken_on_trust_by_its_importer() {
    use crate::diagnostics::{Diagnostic, codes};

    let host = parameterized_recovery(
        PARAMETERIZED_RECOVERY_MAKE,
        PARAMETERIZED_RECOVERY_USE,
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/A/Use.fl")],
    );
    let file_of = |d: &Diagnostic| d.span.map(|s| sources.get(s.file).path.clone());
    let make = "/p/src/A/Make.fl";
    let primary = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::S007 && file_of(d).as_deref() == Some(make))
        .expect("the template's parse error keeps its file");
    let span = primary.span.expect("the parse error keeps its span");
    assert_eq!(&sources.get(span.file).text[span.range()], ")");
    assert!(
        analysis
            .diagnostics
            .iter()
            .all(|d| d.code == codes::S007 || d.code == codes::T003),
        "only the parse error and the deliberate mismatch: {:?}",
        analysis.diagnostics
    );
    for code in [
        codes::R004,
        codes::R005,
        codes::T001,
        codes::T061,
        codes::C002,
        codes::C011,
    ] {
        assert!(
            !analysis.diagnostics.iter().any(|d| d.code == code),
            "{code} is a recovery cascade: {:?}",
            analysis.diagnostics
        );
    }
    // `answer` keeps its type: only the deliberate misuse is a mismatch.
    let mismatches: Vec<_> = (analysis.diagnostics.iter())
        .filter(|d| d.code == codes::T003)
        .collect();
    assert_eq!(mismatches.len(), 1, "{:?}", analysis.diagnostics);
    assert_eq!(
        file_of(mismatches[0]).as_deref(),
        Some("/p/src/A/Use.fl")
    );
    // the valid neighbour is neither incomplete nor dragged into the trust
    let other = "/p/src/A/Other.fl";
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|d| file_of(d).as_deref() == Some(other)),
        "{:?}",
        analysis.diagnostics
    );
    assert!(analysis.module("A.Other").is_some());
}

#[cfg(feature = "ide")]
#[test]
fn an_edited_parameterized_module_keeps_recovery_across_cache_reuse() {
    let broken = parameterized_recovery(
        PARAMETERIZED_RECOVERY_MAKE,
        PARAMETERIZED_RECOVERY_USE,
    );
    let (_, fresh) = analyzed(&broken, None);
    assert!(
        fresh.iter().any(|d| d.starts_with("FLICK-S007")),
        "{fresh:?}"
    );
    let mut cache = ModuleCache::default();
    let (_, first) = analyzed(&broken, Some(&mut cache));
    assert_eq!(first, fresh);
    let (_, warm) = analyzed(&broken, Some(&mut cache));
    assert_eq!(warm, fresh);

    // Fixing the template must invalidate the cached incomplete interfaces,
    // including those of its application and importer.
    let fixed = parameterized_recovery(
        PARAMETERIZED_RECOVERY_FIXED_MAKE,
        PARAMETERIZED_RECOVERY_FIXED_USE,
    );
    let (_, after) = analyzed(&fixed, Some(&mut cache));
    assert!(after.is_empty(), "{after:?}");

    // Breaking it again must bring the recovery back, not a stale clean run.
    let (_, again) = analyzed(&broken, Some(&mut cache));
    assert_eq!(again, fresh);
}
