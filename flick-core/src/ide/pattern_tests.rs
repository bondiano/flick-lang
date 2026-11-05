use super::tests::fixture_of;
use super::{Symbol, Target};
use crate::diagnostics::Diagnostic;
use crate::iface::Namespace;

#[test]
fn constructor_payload_binders_keep_their_hover_definition_and_rename_ranges() {
    let f = fixture_of(&[
        (
            "src/App/Domain.fl",
            "module App.Domain exposing (CliError(..), Box(..), fail)

type error CliError = CliError String

type Box = Box String

fail : Unit ->{; CliError} String
let fail () = from_result (Err (CliError \"broken\"))
",
        ),
        (
            "src/App/Main.fl",
            "module App.Main exposing (..)

import App.Domain as Cli exposing (Box(..))

handled : Unit ->{} String
let handled () =
  try Cli.fail () with
  | Cli.CliError message -> message

qualified : Cli.Box -> String
let qualified box =
  match box with
  | Cli.Box qualified_message -> qualified_message

unqualified : Cli.Box -> String
let unqualified box =
  match box with
  | Box plain_message -> plain_message
",
        ),
    ]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Main");
    for (pattern, constructor, name) in [
        ("Cli.CliError message", "CliError", "message"),
        ("Cli.Box qualified_message", "Box", "qualified_message"),
        ("Box plain_message", "Box", "plain_message"),
    ] {
        let start = f.at("App.Main", &format!("{name} ->"), 0);
        let binder = (start, start + u32::try_from(name.len()).expect("small"));
        let body = f.at("App.Main", name, 1);
        let body_range = (body, body + u32::try_from(name.len()).expect("small"));
        let local = Symbol::Local {
            file,
            binder,
            name: name.into(),
        };
        for range in [binder, body_range] {
            let (markdown, hovered) = ide.hover(file, range.0 + 1).expect("binder hover");
            assert!(markdown.contains(&format!("{name} : String")), "{markdown}");
            assert_eq!(hovered, range);
            assert_eq!(
                ide.symbol_at(file, range.0 + 1),
                Some((local.clone(), range))
            );
            let definition = ide.definition(&local).expect("binder definition");
            assert_eq!(definition.target, Target::File(file));
            assert_eq!((definition.start, definition.end), binder);
            let mut expected = vec![(file, binder), (file, body_range)];
            expected.sort();
            assert_eq!(
                ide.rename(file, range.0 + 1, "renamed")
                    .expect("local rename"),
                expected
            );
        }
        let start = f.at("App.Main", pattern, 0)
            + u32::try_from(pattern.find(constructor).expect("constructor")).expect("small");
        let constructor_range = (
            start,
            start + u32::try_from(constructor.len()).expect("small"),
        );
        let (symbol, range) = ide.symbol_at(file, start + 1).expect("constructor symbol");
        assert_eq!(range, constructor_range);
        assert!(matches!(&symbol, Symbol::Entity(e)
            if e.namespace == Namespace::Constructor && e.name == constructor));
        let (markdown, range) = ide.hover(file, start + 1).expect("constructor hover");
        assert!(markdown.contains(&format!("{constructor} :")), "{markdown}");
        assert_eq!(range, constructor_range);
        let definition = ide.definition(&symbol).expect("constructor definition");
        assert_eq!(definition.target, Target::File(f.file("App.Domain")));
        assert_eq!(
            &ide.sources.get(f.file("App.Domain")).text
                [definition.start as usize..definition.end as usize],
            constructor
        );
        let edits = ide
            .rename(file, start + 1, "Renamed")
            .expect("constructor rename");
        assert!(edits.contains(&(file, constructor_range)), "{edits:?}");
        assert!(!edits.contains(&(file, binder)), "{edits:?}");
    }
}

#[test]
fn private_route_constructor_patterns_show_types_and_documentation() {
    let source = "module App.Pages exposing (profile)

type error NotFound = NotFound
  deriving Schema

type Page =
  /// The landing page.
  | Home
  /// A user profile by numeric identifier.
  | User Float
  deriving Schema, Url

page : Atom.Writable Page
let page = Atom.route Page.url Home

name : Float ->{; NotFound} String
let name id =
  if id == 404 then from_result (Err NotFound) else show id

profile : Atom.Atom (Atom.Async String {NotFound})
let profile =
  Atom.serializable_result
    (Codec.json Schema.string)
    (Codec.json NotFound.schema)
    (Atom.loader page (fun p -> task {
      match p with
      | Home -> \"home\"
      | User 404 ->
        let _ = name 404
        from_result (Err NotFound)
      | User id -> name id
    }))
";
    let f = fixture_of(&[("src/App/Pages.fl", source)]);
    let ide = f.ide();
    assert!(
        ide.analysis.diagnostics.iter().all(Diagnostic::unnecessary),
        "{:?}",
        ide.analysis.diagnostics
    );
    let file = f.file("App.Pages");
    for (pattern, name, ty, docs) in [
        ("Home ->", "Home", "Page", "The landing page."),
        (
            "User 404",
            "User",
            "Float -> Page",
            "A user profile by numeric identifier.",
        ),
        (
            "User id ->",
            "User",
            "Float -> Page",
            "A user profile by numeric identifier.",
        ),
    ] {
        let start = f.at("App.Pages", pattern, 0);
        let (markdown, range) = ide.hover(file, start + 1).expect("constructor hover");
        assert!(markdown.contains(&format!("{name} : {ty}")), "{markdown}");
        assert!(markdown.contains(docs), "{markdown}");
        assert_eq!(
            range,
            (start, start + u32::try_from(name.len()).expect("small"))
        );
    }
    let start = f.at("App.Pages", "id -> name", 0);
    let (markdown, range) = ide.hover(file, start + 1).expect("payload binder hover");
    assert!(markdown.contains("id : Float"), "{markdown}");
    assert_eq!(range, (start, start + 2));
}
