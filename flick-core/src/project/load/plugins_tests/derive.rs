//! A class marked `@derive`: its plugin derives its instances for the types
//! that name it in `deriving`, and for a type of another module of the
//! package with `deriving instance`.

use std::path::PathBuf;

use super::{host, library, show};
use crate::project::load::{check, compile};
use crate::source::SourceMap;

/// `Pairs` with a class its plugin derives.
const PAIRS: &str = "module Pairs exposing (..)

@derive
class Arbitrary a =
  arbitrary : a

instance Arbitrary Float =
  let arbitrary = 0.0
";

const SHAPES: &str = "module App.Shapes exposing (..)

type Shape = Square Float | Circle Float
";

const MAIN: &str = "module App.Main exposing (..)

import Pairs
import App.Shapes

type Point = Point Float Float
  deriving Pairs.Arbitrary

deriving instance Pairs.Arbitrary (App.Shapes.Shape)

point : Point
let point = Pairs.arbitrary

shape : App.Shapes.Shape
let shape = Pairs.arbitrary
";

fn roots() -> [PathBuf; 2] {
    [
        PathBuf::from("/p/src/App/Main.fl"),
        PathBuf::from("/p/src/App/Shapes.fl"),
    ]
}

#[test]
fn a_class_marked_derive_is_derived_by_its_plugin() {
    let host = host(MAIN);
    host.write("/pairs/src/Pairs.fl", PAIRS);
    host.write("/p/src/App/Shapes.fl", SHAPES);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, _) = compile(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = diagnostics.iter().map(|d| show(&sources, d)).collect();
    assert!(diagnostics.is_empty(), "{shown:#?}");
}

#[test]
fn what_a_derive_refuses_is_on_its_class() {
    let main = MAIN.replace("type Point = Point Float Float", "type Point");
    let host = host(&main);
    host.write("/pairs/src/Pairs.fl", PAIRS);
    host.write("/p/src/App/Shapes.fl", SHAPES);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert!(
        shown
            .iter()
            .any(|s| s.contains("`Pairs.Arbitrary`") && s.contains("no constructor")),
        "{shown:#?}"
    );
}

#[test]
fn deriving_instance_of_a_class_without_derive_is_c013() {
    let main = MAIN.replace(
        "deriving instance Pairs.Arbitrary (App.Shapes.Shape)",
        "deriving instance Show (App.Shapes.Shape)",
    );
    let host = host(&main);
    host.write("/pairs/src/Pairs.fl", PAIRS);
    host.write("/p/src/App/Shapes.fl", SHAPES);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert!(
        shown
            .iter()
            .any(|s| s.starts_with("FLICK-C013 `Show`") && s.contains("has no derive")),
        "{shown:#?}"
    );
}

#[test]
fn deriving_instance_of_a_type_no_import_declares_is_c013() {
    let main = MAIN.replace(
        "deriving instance Pairs.Arbitrary (App.Shapes.Shape)",
        "deriving instance Pairs.Arbitrary (App.Shapes.Missing)",
    );
    let host = host(&main);
    host.write("/pairs/src/Pairs.fl", PAIRS);
    host.write("/p/src/App/Shapes.fl", SHAPES);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert!(
        shown
            .iter()
            .any(|s| s.starts_with("FLICK-C013 `App.Shapes.Missing`")),
        "{shown:#?}"
    );
}

#[test]
fn a_class_of_a_module_not_imported_is_not_derived() {
    let main = "module App.Main exposing (..)

import App.Shapes

type Point = Point Float Float
  deriving Pairs.Arbitrary
";
    let shapes = SHAPES.replace("exposing (..)\n", "exposing (..)\n\nimport Pairs\n");
    let host = host(main);
    host.write("/pairs/src/Pairs.fl", PAIRS);
    host.write("/p/src/App/Shapes.fl", &shapes);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&host, &mut sources, Some(&project), &roots());
    let shown: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    // the clause's error alone: no plugin ran to write code that names `Pairs`
    let unknown = shown
        .iter()
        .filter(|s| s.contains("unknown module `Pairs`"));
    assert_eq!(unknown.count(), 1, "{shown:#?}");
}
