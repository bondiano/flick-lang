//! Projections in release Go: the built-in `List.map` of a path of fields is
//! a loop over the slice, and an applied path is its reads.

use std::collections::BTreeMap;

use super::{Input, go};
use crate::build::builtin_modules;
use crate::build::go::bindings;
use crate::codegen::Known;
use crate::ir::tests::lower_text;
use crate::ir::{Module, lang_items};
use crate::opt;

/// The Go of the module `App` of `text`, optimized for release.
fn release(text: &str, exported: &[&str]) -> String {
    let mut ir = lower_text("App", text).expect("lowers");
    opt::optimize(
        &mut ir,
        &|name: &str| exported.contains(&name),
        &|_, _| None,
        &|_, _| None,
    );
    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let out = go(&Input {
        own: &[&ir],
        builtins: &builtins,
        entry: &ir.name,
        suites: &[],
        test: false,
        paths: &BTreeMap::new(),
        lines: &[],
        sources: &[],
        foreign: &BTreeMap::new(),
        bindings: bindings(),
        lang: &Known::of(&lang_items()),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let (_, app) = (out.files.iter())
        .find(|(name, _)| name.ends_with("m_App.go"))
        .expect("the Go of App");
    app.clone()
}

const PROGRAM: &str = "module App exposing (main, cities, zips, city, computes)\n\n\
    type alias Address = { city : String; zip : Int }\n\n\
    type alias User = { name : String; address : Address }\n\n\
    cities : List User -> List String\nlet cities users = List.map .address.city users\n\n\
    zips : List User -> List Int\nlet zips users =\n  let get = fun u -> u.address.zip\n  List.map get users\n\n\
    city : User -> String\nlet city user = user |> .address.city\n\n\
    computes : List User -> List Int\nlet computes users = List.map (fun u -> u.address.zip + 1) users\n\n\
    main : Unit ->{Host.Console; AppError} Unit\n\
    let main () =\n  let ada = { name = \"Ada\"; address = { city = \"London\"; zip = 1 } }\n  \
    Platform.stdout_line (show (cities [ada], zips [ada], city ada, computes [ada]))\n";

/// Inlined into `main` or not, no projection is called per element: the
/// only callback and the only call of `List.map` are those of the function
/// that computes.
#[test]
fn the_builtin_map_of_a_projection_is_a_loop_without_a_callback() {
    let go = release(PROGRAM, &["main", "cities", "zips", "city", "computes"]);
    assert_eq!(go.matches("F_List__map(").count(), 1, "{go}");
    assert_eq!(go.matches("flickrt.Fn(").count(), 1, "{go}");
    assert!(go.contains("return F_List__map(fb, flickrt.Fn("), "{go}");
    // `cities` and `zips` loop over their lists, reading two fields of each
    assert_eq!(go.matches("for ").count(), 2, "{go}");
    assert!(
        go.contains("flickrt.Field(flickrt.Field(l__x24_elem2, \"address\"), \"zip\")"),
        "{go}"
    );
}
