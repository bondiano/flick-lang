use super::tests::fixture_of;
use crate::diagnostics::Diagnostic;
use crate::syntax::kind::SyntaxKind as K;

#[test]
fn declaration_type_variables_and_service_operations_have_hovers() {
    let source = "module App.Hovers exposing (..)

type Lifecycle

class Functor (f : Type -> Type)

class Functor f => Traversable (f : Type -> Type) =
  traverse : forall c a b r e. (a ->{r; e} b)^c -> (f a ->{r; e} f b)^c

service Setup (s : Region) =
  /// The value belongs to the instance and holds region `s`.
  own : forall a. a ->{} a^{s}
  /// Runs work from mount to unmount.
  start : forall c. Lifecycle^c ->{} Unit
";
    let f = fixture_of(&[("src/App/Hovers.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Hovers");
    let tokens = ide.analysis.by_file(file).expect("module").tree.tokens();
    let check_kind = |start: usize, end: usize, expected: &[(&str, &str)]| {
        for token in &tokens {
            if token.kind != K::Ident || (token.start as usize) < start || token.end as usize > end
            {
                continue;
            }
            let name = token.text(source);
            let Some((_, kind)) = expected.iter().find(|(n, _)| *n == name) else {
                continue;
            };
            let (markdown, range) = ide.hover(file, token.start).expect("variable kind hover");
            assert_eq!(range, (token.start, token.end), "{name}: {markdown}");
            let code = format!("{name} : {kind}");
            assert!(
                markdown.starts_with("```flick\n") && markdown.contains(&code),
                "{name}: {markdown}"
            );
        }
    };
    let class_start = source.find("class Functor f =>").expect("class");
    let traverse_start = source.find("  traverse :").expect("method");
    check_kind(class_start, traverse_start, &[("f", "Type -> Type")]);
    let superclass_f = u32::try_from(class_start + "class Functor ".len()).expect("small");
    assert!(
        ide.hover(file, superclass_f)
            .expect("superclass parameter")
            .0
            .contains("Functor f => f : Type -> Type"),
    );
    let setup_start = source.find("service Setup").expect("service");
    check_kind(
        traverse_start,
        setup_start,
        &[
            ("f", "Type -> Type"),
            ("c", "CaptureRow"),
            ("a", "Type"),
            ("b", "Type"),
            ("r", "ServiceRow"),
            ("e", "ErrorRow"),
        ],
    );
    let end = source.len();
    check_kind(
        setup_start,
        end,
        &[("s", "Region"), ("c", "CaptureRow"), ("a", "Type")],
    );
    for (name, doc) in [
        (
            "own",
            "The value belongs to the instance and holds region `s`.",
        ),
        ("start", "Runs work from mount to unmount."),
    ] {
        let start = u32::try_from(source.find(&format!("  {name} :")).expect("operation") + 2)
            .expect("small");
        let (markdown, range) = ide.hover(file, start).expect("operation hover");
        assert_eq!(
            range,
            (start, start + u32::try_from(name.len()).expect("small"))
        );
        assert!(
            markdown.contains(&format!("Setup.{name} : ")) && !markdown.contains("forall"),
            "{markdown}"
        );
        assert!(markdown.contains(doc), "{markdown}");
    }
}

#[test]
fn constrained_type_variable_hover_shows_required_classes() {
    let source = "module App.Assertions exposing (..)

type Assertion = Pass

equal : (Eq a, Show a) => a -> a -> Assertion
let equal expected actual = Pass
";
    let f = fixture_of(&[("src/App/Assertions.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Assertions");
    let signature = source.find("equal :").expect("signature");
    let end = source.find("\nlet equal").expect("definition");
    for token in ide.analysis.by_file(file).expect("module").tree.tokens() {
        if token.kind != K::Ident
            || token.text(source) != "a"
            || (token.start as usize) < signature
            || token.end as usize > end
        {
            continue;
        }
        let (markdown, range) = ide.hover(file, token.start).expect("constrained variable");
        assert_eq!(range, (token.start, token.end));
        assert!(
            markdown.starts_with("```flick\n(Eq a, Show a) => a : Type\n```"),
            "{markdown}"
        );
        assert!(!markdown.contains("ordinary value types"), "{markdown}");
    }
}

#[test]
fn instance_method_signature_variables_have_kinds_and_constraints() {
    let source = "module App.Folding exposing (..)

class Foldable (f : Type -> Type) =
  fold : forall c a b. (b -> a -> b)^c -> (b^c -> (f a -> b)^c)^c

type Result e a = Err e | Ok a

fold_list : forall c a b. (b -> a -> b)^c -> (b^c -> (List a -> b)^c)^c
foreign pure fold_list from \"runtime\" \"foldList\"
  retention call

instance Foldable (Result e) =
  fold : forall c a b. (b -> a -> b)^c -> (b^c -> (Result e a -> b)^c)^c
  let fold f init value =
    match value with
    | Err _ -> init
    | Ok x -> f init x
";
    let f = fixture_of(&[("src/App/Folding.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Folding");
    let tokens = ide.analysis.by_file(file).expect("module").tree.tokens();
    for (start, end) in [
        (
            source.find("fold_list :").expect("signature"),
            source.find("\nforeign pure fold_list").expect("foreign"),
        ),
        (
            source.find("instance Foldable").expect("instance"),
            source.find("\n  let fold").expect("method body"),
        ),
    ] {
        for token in &tokens {
            if token.kind != K::Ident || (token.start as usize) < start || token.end as usize > end
            {
                continue;
            }
            let kind = match token.text(source) {
                "c" => "CaptureRow",
                "a" | "b" | "e" => "Type",
                _ => continue,
            };
            let (markdown, range) = ide.hover(file, token.start).expect("variable hover");
            assert_eq!(range, (token.start, token.end));
            assert!(
                markdown.starts_with(&format!("```flick\n{} : {kind}", token.text(source))),
                "{markdown}"
            );
        }
    }
}

#[test]
fn outer_error_row_variable_hover_survives_nested_forall() {
    let source = "module App.ClockTest exposing (..)

type Test
service VirtualTime

clock_test : forall e. String -> (forall s. Eff {Scope s, VirtualTime} e Unit) -> Test
foreign pure clock_test from \"runtime\" \"clockTest\"
";
    let f = fixture_of(&[("src/App/ClockTest.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.ClockTest");
    let signature = source.find("clock_test :").expect("signature");
    let end = source.find("\nforeign pure").expect("definition");
    for token in ide.analysis.by_file(file).expect("module").tree.tokens() {
        if token.kind != K::Ident || (token.start as usize) < signature || token.end as usize > end
        {
            continue;
        }
        let (kind, phrase) = match token.text(source) {
            "e" => ("ErrorRow", "errors a computation may fail with"),
            "s" => ("Region", "region identity"),
            _ => continue,
        };
        let (markdown, range) = ide.hover(file, token.start).expect("variable hover");
        assert_eq!(range, (token.start, token.end));
        assert!(
            markdown.starts_with(&format!("```flick\n{} : {kind}\n```", token.text(source))),
            "{markdown}"
        );
        assert!(markdown.contains(phrase), "{markdown}");
    }
}

#[test]
fn a_let_mut_variable_is_one_symbol_with_its_assignments() {
    let source = "module App.Vars exposing (counted)

counted : Int -> Int
let counted n =
  let mut total = n
  total = total + 1
  total
";
    let f = fixture_of(&[("src/App/Vars.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.is_empty(),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Vars");
    let declared = f.at("App.Vars", "total", 0);
    let target = f.at("App.Vars", "total", 1);
    // the declaration, the assignment, the read in its value and the result
    let edits = ide.rename(file, declared + 1, "sum").expect("a variable");
    assert_eq!(edits.len(), 4, "{edits:?}");
    let from_target = ide.rename(file, target + 1, "sum").expect("its assignment");
    assert_eq!(from_target.len(), 4, "{from_target:?}");
    let (markdown, range) = ide.hover(file, target).expect("hover of the assigned name");
    assert_eq!(range, (target, target + 5));
    assert_eq!(markdown, "```flick\nmut total : Int\n```");
    for (at, _) in source.match_indices("total") {
        let at = u32::try_from(at).expect("small source");
        let (markdown, range) = ide.hover(file, at).expect("mutable name hover");
        assert_eq!(range, (at, at + 5));
        assert_eq!(markdown, "```flick\nmut total : Int\n```");
    }
    let (symbol, _) = ide.symbol_at(file, target).expect("symbol");
    let definition = ide.definition(&symbol).expect("definition");
    assert_eq!(definition.start, declared);
}

#[test]
fn mutable_hover_follows_the_binder_through_shadowing() {
    let source = "module App.Vars exposing (counted)
counted : Int -> (Int, Int)
let counted n =
  let mut total = n
  total = total + 1
  let other =
    let total = total
    total
  (total, other)
";
    let f = fixture_of(&[("src/App/Vars.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Vars");
    let expected = [true, true, true, false, true, false, true];
    let occurrences: Vec<_> = source.match_indices("total").collect();
    assert_eq!(occurrences.len(), expected.len());
    for ((at, _), mutable) in occurrences.into_iter().zip(expected) {
        let at = u32::try_from(at).expect("small source");
        let (markdown, range) = ide.hover(file, at).expect("local name hover");
        let prefix = if mutable { "mut " } else { "" };
        assert_eq!(markdown, format!("```flick\n{prefix}total : Int\n```"));
        assert_eq!(range, (at, at + 5));
    }
}

#[test]
fn mutable_function_values_do_not_mark_their_parameters_mutable() {
    let source = "module App.Vars exposing (applied)
applied : Int -> Int
let applied n =
  let mut step : Int -> Int = fun value -> value
  step = fun value -> value + 1
  step n
";
    let f = fixture_of(&[("src/App/Vars.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Vars");
    for (name, expected) in [
        ("step", "```flick\nmut step : Int -> Int\n```"),
        ("value", "```flick\nvalue : Int\n```"),
    ] {
        for (at, _) in source.match_indices(name) {
            let at = u32::try_from(at).expect("small source");
            assert_eq!(ide.hover(file, at).expect("local name hover").0, expected);
        }
    }
}
