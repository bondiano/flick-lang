//! References, definitions and renaming of record fields by provenance:
//! a label belongs to an alias's field only where the checker typed its
//! record as that alias. Two aliases with a label of one spelling are two
//! fields.

use super::rename::Occurrence;
use super::tests::{Fixture, fixture_of};
use super::{Location, Target};
use crate::diagnostics::Severity;

pub(super) const DOMAIN: &str = "module App.Domain exposing (..)

/// A person.
type alias User = {
  /// The full name.
  name : String;
  address : Address;
}

type alias Address = { city : String; zip : String }

type alias Product = { name : String; price : Float }

make : String -> Address -> User
let make n a = { name = n; address = a }

renamed : User -> User
let renamed u = { u with name = \"x\" }

label : Product -> String
let label p = p.name
";

pub(super) const MAIN: &str = "module App.Main exposing (..)

import App.Domain exposing (User, Address, Product, make)

greet : User -> String
let greet user = \"Привет, \" ++ user.name

city_of : User -> String
let city_of = .address.city

shout : User -> String
let shout u = u |> .name

priced : Product -> Float
let priced p = p.price
";

pub(super) fn project(main: &str) -> Fixture {
    fixture_of(&[("src/App/Domain.fl", DOMAIN), ("src/App/Main.fl", main)])
}

/// Each occurrence as its module and its line up to it, in order.
pub(super) fn shown(f: &Fixture, found: &[Occurrence]) -> Vec<(String, String)> {
    let ide = f.ide();
    let mut out: Vec<(String, String)> = (found.iter())
        .map(|(file, (start, end))| {
            let m = ide.module(*file).expect("module");
            let text = ide.text(*file);
            let line = text[..*start as usize].rfind('\n').map_or(0, |i| i + 1);
            assert_eq!(&text[*start as usize..*end as usize], "name");
            (
                m.name.clone(),
                text[line..*end as usize].trim_start().to_owned(),
            )
        })
        .collect();
    out.sort();
    out
}

fn texts(items: &[(&str, &str)]) -> Vec<(String, String)> {
    (items.iter())
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

const USER_NAME: [(&str, &str); 5] = [
    ("App.Domain", "let make n a = { name"),
    ("App.Domain", "let renamed u = { u with name"),
    ("App.Domain", "name"),
    ("App.Main", "let greet user = \"Привет, \" ++ user.name"),
    ("App.Main", "let shout u = u |> .name"),
];

#[test]
fn a_field_is_referenced_where_its_record_is_its_alias() {
    let f = project(MAIN);
    let ide = f.ide();
    let at = f.at("App.Main", "user.name", 0) + 5;
    let (symbol, range) = ide.symbol_at(f.file("App.Main"), at).expect("a field");
    assert_eq!(range, (at, at + 4));
    let found = ide.references(&symbol);
    // the declaration's label, a literal, an update, a projection and an
    // accessor; never `Product`'s field of the same spelling
    assert_eq!(shown(&f, &found), texts(&USER_NAME));
    // from the declaration, the same
    let declared = f.at("App.Domain", "name :", 0);
    let (from_declaration, _) = ide
        .symbol_at(f.file("App.Domain"), declared)
        .expect("a field");
    assert_eq!(from_declaration, symbol);
}

#[test]
fn a_field_of_another_alias_with_that_label_is_its_own() {
    let f = project(MAIN);
    let ide = f.ide();
    let at = f.at("App.Domain", "p.name", 0) + 2;
    let (symbol, _) = ide.symbol_at(f.file("App.Domain"), at).expect("a field");
    let found = ide.references(&symbol);
    assert_eq!(
        shown(&f, &found),
        texts(&[
            ("App.Domain", "let label p = p.name"),
            ("App.Domain", "type alias Product = { name")
        ])
    );
}

/// The program with `edits` replaced by `new`, checked to have no errors.
pub(super) fn applied(
    f: &Fixture,
    main: &str,
    edits: &[Occurrence],
    new: &str,
) -> (String, String) {
    let edit = |module: &str, text: &str| {
        let file = f.file(module);
        let mut text = text.to_owned();
        for (_, (start, end)) in edits.iter().rev().filter(|(e, _)| *e == file) {
            text.replace_range(*start as usize..*end as usize, new);
        }
        text
    };
    let (domain, main) = (edit("App.Domain", DOMAIN), edit("App.Main", main));
    let checked = fixture_of(&[("src/App/Domain.fl", &domain), ("src/App/Main.fl", &main)]);
    let errors: Vec<_> = (checked.ide().analysis.diagnostics.iter())
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{domain}\n{main}\n{errors:?}");
    (domain, main)
}

#[test]
fn a_rename_edits_the_field_everywhere_and_nothing_else() {
    let f = project(MAIN);
    let ide = f.ide();
    let at = f.at("App.Main", ".name", 0) + 1;
    let edits = ide
        .rename(f.file("App.Main"), at, "full_name")
        .expect("renamed");
    let (domain, main) = applied(&f, MAIN, &edits, "full_name");
    assert!(domain.contains("  full_name : String;"), "{domain}");
    assert!(
        domain.contains("{ name : String; price : Float }"),
        "{domain}"
    );
    assert!(domain.contains("let label p = p.name"), "{domain}");
    assert!(main.contains("++ user.full_name"), "{main}");
    assert!(main.contains("u |> .full_name"), "{main}");
    // a rename at the declaration is the same edit
    let declared = f.at("App.Domain", "name :", 0);
    assert_eq!(
        ide.rename(f.file("App.Domain"), declared, "full_name"),
        Ok(edits)
    );
}

#[test]
fn a_new_name_is_checked() {
    let f = project(MAIN);
    let ide = f.ide();
    let file = f.file("App.Main");
    let at = f.at("App.Main", "user.name", 0) + 5;
    assert_eq!(
        ide.rename(file, at, "Name"),
        Err("`Name` is not a lowercase name".to_owned())
    );
    assert_eq!(
        ide.rename(file, at, "match"),
        Err("`match` is a reserved keyword".to_owned())
    );
    assert_eq!(
        ide.rename(file, at, "address"),
        Err(
            "cannot rename the field `name` of `User`: `User` already has a field `address`"
                .to_owned()
        )
    );
}

#[test]
fn an_accessor_step_leads_to_the_field_of_its_record() {
    let f = project(MAIN);
    let ide = f.ide();
    let city = f.at("App.Main", ".city", 0) + 1;
    let declared = f.at("App.Domain", "city", 0);
    assert_eq!(
        ide.field_definitions(f.file("App.Main"), city),
        [Location {
            target: Target::File(f.file("App.Domain")),
            start: declared,
            end: declared + 4,
        }]
    );
}

/// A record the checker does not type as an alias may be of any record
/// with its fields: each is a target, none is the declaration.
#[test]
fn a_record_of_no_alias_leads_to_each_with_its_fields() {
    let main = format!(
        "{MAIN}\ntype alias Person = {{ name : String; address : Address }}\n\nnames : List User -> List String\nlet names us = List.map .name us\n"
    );
    let f = project(&main);
    let ide = f.ide();
    let file = f.file("App.Main");
    let at = f.at("App.Main", ".name us", 0) + 1;
    let targets: Vec<(Target, u32)> = (ide.field_definitions(file, at).into_iter())
        .map(|l| (l.target, l.start))
        .collect();
    let user = (
        Target::File(f.file("App.Domain")),
        f.at("App.Domain", "name :", 0),
    );
    let person = (
        Target::File(file),
        f.at("App.Main", "name : String; address", 0),
    );
    assert!(
        targets == [user.clone(), person.clone()] || targets == [person, user],
        "{targets:?}"
    );
    // nor does its hover name one of them
    assert_eq!(
        ide.hover(file, at).expect("hover").0,
        "```flick\nname : String\n```"
    );
    assert!(ide.symbol_at(file, at).is_none());
}

/// Read from `User`'s `address`, a record is the `Address` that field is
/// declared with, though the checker shows it written out.
#[test]
fn a_record_read_from_a_field_is_of_the_alias_that_field_is_declared_with() {
    let f = project(MAIN);
    let ide = f.ide();
    let at = f.at("App.Main", ".city", 0) + 1;
    let edits = ide.rename(f.file("App.Main"), at, "area").expect("renamed");
    let (domain, main) = applied(&f, MAIN, &edits, "area");
    assert!(
        domain.contains("{ area : String; zip : String }"),
        "{domain}"
    );
    assert!(main.contains("let city_of = .address.area"), "{main}");
    assert_eq!(edits.len(), 2);
}
