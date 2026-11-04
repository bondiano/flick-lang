//! Each form of a block, as the heads of `pairs` for them expand it:
//! expressions without a body and with raw text, declarations with a body
//! or the rest of a declaration, and the declarations they give.

use std::path::PathBuf;

use flick_test_pairs::Pairs;

use super::{host, library, show};
#[cfg(feature = "ide")]
use crate::ide::Ide;
use crate::ir;
use crate::plugin::testing::expand;
#[cfg(feature = "ide")]
use crate::project::load::analyze;
use crate::project::load::{check, compile};
use crate::source::SourceMap;

const IMPORT: &str = "module App.Main exposing (..)\n\nimport Pairs exposing (plugin named, plugin text_of, plugin define, plugin table, plugin traced, plugin variants, plugin shows)\n\n";

const FORMS: &str = "shape : String
let shape = named Shape

path : String
let path = named App.Api.place

ddl : String
let ddl = text_of r#\"create table t (name text default 'a') -- why\"#

define answer { 40.0 + 2.0 }

table Point { x : Float = 1.0; label : String = \"p\" }

double : Float -> Float
traced let double x =
  x * 2.0

variants Color { Red; Green }

type Box = Box

shows Box

checks : List String
let checks = [shape, path, ddl, show answer, point.label, double_name, show Red, show Box]

four : Float
let four = double 2.0
";

/// A module that uses what the blocks of `App.Main` declare.
const USE: &str = "module App.Use exposing (here)

import App.Main exposing (Point, point, Color(..))

here : (Point, Color)
let here = (point, Green)
";

fn roots() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/p/src/App/Main.fl"),
        PathBuf::from("/p/src/App/Use.fl"),
    ]
}

#[test]
fn every_form_of_a_block_expands_and_its_declarations_are_the_modules() {
    let main = format!("{IMPORT}{FORMS}");
    let host = host(&main);
    host.write("/p/src/App/Use.fl", USE);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, compilation) = compile(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = diagnostics.iter().map(|d| show(&sources, d)).collect();
    assert!(diagnostics.is_empty(), "{shown:#?}");
    let module = (compilation.modules.iter())
        .find(|m| m.name == "App.Main")
        .expect("the module");
    let printed = ir::print(&module.ir);
    for needle in [
        "\"Shape\"",
        "\"App.Api.place\"",
        "create table t (name text default 'a') -- why",
        "double_name",
    ] {
        assert!(printed.contains(needle), "{needle} in {printed}");
    }
    // where the module declares a generated name is in its text: at the block
    let len = u32::try_from(main.len()).expect("short");
    for export in &module.interface.exports {
        let [start, end] = export.at.expect("declared here");
        assert!(
            end <= len,
            "{} at {start}..{end} past {len}",
            export.entity.name
        );
        if export.entity.name == "point" {
            assert!(main[start as usize..end as usize].starts_with("table Point"));
        }
    }
}

#[test]
fn errors_land_on_the_users_code_of_a_declaration_or_on_the_block() {
    let report = |body: &str| {
        let main = format!("{IMPORT}{body}");
        let mut sources = SourceMap::default();
        let project = library(&mut sources);
        let host = host(&main);
        let found = check(
            &host,
            &mut sources,
            Some(&project),
            &[PathBuf::from("/p/src/App/Main.fl")],
        );
        found.iter().map(|d| show(&sources, d)).collect::<Vec<_>>()
    };
    // in a hole: at that code
    let found = report("table Bad { x : Float = \"no\" }\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`\"no\"`"), "{found:?}");
    // a name the block declares twice with the module: on the block
    let found = report("clash : Float\nlet clash = 1.0\n\ndefine clash { 2.0 }\n");
    assert!(
        found.iter().any(|f| f.contains("`define clash { 2.0 }`")),
        "{found:?}"
    );
    // a head where it does not stand: what the plugin says
    let found = report("x : String\nlet x = define y { 1.0 }\n");
    assert!(
        found.iter().any(|f| f.contains("stands elsewhere")),
        "{found:?}"
    );
}

#[cfg(feature = "ide")]
#[test]
fn editors_see_the_users_code_inside_a_declaration() {
    let main = format!("{IMPORT}base : Float\nlet base = 2.0\n\ndefine answer {{ base + 40.0 }}\n");
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let host = host(&main);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Main.fl")],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App.Main").expect("the module").file;
    let inside = u32::try_from(main.rfind("base").expect("used")).expect("short");
    let (shown, _) = ide
        .hover(file, inside + 1)
        .expect("a hover inside the block");
    assert!(shown.contains("Float"), "{shown}");
}

#[test]
fn the_code_of_a_declaration_shows_as_declarations() {
    let source = format!("{IMPORT}table Point {{ x : Float = 1.0 }}\n\ntraced let double x =\n  x * 2.0\n\nshows Box\n");
    let shown: Vec<String> = (expand(&Pairs, &source).into_iter())
        .map(|found| found.unwrap_or_else(|e| panic!("{e:?}")))
        .collect();
    assert_eq!(
        shown,
        [
            "type alias Point = { x : Float }\nlet point : Point = { x = 1.0 }",
            "let double x = x * 2.0\nlet double_name = \"double\"",
            "instance (Show Box) =\n  let show _ = \"Box\"",
        ]
    );
}
