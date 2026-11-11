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
        ("get", 2, "MonadState m s => m s", "Reads the state.", false),
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
                    "throw_error : MonadError m e => e^c -> (m a)^c",
                    "catch_error : ",
                    "(m a)^d -> (m a)^(c + d)",
                    "Raises a domain error.",
                    "Handles a domain error.",
                ][..],
            ),
            (
                "Box",
                &["Empty : Box a", "Box : a^captures -> (Box a)^{captures}"][..],
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

save : String ->{Async, File.FileSystem; e} Unit
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
fn instance_method_without_written_signature_has_hover_type() {
    let classes = "module Mtl.Classes exposing (MonadTrans(..))

class MonadTrans (t : (Type -> Type) -> Type -> Type) =
  lift : forall c m a. Monad m => (m a)^c -> (t m a)^c
";
    let maybe = "module Mtl.MaybeT exposing (MaybeT(..))

import Mtl.Classes exposing (MonadTrans(..))

type MaybeT (m : Type -> Type) a = MaybeT (m (Option a))

instance MonadTrans MaybeT =
  let lift action = MaybeT (map Some action)
";
    let f = fixture_of(&[
        ("src/Mtl/Classes.fl", classes),
        ("src/Mtl/MaybeT.fl", maybe),
    ]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("Mtl.MaybeT");
    let start = f.at("Mtl.MaybeT", "let lift", 0) + 4;
    let (markdown, range) = ide.hover(file, start + 1).expect("lift hover");
    assert_eq!(range, (start, start + 4));
    assert!(
        markdown.contains("lift : Monad m => (m a)^c -> (MaybeT m a)^c"),
        "{markdown}"
    );
    assert!(markdown.contains("from `instance MonadTrans MaybeT`"));
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
