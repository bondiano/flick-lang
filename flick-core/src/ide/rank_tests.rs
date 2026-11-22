//! Completion ranks what fits its place first and keeps every candidate.
//! Each place pins the candidates that fit, in order, worked out from the
//! types below by hand; the controls complete the same places with a
//! candidate that fits and one that does not, and the checker accepts the
//! first and rejects the second, so that the order agrees with it.

use super::Ide;
use super::complete::{Completion, CompletionKind};
use super::expect::MISFIT;
use super::tests::{Fixture, fixture_of};
use crate::diagnostics::Severity;
use crate::diagnostics::codes::{self, Code};
use crate::source::FileId;

const DOMAIN: &str =
    "module App.Domain exposing (Shape(..), Figure, NotFound(..), Busy(..), Users, area, describe)

type Shape = Circle Float | Square Float

type alias Figure = Shape

type error NotFound = NotFound String

type error Busy = Busy

service Users =
  load : String ->{; NotFound} Shape

area : Figure -> Float
let area s =
  match s with
  | Circle r -> r * r
  | Square a -> a * a

describe : Shape -> String
let describe s =
  match s with
  | Circle _ -> \"circle\"
  | Square _ -> \"square\"
";

/// The module completed at each `{place}`.
const MAIN: &str = "module App.Main exposing (..)

import App.Domain exposing (Shape(..), Figure, NotFound(..), Busy(..), Users, area, describe)

service Log =
  info : String ->{} Unit

grow : Float -> Figure -> Shape
let grow k s = s

fetch : String ->{Users; NotFound} Shape
let fetch id = Users.load id

logged : String ->{Log; NotFound} Shape
let logged id =
  Log.info id
  Circle 1.0

busy : String ->{Log; Busy} Shape
let busy id = Fail.fail Busy

argument : Shape ->{Log; NotFound} String
let argument shape =
  let size = area shape
  describe {argument}

hole : Shape -> String
let hole shape =
  let size = area shape
  describe {hole}

second : Float -> Unit
let second size =
  let fig : Shape = grow size {second}
  ()

annotated : Shape ->{Log; NotFound} Unit
let annotated shape =
  let fig : Figure = {annotated}
  Log.info (describe fig)

statement : Shape ->{Log; NotFound} String
let statement shape =
  {statement}
  describe shape

unannotated : Shape -> String
let unannotated shape =
  let name = {unannotated}
  name

deferred : Shape ->{Log; NotFound} Unit
let deferred shape =
  let later = fun id -> fetch id
  Log.info (describe shape)

matched : Shape -> Float
let matched shape =
  match shape with
  | {matched}

caught : Unit ->{Users} String
let caught () =
  try describe (fetch \"x\") with
  | {caught}
";

/// A module that does not import `App.Domain`, whose names it completes.
const OTHER: &str = "module App.Other exposing (..)

import App.Main

later : Unit -> String
let later () =
  NotF
";

/// Each place: what is written there while completing, then a candidate
/// that fits and one that does not, and the code that rejects the latter.
type Place = (&'static str, &'static str, &'static str, &'static str, Code);

const PLACES: [Place; 8] = [
    ("argument", "sha", "shape", "size", codes::T003),
    ("hole", "", "shape", "(logged \"x\")", codes::E001),
    ("second", "", "(Circle 1.0)", "(fetch \"x\")", codes::E001),
    ("annotated", "", "logged \"x\"", "fetch \"x\"", codes::E002),
    ("statement", "se", "second 1.0", "logged \"x\"", codes::T011),
    (
        "unannotated",
        "sha",
        "describe shape",
        "fetch \"x\"",
        codes::E001,
    ),
    (
        "matched",
        "Ci",
        "Circle r -> r\n  | Square a -> a",
        "NotFound _ -> 0.0\n  | _ -> 1.0",
        codes::T003,
    ),
    (
        "caught",
        "",
        "NotFound _ -> \"none\"",
        "Circle _ -> \"none\"",
        codes::E007,
    ),
];

/// The project with `pick` of each place written there, and where each is:
/// the places are in the order of the text, so a later one moves none.
fn program(pick: impl Fn(&Place) -> &'static str) -> (Fixture, Vec<usize>) {
    let mut main = MAIN.to_owned();
    let mut starts = Vec::new();
    for place in &PLACES {
        let marker = format!("{{{}}}", place.0);
        let start = main.find(&marker).expect("a place");
        main.replace_range(start..start + marker.len(), pick(place));
        starts.push(start);
    }
    let files = [
        ("src/App/Domain.fl", DOMAIN),
        ("src/App/Main.fl", &main),
        ("src/App/Other.fl", OTHER),
    ];
    (fixture_of(&files), starts)
}

/// The labels that fit at `at`, as an editor orders them, after checking
/// that ranking moves items back and changes nothing else about them.
fn fitting(ide: &Ide, file: FileId, at: u32) -> Vec<String> {
    let items = ide.complete(file, at, false);
    let mut plain = ide.complete_names(file, at, None);
    plain.extend(ide.auto_imports(file, at, &plain, None));
    ide.enrich(&mut plain);
    assert_eq!(items.len(), plain.len(), "every candidate stays");
    for (ranked, plain) in items.iter().zip(&plain) {
        assert!(ranked.sort == plain.sort || ranked.sort == plain.sort + MISFIT);
        let unranked = Completion {
            sort: plain.sort,
            ..ranked.clone()
        };
        assert_eq!(&unranked, plain, "docs, snippets and edits stay");
    }
    let mut fit: Vec<(u8, String)> = (items.into_iter())
        .filter(|c| c.sort < MISFIT)
        .map(|c| (c.sort, c.label))
        .collect();
    fit.sort();
    fit.into_iter().map(|(_, label)| label).collect()
}

#[test]
fn completion_ranks_candidates_that_fit_the_place_first() {
    let (f, _) = program(|p| p.1);
    let ide = f.ide();
    let file = f.file("App.Main");
    let text = &ide.sources.get(file).text;
    let at = |line: &str, written: &str| {
        let start = text.find(line).unwrap_or_else(|| panic!("{line}"));
        u32::try_from(start + line.len() - written.len()).expect("small")
    };
    let fits = |line: &str, written: &str| fitting(&ide, file, at(line, written));
    // an argument: the values and constructors of its type, where a
    // runtime stage of `Log` allows `logged` but not `fetch` (`Users`) or
    // `busy` (`Busy`)
    let shapes = ["shape", "grow", "logged", "Circle", "Square"];
    assert_eq!(fits("describe sha", "sha"), shapes);
    // the same in a pure function, where no call runs
    assert_eq!(
        fits("  describe \n", "\n"),
        ["shape", "grow", "Circle", "Square"]
    );
    // a second argument after its first, of a transparent alias
    assert_eq!(fits("grow size \n", "\n"), ["grow", "Circle", "Square"]);
    // the body of an annotated `let`, through the alias it names
    assert_eq!(fits("let fig : Figure = \n", "\n"), shapes);
    // the constructors of the type a `match` takes apart
    assert_eq!(fits("| Ci", "Ci"), ["Circle", "Square"]);
    // the errors the body of a `try` may raise
    assert_eq!(fits("with\n  | \n", "\n"), ["NotFound"]);
    // a statement that is dropped is a `Unit`
    let dropped = fits("  se\n", "se\n");
    assert!(dropped.contains(&"second".to_owned()), "{dropped:?}");
    assert!(dropped.contains(&"annotated".to_owned()), "{dropped:?}");
    assert!(!dropped.contains(&"logged".to_owned()), "{dropped:?}");
    // an unannotated `let` says no type: only what a pure body may call
    let unknown = fits("let name = sha", "sha");
    assert!(unknown.contains(&"describe".to_owned()), "{unknown:?}");
    assert!(!unknown.contains(&"fetch".to_owned()), "{unknown:?}");
    // a `fun` decides its own rows: `fetch` needs `Users`, which the
    // signature around it does not give, and stays where it was
    let inside = fits("-> fetch id", "fetch id");
    assert!(inside.contains(&"fetch".to_owned()), "{inside:?}");
    // an auto-import of a constructor is the constructor, not the type it
    // shares its name with
    let other = f.file("App.Other");
    let at = ide.sources.get(other).text.find("NotF").expect("written");
    let items = ide.complete(other, u32::try_from(at).expect("small"), false);
    let found = items
        .iter()
        .find(|c| c.label == "NotFound")
        .expect("offered");
    assert_eq!(found.kind, CompletionKind::Constructor);
    assert_eq!(
        found.detail.as_deref(),
        Some("App.Domain.NotFound : String -> NotFound")
    );
}

#[test]
fn the_checker_accepts_what_fits_and_rejects_what_does_not() {
    let errors = |f: &Fixture| -> Vec<(Code, u32)> {
        let file = f.file("App.Main");
        (f.ide().analysis.diagnostics.iter())
            .filter(|d| d.severity == Severity::Error)
            .filter_map(|d| d.span.filter(|s| s.file == file).map(|s| (d.code, s.start)))
            .collect()
    };
    let (accepted, _) = program(|p| p.2);
    assert_eq!(
        errors(&accepted),
        [],
        "{:?}",
        accepted.ide().analysis.diagnostics
    );
    let (rejected, starts) = program(|p| p.3);
    let found = errors(&rejected);
    for ((place, _, _, misfit, code), start) in PLACES.iter().zip(starts) {
        let within = start..=start + misfit.len();
        assert!(
            (found.iter()).any(|(c, at)| c == code && within.contains(&(*at as usize))),
            "{place}: {code:?} in {found:?}"
        );
    }
}

#[test]
fn postfix_catch_completion_ranks_body_errors_first() {
    let partial_empty = "\
module App.Main exposing (..)

import App.Domain exposing (Shape(..), NotFound(..), Busy(..), Users, describe)

fetch : String ->{Users; NotFound} Shape
let fetch id = Users.load id

caught : Unit ->{Users} String
let caught () =
  describe (fetch \"x\") catch
  | \n";
    let f = fixture_of(&[
        ("src/App/Domain.fl", DOMAIN),
        ("src/App/Main.fl", partial_empty),
    ]);
    let ide = f.ide();
    let file = f.file("App.Main");
    let at = f.at("App.Main", "  | \n", 0) + 4;
    assert_eq!(fitting(&ide, file, at), ["NotFound"]);

    let partial_prefix = partial_empty.replace("  | \n", "  | Not\n");
    let f = fixture_of(&[
        ("src/App/Domain.fl", DOMAIN),
        ("src/App/Main.fl", &partial_prefix),
    ]);
    let ide = f.ide();
    let file = f.file("App.Main");
    let at = f.at("App.Main", "Not\n", 0);
    assert_eq!(fitting(&ide, file, at), ["NotFound"]);
}
