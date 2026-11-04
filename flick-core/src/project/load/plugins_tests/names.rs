//! The names of generated code: its locals and the user's do not see each
//! other, and it names a module by its path whatever the file calls it.

use std::path::PathBuf;

use flick_test_pairs::Pairs;

use super::{host, library, show};
#[cfg(feature = "ide")]
use crate::ide::Ide;
use crate::plugin::testing::expand;
#[cfg(feature = "ide")]
use crate::project::load::analyze;
use crate::project::load::{check, compile};
use crate::source::SourceMap;
use crate::{doc, ir};

const HYGIENE: &str = "module App.Main exposing (..)

import Pairs as P exposing (plugin capture)
import Other as Pairs

x : Float
let x = 1.0

held : { held : Float; own : String; shown : String }
let held = capture { x + 1.0 }

hidden : Float -> { held : Float; own : String; shown : String }
let hidden show = capture { show }

param : Float -> { held : Float; own : String; shown : String }
let param x = capture { x }

other : Float
let other = Pairs.o
";

#[test]
fn generated_locals_and_the_users_do_not_capture_each_other() {
    let host = host(HYGIENE);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = [PathBuf::from("/p/src/App/Main.fl")];
    let (diagnostics, compilation) = compile(&host, &mut sources, Some(&project), &roots);
    let shown: Vec<String> = diagnostics.iter().map(|d| show(&sources, d)).collect();
    assert!(diagnostics.is_empty(), "{shown:#?}");
    let module = (compilation.modules.iter())
        .find(|m| m.name == "App.Main")
        .expect("the module");
    let printed = ir::print(&module.ir);
    // the user's `x` and the generated one are two locals of `param`
    let param = &printed[printed.find("param").expect("param")..];
    assert!(param.contains("x$1"), "{param}");
}

const COMPANION: &str = "module App.Main exposing (Box)

import Pairs exposing (plugin companion)

type Box = Box

companion Box { \"boxed\" }
";

/// A module that names the companion its importee's block declares.
const LABEL: &str = "module App.Use exposing (label)

import App.Main exposing (Box)

label : String
let label = Box.label
";

#[test]
fn a_block_declares_a_companion_of_a_type_which_its_importers_name() {
    let host = host(COMPANION);
    host.write("/p/src/App/Use.fl", LABEL);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = [
        PathBuf::from("/p/src/App/Main.fl"),
        PathBuf::from("/p/src/App/Use.fl"),
    ];
    let (diagnostics, _) = compile(&host, &mut sources, Some(&project), &roots);
    let shown: Vec<String> = diagnostics.iter().map(|d| show(&sources, d)).collect();
    assert!(diagnostics.is_empty(), "{shown:#?}");
}

#[test]
fn a_companion_of_no_type_of_the_module_is_an_error_on_its_block() {
    let main = COMPANION.replace("companion Box", "companion Nope");
    let host = host(&main);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Main.fl")],
    );
    let shown: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert!(
        shown.iter().any(
            |s| s.starts_with("FLICK-P036 `companion Nope { \"boxed\" }`")
                && s.contains("`Nope` is no type of this module")
        ),
        "{shown:#?}"
    );
}

#[cfg(feature = "ide")]
#[test]
fn editors_find_a_generated_name_at_the_name_its_block_gives() {
    let main = "module App.Main exposing (..)

import Pairs exposing (plugin table)

/// A point of the plane.
table Point { x : Float = 1.0 }

here : Point
let here = point
";
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let host = host(main);
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
    let offset = |needle: &str| u32::try_from(main.find(needle).expect("in main")).expect("short");
    let name = offset("Point {");
    let named = (name, name + 5);
    // the use of a generated name goes to the name in its block
    let (symbol, _) = ide.symbol_at(file, offset("point\n")).expect("a symbol");
    let found = ide.definition(&symbol).expect("a definition");
    assert_eq!((found.start, found.end), named);
    // the name in the block is the generated declaration
    let (shown, _) = ide.hover(file, name + 1).expect("a hover on the name");
    assert!(shown.contains("Point"), "{shown}");
    // the documentation of the block is the generated declaration's
    let (shown, _) = ide
        .hover(file, offset(": Point") + 2)
        .expect("a hover on the type");
    assert!(shown.contains("A point of the plane."), "{shown}");
    let outline: Vec<String> = ide
        .document_symbols(file)
        .into_iter()
        .map(|o| o.name)
        .collect();
    assert_eq!(outline, ["Point", "point", "here"]);
    let refused = ide
        .rename(file, offset("point\n"), "spot")
        .expect_err("refused");
    assert!(refused.contains("declared by plugin `pairs`"), "{refused}");
}

#[test]
fn the_documentation_of_a_block_documents_what_it_declares() {
    let main = "module App.Main exposing (Point)

import Pairs exposing (plugin table)

/// A point of the plane.
table Point { x : Float = 1.0 }
";
    let host = host(main);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = [PathBuf::from("/p/src/App/Main.fl")];
    let (diagnostics, compilation) = compile(&host, &mut sources, Some(&project), &roots);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let module = (compilation.modules.iter())
        .find(|m| m.name == "App.Main")
        .expect("the module");
    let text = &module.expansion.as_ref().expect("expanded").text;
    let page = doc::page(&module.interface, text);
    assert!(page.contains("A point of the plane."), "{page}");
}

#[test]
fn a_companion_shows_as_generated_code_names_it() {
    let shown = expand(&Pairs, COMPANION);
    assert_eq!(shown, [Ok("let Box$label = \"boxed\"".to_owned())]);
}
