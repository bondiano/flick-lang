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
