use super::tests::fixture_of;
use super::{Symbol, Target};
use crate::diagnostics::Diagnostic;

#[test]
fn alias_declaration_hover_covers_parameters_without_widening_symbol_ranges() {
    let source = "module App.Plan exposing (plan, ready, pair)

type Identity a
type ReaderT r (m : Type -> Type) a
type StateT s (m : Type -> Type) a
type MaybeT (m : Type -> Type) a

type alias Config = { budget : Number; overhead : Number }
type alias PlanState = { remaining : Number; completed : List String }
/// A reusable stack of independent effects.
type alias Planner a = MaybeT (StateT PlanState (ReaderT Config Identity)) a
type alias Ready = Planner Number
type alias Pair a b = (a, b)

plan : Planner Number -> Planner Number
let plan value = value

ready : Ready -> Ready
let ready value = value

pair : Pair String Number -> Pair String Number
let pair value = value
";
    let f = fixture_of(&[("src/App/Plan.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Plan");
    for (name, header, declaration) in [
        (
            "Planner",
            "Planner a",
            "type alias Planner a = MaybeT (StateT PlanState (ReaderT Config Identity)) a",
        ),
        ("Ready", "Ready", "type alias Ready = Planner Number"),
        ("Pair", "Pair a b", "type alias Pair a b = (a, b)"),
    ] {
        let occurrences: Vec<_> = source
            .match_indices(name)
            .map(|(start, _)| {
                let start = u32::try_from(start).expect("small");
                (
                    file,
                    (start, start + u32::try_from(name.len()).expect("small")),
                )
            })
            .collect();
        let (_, declared) = occurrences[0];
        let (symbol, symbol_range) = ide.symbol_at(file, declared.0 + 1).expect("alias symbol");
        assert_eq!(symbol_range, declared);
        let definition = ide.definition(&symbol).expect("alias definition");
        assert_eq!(definition.target, Target::File(file));
        assert_eq!((definition.start, definition.end), declared);
        assert_eq!(
            ide.renamable(file, declared.0 + 1)
                .expect("renamable alias")
                .1,
            declared
        );
        assert_eq!(ide.references(&symbol), occurrences);
        assert_eq!(
            ide.rename(file, declared.0 + 1, "Renamed")
                .expect("alias rename"),
            occurrences
        );
        for (index, (_, identifier)) in occurrences.iter().enumerate() {
            let (markdown, range) = ide.hover(file, identifier.0 + 1).expect("alias hover");
            assert!(
                markdown.starts_with(&format!("```flick\n{declaration}\n```")),
                "{markdown}"
            );
            let expected = if index == 0 {
                (
                    declared.0,
                    declared.0 + u32::try_from(header.len()).expect("small"),
                )
            } else {
                *identifier
            };
            assert_eq!(range, expected, "{name} occurrence {index}");
        }
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn module_documentation_reaches_imports_aliases_qualifiers_and_headers() {
    let reader = "/// Reader computations share one environment.
/// Running one supplies that environment.

module App.ReaderT exposing (Box(..), run)

type Box = Box

run : Number -> Number
let run value = value
";
    let plain = "/// Ordinary qualified module documentation.
module App.Plain exposing (run)

run : Number -> Number
let run value = value
";
    let main = "/// Module declarations have documentation too.
module App.Main exposing (..)

import App.ReaderT as ReaderT
import App.Plain

read : Number -> Number
let read value = ReaderT.run value

plain : Number -> Number
let plain value = App.Plain.run value

box : ReaderT.Box -> ReaderT.Box
let box value = value
";
    let f = fixture_of(&[
        ("src/App/ReaderT.fl", reader),
        ("src/App/Plain.fl", plain),
        ("src/App/Main.fl", main),
    ]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let reader_docs =
        "Reader computations share one environment.\nRunning one supplies that environment.";
    for (file_module, needle, module, prefix, docs) in [
        (
            "App.Main",
            "App.ReaderT as",
            "App.ReaderT",
            "App.ReaderT",
            reader_docs,
        ),
        (
            "App.Main",
            "ReaderT\n",
            "App.ReaderT",
            "ReaderT",
            reader_docs,
        ),
        (
            "App.Main",
            "ReaderT.run",
            "App.ReaderT",
            "ReaderT",
            reader_docs,
        ),
        (
            "App.Main",
            "ReaderT.Box",
            "App.ReaderT",
            "ReaderT",
            reader_docs,
        ),
        (
            "App.Main",
            "App.Plain\n",
            "App.Plain",
            "App.Plain",
            "Ordinary qualified module documentation.",
        ),
        (
            "App.Main",
            "App.Plain.run",
            "App.Plain",
            "App.Plain",
            "Ordinary qualified module documentation.",
        ),
        (
            "App.ReaderT",
            "App.ReaderT exposing",
            "App.ReaderT",
            "App.ReaderT",
            reader_docs,
        ),
        (
            "App.Main",
            "App.Main exposing",
            "App.Main",
            "App.Main",
            "Module declarations have documentation too.",
        ),
    ] {
        let file = f.file(file_module);
        let start = f.at(file_module, needle, 0);
        let expected_range = (start, start + u32::try_from(prefix.len()).expect("small"));
        for offset in [start + 1, expected_range.1 - 1] {
            assert_eq!(
                ide.symbol_at(file, offset),
                Some((Symbol::Module(module.into()), expected_range)),
                "{file_module}: {needle}"
            );
            let (markdown, range) = ide.hover(file, offset).expect("module hover");
            assert_eq!(range, expected_range);
            assert_eq!(
                markdown,
                format!("```flick\nmodule {module}\n```\n\n{docs}")
            );
        }
    }
}

#[test]
fn signature_hover_keeps_the_written_lines() {
    let source = "module App.Range exposing (between)

/// A range check.
between : {
  /// The lower bound.
  minimum : Number;
  maximum : Number;
} ->
  Bool
let between _ = true
";
    let f = fixture_of(&[("src/App/Range.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Range");
    let at = u32::try_from(source.find("between :").expect("signature")).expect("small");
    let (markdown, _) = ide.hover(file, at + 1).expect("hover");
    assert!(
        markdown.starts_with(
            "```flick\nbetween : {\n  /// The lower bound.\n  minimum : Number;\n  maximum : Number;\n} ->\n  Bool\n```"
        ),
        "{markdown}"
    );
}
