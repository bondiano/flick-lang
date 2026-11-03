use super::tests::fixture_of;
use super::{Symbol, Target};
use crate::diagnostics::Diagnostic;

#[test]
fn private_recursive_type_and_constructor_hover_uses_local_declarations() {
    let source = "module App.Main exposing (make, example)

type Tree a = Leaf a | Branch (List (Tree a))
  deriving Schema

type Expr = Num Float | Neg Expr | Block Statements
  deriving Schema

type Statements = Statements (List Expr)
  deriving Schema

make : Tree Float
let make = Branch [Leaf 1]

example : Expr
let example = Block (Statements [Neg (Num 1)])
";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Main");
    for (needle, expected) in [
        ("Tree a =", "type Tree : Type -> Type"),
        ("Tree a))", "type Tree : Type -> Type"),
        ("Expr =", "type Expr : Type"),
        ("Expr | Block", "type Expr : Type"),
        ("Statements =", "type Statements : Type"),
        ("Leaf a", "Leaf :"),
        ("Branch (", "Branch :"),
        ("Num Float", "Num :"),
        ("Neg Expr", "Neg :"),
        ("Block Statements", "Block :"),
        ("Statements (", "Statements :"),
        ("Branch [", "Branch :"),
        ("Leaf 1", "Leaf :"),
        ("Block (", "Block :"),
    ] {
        let start = u32::try_from(source.find(needle).expect("hover target")).expect("small");
        let (markdown, range) = ide.hover(file, start + 1).expect(needle);
        assert!(markdown.contains(expected), "{needle}: {markdown}");
        let name = needle.split([' ', '=']).next().expect("name");
        if name == "Branch" {
            assert!(
                markdown.contains("List") && markdown.contains("Tree a"),
                "{markdown}"
            );
        }
        if name == "Block" {
            assert!(markdown.contains("Statements"), "{markdown}");
        }
        assert_eq!(
            range,
            (start, start + u32::try_from(name.len()).expect("small")),
            "{needle}"
        );
    }
}

#[test]
fn alias_declaration_hover_covers_parameters_without_widening_symbol_ranges() {
    let source = "module App.Plan exposing (plan, ready, pair)

type Identity a
type ReaderT r (m : Type -> Type) a
type StateT s (m : Type -> Type) a
type MaybeT (m : Type -> Type) a

type alias Config = { budget : Float; overhead : Float }
type alias PlanState = { remaining : Float; completed : List String }
/// A reusable stack of independent effects.
type alias Planner a = MaybeT (StateT PlanState (ReaderT Config Identity)) a
type alias Ready = Planner Float
type alias Pair a b = (a, b)

plan : Planner Float -> Planner Float
let plan value = value

ready : Ready -> Ready
let ready value = value

pair : Pair String Float -> Pair String Float
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
        ("Ready", "Ready", "type alias Ready = Planner Float"),
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

run : Float -> Float
let run value = value
";
    let plain = "/// Ordinary qualified module documentation.
module App.Plain exposing (run)

run : Float -> Float
let run value = value
";
    let main = "/// Module declarations have documentation too.
module App.Main exposing (..)

import App.ReaderT as ReaderT
import App.Plain

read : Float -> Float
let read value = ReaderT.run value

plain : Float -> Float
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
  minimum : Float;
  maximum : Float;
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
            "```flick\nbetween : {\n  /// The lower bound.\n  minimum : Float;\n  maximum : Float;\n} ->\n  Bool\n```"
        ),
        "{markdown}"
    );
}

#[test]
fn class_method_hover_keeps_types_and_docs_after_instance_selection() {
    let classes = "module App.Classes exposing (MonadReader(..), MonadState(..))

class MonadReader (m : Type -> Type) r | m -> r =
  /// Reads the environment.
  ask : m r

class MonadState (m : Type -> Type) s | m -> s =
  /// Reads the state.
  get : m s
  /// Replaces the state.
  put : s -> m Unit
";
    let boxed = "module App.Box exposing (Box(..))

import App.Classes exposing (MonadReader, MonadState)

type Box a = Box a

instance MonadReader Box String =
  let ask = Box \"environment\"

instance MonadState Box Float =
  let get = Box 1
  let put _ = Box ()
";
    let main = "module App.Main exposing (..)

import App.Classes exposing (MonadState, ask, get, put)
import App.Box exposing (Box)

environment : Box String
let environment = ask

current : Box Float
let current = get

saved : Box Unit
let saved = put 2

generic : MonadState m s => m s
let generic = get
";
    let f = fixture_of(&[
        ("src/App/Classes.fl", classes),
        ("src/App/Box.fl", boxed),
        ("src/App/Main.fl", main),
    ]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Main");
    for (name, nth, ty, docs, resolved) in [
        ("ask", 1, "Box String", "Reads the environment.", true),
        ("get", 1, "Box Float", "Reads the state.", true),
        ("put", 1, "Float -> Box Unit", "Replaces the state.", true),
        (
            "get",
            2,
            "forall m s. MonadState m s => m s",
            "Reads the state.",
            false,
        ),
    ] {
        let start = f.at("App.Main", name, nth);
        let (markdown, range) = ide.hover(file, start + 1).expect("method hover");
        assert!(markdown.contains(&format!("{name} : {ty}")), "{markdown}");
        assert!(markdown.contains(docs), "{markdown}");
        assert_eq!(
            range,
            (start, start + u32::try_from(name.len()).expect("small"))
        );
        if resolved {
            assert!(markdown.contains("instance "), "{markdown}");
            let (symbol, _) = ide.symbol_at(file, start + 1).expect("method symbol");
            let definition = ide.definition(&symbol).expect("instance method definition");
            assert_eq!(definition.target, Target::File(f.file("App.Box")));
            assert_eq!(
                definition.start,
                f.at("App.Box", &format!("let {name}"), 0) + 4
            );
        }
    }
}

#[test]
fn member_exposure_hover_lists_visible_signatures_in_imports_and_exports() {
    let domain = "module App.Domain exposing (MonadError(..), Box(..), Token, Visible, unrelated)

class MonadError (m : Type -> Type) e | m -> e =
  /// Raises a domain error.
  throw_error : forall c a. e^c -> (m a)^c
  /// Handles a domain error.
  catch_error : forall c d a. (forall v. e^v -> (m a)^(c + v))^c ->
    ((m a)^d -> (m a)^(c + d))^c

type Box a = Empty | Box a
type Token = Visible | Hidden

unrelated : Float
let unrelated = 42
";
    let public = "module App.Public exposing (MonadError(..), Box(..), Token(..))

import App.Domain as D exposing (MonadError(..), Box(..), Token(..))
";
    let main = "module App.Main

import App.Public as P exposing (MonadError(..), Box(..), Token(..))
";
    let f = fixture_of(&[
        ("src/App/Domain.fl", domain),
        ("src/App/Public.fl", public),
        ("src/App/Main.fl", main),
    ]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    for (module, source) in [
        ("App.Domain", domain),
        ("App.Public", public),
        ("App.Main", main),
    ] {
        let file = f.file(module);
        for (name, signatures) in [
            (
                "MonadError",
                &[
                    "throw_error : forall ",
                    "MonadError m e => e^c -> (m a)^c",
                    "catch_error : forall ",
                    "((m a)^d -> (m a)^(c + d))^c",
                    "Raises a domain error.",
                    "Handles a domain error.",
                ][..],
            ),
            (
                "Box",
                &[
                    "Empty : forall a. Box a",
                    "Box : forall a captures. a^captures -> (Box a)^{captures}",
                ][..],
            ),
            ("Token", &["Visible : Token"][..]),
        ] {
            for (start, _) in source.match_indices(&format!("{name}(..)")) {
                let dots = u32::try_from(start + name.len() + 1).expect("small");
                for offset in dots..dots + 2 {
                    let (markdown, range) = ide.hover(file, offset).expect("member list hover");
                    assert_eq!(range, (dots, dots + 2));
                    for signature in signatures {
                        assert!(markdown.contains(signature), "{module}: {markdown}");
                    }
                    for hidden in ["Hidden", "unrelated :"] {
                        assert!(!markdown.contains(hidden), "{markdown}");
                    }
                    if name != "MonadError" {
                        assert!(!markdown.contains("throw_error :"), "{markdown}");
                    }
                    if name != "Box" {
                        assert!(!markdown.contains("Empty :"), "{markdown}");
                    }
                }
                let name_at = u32::try_from(start + 1).expect("small");
                let (_, range) = ide.hover(file, name_at).expect("ordinary type hover");
                assert_eq!(range, (name_at - 1, dots - 1));
                assert!(ide.hover(file, dots + 2).is_none(), "closing parenthesis");
            }
        }
    }
}

#[test]
fn hover_reaches_instance_method_names_and_qualified_constructor_patterns() {
    let classes = "module App.Classes exposing (MonadState(..))

class MonadState (m : Type -> Type) s | m -> s =
  /// Runs a state step.
  state : forall c a. (forall v. s^v -> (a, s)^(c + v))^c -> (m a)^c
";
    let boxed = "module App.Box exposing (Box(..))

import App.Classes exposing (MonadState(..))

type Box a = Box a

instance MonadState Box Float =
  state : forall c a. (forall v. Float^v -> (a, Float)^(c + v))^c -> (Box a)^c
  let state step =
    match step 1 with
    | (a, _) -> Box a

type Wrap (m : Type -> Type) a = Wrap (m a)

instance MonadState m s => MonadState (Wrap m) s =
  state : forall c a. (forall v. s^v -> (a, s)^(c + v))^c -> (Wrap m a)^c
  let state step = Wrap (state step)
";
    let main = "module App.Main exposing (save)

import App.Box exposing (Box)

save : String ->{File.FileSystem; e} Unit
let save path =
  try File.write_text path \"x\" with
  | File.WriteError _ -> ()
";
    let f = fixture_of(&[
        ("src/App/Classes.fl", classes),
        ("src/App/Box.fl", boxed),
        ("src/App/Main.fl", main),
    ]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Box");
    for (nth, instance, ty) in [
        (0, "MonadState Box Float", "(Box a)^c"),
        (1, "MonadState Box Float", "(Box a)^c"),
        (2, "MonadState (Wrap m) s", "(Wrap m a)^c"),
        (3, "MonadState (Wrap m) s", "(Wrap m a)^c"),
    ] {
        let start = f.at("App.Box", "state", nth);
        let (markdown, range) = ide.hover(file, start + 1).expect("instance method hover");
        assert_eq!(range, (start, start + 5));
        assert!(markdown.contains(ty), "{markdown}");
        assert!(markdown.contains("Runs a state step."), "{markdown}");
        assert!(
            markdown.contains(&format!("from `instance {instance}`")),
            "{markdown}"
        );
    }
    let file = f.file("App.Main");
    let qualifier = f.at("App.Main", "File.WriteError", 0);
    let (markdown, range) = ide
        .hover(file, qualifier + 1)
        .expect("pattern qualifier hover");
    assert_eq!(range, (qualifier, qualifier + 4));
    assert!(
        markdown.starts_with("```flick\nmodule File\n```"),
        "{markdown}"
    );
}

#[test]
fn generated_url_companion_hover_keeps_its_type_and_navigation() {
    let source = "module App.Pages exposing (home)

/// Pages of this application.
type Page = Home | User Float
  deriving Schema, Url

home : String
let home = Route.print Page.url Home
";
    let f = fixture_of(&[("src/App/Pages.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Pages");
    let start = f.at("App.Pages", "Page.url", 0) + 5;
    let (markdown, range) = ide.hover(file, start + 1).expect("companion hover");
    assert!(markdown.contains("Page.url : Url Page"), "{markdown}");
    for docs in ["deriving Url", "Route.print", "Route.parse"] {
        assert!(markdown.contains(docs), "{markdown}");
    }
    assert_eq!(range, (start, start + 3));
    let (symbol, _) = ide.symbol_at(file, start + 1).expect("companion symbol");
    let definition = ide.definition(&symbol).expect("companion definition");
    let declared = f.at("App.Pages", "Page =", 0);
    assert_eq!(definition.target, Target::File(file));
    assert_eq!((definition.start, definition.end), (declared, declared + 4));
}
