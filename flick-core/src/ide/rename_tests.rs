use super::tests::{Fixture, fixture_of};
use crate::diagnostics::Severity;

const NAMED: &str = "module App.Named exposing (Named(..), describe)
class Named a =
  describe : a -> String
  describe : Show a => a -> String
  let describe x = show x
instance Named Int
";

const OTHER: &str = "module App.Other exposing (Other(..), describe)
class Other a =
  describe : a -> String
instance Other Bool =
  let describe _ = \"other\"
";

const FACADE: &str = "module App.Facade exposing (Named(..), describe)
import App.Named exposing (Named(..), describe)
";

const USE: &str = "module App.Use exposing (..)
import App.Facade as N exposing (Named(..), describe)
import App.Other as O
instance Named String =
  describe : String -> String
  let describe x = x
generic : Named a => a -> String
let generic x = describe x
let concrete = N.describe 1
let other = O.describe true
local : String -> String
let local describe = describe
let record = { describe = \"field\" }
let field = record.describe
";

fn assert_clean(f: &Fixture) {
    let errors: Vec<_> = f
        .ide()
        .analysis
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn class_method_rename_links_defaults_instances_imports_and_calls_only_of_its_class() {
    let f = fixture_of(&[
        ("src/App/Named.fl", NAMED),
        ("src/App/Other.fl", OTHER),
        ("src/App/Facade.fl", FACADE),
        ("src/App/Use.fl", USE),
    ]);
    assert_clean(&f);
    let ide = f.ide();
    let expected: Vec<_> = [
        ("App.Named", NAMED, 4),
        ("App.Facade", FACADE, 2),
        ("App.Use", USE, 5),
    ]
    .into_iter()
    .flat_map(|(module, text, count)| {
        let file = f.file(module);
        text.match_indices("describe")
            .take(count)
            .map(move |(at, _)| {
                let at = u32::try_from(at).expect("small source");
                (file, (at, at + 8))
            })
    })
    .collect();
    let mut expected = expected;
    expected.sort();
    // Every class/instance signature, default/implementation, import and call
    // starts the same rename. Fields, locals and the other class stay intact.
    for &(file, (start, end)) in &expected {
        let (symbol, range, capital) = ide.renamable(file, start + 1).expect("method");
        assert_eq!(range, (start, end));
        assert!(!capital);
        assert_eq!(
            ide.rename(file, start + 1, "render").expect("rename"),
            expected
        );
        // Navigation may still choose the concrete instance, not the class.
        assert!(ide.definition(&symbol).is_some());
    }
}

#[test]
fn class_method_rename_refuses_ambiguous_imports_and_duplicate_declarations() {
    let ambiguous = "module App.Use exposing (..)\nimport App.Named exposing (..)\nimport App.Other exposing (..)\nlet result = describe true\n";
    let f = fixture_of(&[
        ("src/App/Named.fl", NAMED),
        ("src/App/Other.fl", OTHER),
        ("src/App/Use.fl", ambiguous),
    ]);
    let ide = f.ide();
    assert!(
        ide.rename(f.file("App.Use"), f.at("App.Use", "describe", 0), "render")
            .is_err()
    );
    assert!(
        ide.rename(
            f.file("App.Named"),
            f.at("App.Named", "describe :", 0),
            "render"
        )
        .is_err(),
        "do not rename a signature while leaving an ambiguous call behind"
    );

    let duplicate = "module App.Bad exposing (..)\nclass First a =\n  same : a -> String\nclass Second a =\n  same : a -> String\n";
    let f = fixture_of(&[("src/App/Bad.fl", duplicate)]);
    assert!(
        f.ide()
            .rename(f.file("App.Bad"), f.at("App.Bad", "same", 0), "render")
            .is_err()
    );
}

#[test]
fn class_method_rename_refuses_external_classes_and_unmapped_dictionary_fields() {
    let source = "module App.Main exposing (..)\ntype Token = Token\ninstance Show Token =\n  let show _ = \"token\"\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    assert!(
        f.ide()
            .rename(
                f.file("App.Main"),
                f.at("App.Main", "show", 0) + 1,
                "render"
            )
            .is_err()
    );
    let source = "module App.Main exposing (..)\nclass Describe a =\n  describe : a -> String\nlet dictionary = Describe.dictionary { describe = fun _ -> \"value\" }\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    assert!(
        !f.ide().analysis.modules[0]
            .typing
            .dictionary_constructors
            .is_empty()
    );
    assert!(
        f.ide()
            .rename(
                f.file("App.Main"),
                f.at("App.Main", "describe :", 0),
                "render"
            )
            .is_err(),
        "dictionary field labels have no exact method mapping"
    );
}

#[test]
fn class_method_rename_refuses_collisions_even_in_importers_without_old_name_uses() {
    for exposing in ["(..)", "(Named(..))"] {
        let user = format!(
            "module App.Use exposing (..)\nimport App.Named exposing {exposing}\nrender : Int -> String\nlet render _ = \"existing\"\nlet result = render 1\n"
        );
        let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", &user)]);
        assert_clean(&f);
        assert!(
            f.ide()
                .rename(
                    f.file("App.Named"),
                    f.at("App.Named", "describe :", 0) + 1,
                    "render"
                )
                .is_err(),
            "{exposing}: a new imported method must not collide even without old calls"
        );
    }
}

#[test]
fn class_method_rename_refuses_capture_keywords_and_a_member_of_the_wrong_class() {
    let user = "module App.Use exposing (..)\nimport App.Named exposing (Named, describe)\ncall : Int -> String\nlet call render = describe render\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    for name in ["render", "if", "_", "Show", "with space"] {
        assert!(
            f.ide()
                .rename(
                    f.file("App.Named"),
                    f.at("App.Named", "describe :", 0) + 1,
                    name
                )
                .is_err(),
            "{name}"
        );
    }
    let bad = "module App.Bad exposing (..)\nclass First a =\n  first : a -> String\nclass Second a =\n  second : a -> String\ninstance First Int =\n  let second _ = \"wrong class\"\n";
    let f = fixture_of(&[("src/App/Bad.fl", bad)]);
    assert!(
        f.ide()
            .rename(
                f.file("App.Bad"),
                f.at("App.Bad", "second _", 0) + 1,
                "render"
            )
            .is_err()
    );
}

#[test]
fn class_method_rename_refuses_structural_packs_with_unmapped_method_fields() {
    let user = "module App.Use exposing (..)\nimport App.Named as N exposing (Named)\ntype module Sig =\n  class Named = N.Named\n  describe : Named a => a -> String\npacked : Module Sig\nlet packed = pack N as Sig\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    assert!(
        f.ide()
            .analysis
            .module("App.Use")
            .expect("use module")
            .typing
            .packs
            .values()
            .flatten()
            .any(|(_, module, name)| module == "App.Named" && name == "describe")
    );
    assert!(
        f.ide()
            .rename(
                f.file("App.Named"),
                f.at("App.Named", "describe :", 0) + 1,
                "render"
            )
            .is_err(),
        "do not break an independent structural contract"
    );
}

#[test]
fn class_method_rename_reaches_inline_namespace_signatures_exposures_and_calls() {
    let source = "module App.Main exposing (..)\nmodule Child exposing (Named(..), describe) =\n  class Named a =\n    describe : a -> String\n  instance Named Int =\n    let describe _ = \"child\"\nlet result = Child.describe 1\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    assert_clean(&f);
    let expected: Vec<_> = source
        .match_indices("describe")
        .map(|(at, _)| {
            let at = u32::try_from(at).expect("small");
            (f.file("App.Main"), (at, at + 8))
        })
        .collect();
    for &(file, (start, _)) in &expected {
        assert_eq!(
            f.ide()
                .rename(file, start + 1, "render")
                .expect("inline method"),
            expected
        );
    }
}

#[test]
fn class_method_rename_refuses_another_method_name_without_declaration_type_metadata() {
    let source = "module App.Main exposing (..)\nclass First a =\n  first : a -> String\nclass Second a =\n  second : a -> String\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    assert_clean(&f);
    assert!(
        f.ide()
            .rename(
                f.file("App.Main"),
                f.at("App.Main", "first :", 0) + 1,
                "second"
            )
            .is_err(),
        "method names share the value namespace even without a type at the declaration token"
    );
}

#[test]
fn class_method_rename_ignores_names_in_unrelated_sibling_namespaces() {
    let source = "module App.Main exposing (..)\nmodule Child exposing (Named(..), describe) =\n  class Named a =\n    describe : a -> String\n  instance Named Int =\n    let describe _ = \"child\"\nmodule Sibling exposing (render) =\n  let render = \"unrelated\"\nlet result = Child.describe 1\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    assert_clean(&f);
    let file = f.file("App.Main");
    let edits = f
        .ide()
        .rename(file, f.at("App.Main", "describe :", 0) + 1, "render")
        .expect("sibling has a separate scope");
    assert_eq!(edits.len(), 4);
    let renamed = source.replace("describe", "render");
    assert_clean(&fixture_of(&[("src/App/Main.fl", &renamed)]));
}

#[test]
fn class_method_rename_refuses_capture_by_lexically_opened_module_names() {
    let user = "module App.Use exposing (..)\nimport App.Named exposing (describe)\nview : Ui.View Unit\nlet view = Ui.view (fun () -> ui { text (describe 1) })\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    assert!(
        f.ide()
            .rename(
                f.file("App.Named"),
                f.at("App.Named", "describe :", 0) + 1,
                "text"
            )
            .is_err(),
        "Ui.text would capture the method call inside ui"
    );
    assert!(
        f.ide()
            .rename(
                f.file("App.Named"),
                f.at("App.Named", "describe :", 0) + 1,
                "describe_item"
            )
            .is_ok(),
        "a fresh name remains safe in the opened scope"
    );
    let qualified = "module App.Use exposing (..)\nimport App.Named as N\nview : Ui.View Unit\nlet view = Ui.view (fun () -> ui { text (N.describe 1) })\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", qualified)]);
    assert_clean(&f);
    assert_eq!(
        f.ide()
            .rename(
                f.file("App.Named"),
                f.at("App.Named", "describe :", 0) + 1,
                "text"
            )
            .expect("qualified calls cannot be captured")
            .len(),
        5
    );
    let named = NAMED.replace("describe", "text");
    let user = qualified.replace("N.describe", "N.text");
    assert_clean(&fixture_of(&[
        ("src/App/Named.fl", &named),
        ("src/App/Use.fl", &user),
    ]));
}

#[test]
fn class_method_rename_refuses_capture_of_implicit_lexical_parent_bindings() {
    let source = "module App.Main exposing (..)\nrender : Int -> String\nlet render _ = \"parent\"\nmodule Child exposing (Named(..), describe, result) =\n  class Named a =\n    describe : a -> String\n  instance Named Int =\n    let describe _ = \"child\"\n  let result = render 1\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    assert_clean(&f);
    assert!(
        f.ide()
            .rename(
                f.file("App.Main"),
                f.at("App.Main", "describe :", 0) + 1,
                "render"
            )
            .is_err(),
        "an unchanged reference to the lexical parent's render must not start calling the child method"
    );
    let source = "module App.Main exposing (..)\nmodule Child exposing (Named(..), describe) =\n  class Named a =\n    describe : a -> String\n  instance Named Int =\n    let describe _ = \"child\"\nrender : Int -> String\nlet render _ = \"declared after the child\"\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    assert_clean(&f);
    assert!(
        f.ide()
            .rename(
                f.file("App.Main"),
                f.at("App.Main", "describe :", 0) + 1,
                "render"
            )
            .is_ok(),
        "a later parent declaration is not in the child's lexical prefix"
    );
    assert_clean(&fixture_of(&[(
        "src/App/Main.fl",
        &source.replace("describe", "render"),
    )]));
}

#[test]
fn class_method_rename_checks_inherited_imports_even_without_method_calls() {
    let user = "module App.Use exposing (..)\nimport App.Named exposing (Named(..))\nmodule Child exposing (render) =\n  render : Int -> String\n  let render _ = \"child\"\n";
    let f = fixture_of(&[("src/App/Named.fl", NAMED), ("src/App/Use.fl", user)]);
    assert_clean(&f);
    assert!(
        f.ide()
            .rename(
                f.file("App.Named"),
                f.at("App.Named", "describe :", 0) + 1,
                "render"
            )
            .is_err(),
        "implicit inherited imports also gain the new method name"
    );
}
