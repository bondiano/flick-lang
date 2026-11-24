//! Projections in release: `.a.b` and `fun r -> r.a.b` applied at once,
//! known by a name, or mapped over a list by the built-in `List.map`, are
//! the field reads they stand for; nothing else changes budget.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::tests::{builtin_arity, builtin_generic, release};
use crate::codegen::test_options::resolved_by;
use crate::codegen::{EffectBridge, Known, Labels, Options, js};
use crate::ir::lang_items;
use crate::ir::tests::{check_text, check_text_with};
use crate::types::builtins;

const RECORDS: &str = "type alias Address = { city : String; zip : Int }\n\n\
    type alias User = { name : String; address : Address }\n\n";

/// The release JS of a module `App` exposing `names`, with [`RECORDS`].
fn app(names: &str, definitions: &str) -> String {
    release(&format!(
        "module App exposing ({names})\n\n{RECORDS}{definitions}"
    ))
}

/// The JS of the definition `name` of `js`, up to the next export.
fn definition<'a>(js: &'a str, name: &str) -> &'a str {
    let start = js
        .find(&format!("export const {name} "))
        .unwrap_or_else(|| panic!("`{name}` in\n{js}"));
    let rest = &js[start..];
    let end = rest[1..]
        .find("export const ")
        .map_or(rest.len(), |i| i + 1);
    &rest[..end]
}

#[test]
fn an_applied_accessor_is_its_reads_with_the_root_evaluated_once() {
    let js = app(
        "direct, piped, explicit, fetched, deep",
        "direct : User -> String\nlet direct u = .address.city u\n\n\
         piped : User -> String\nlet piped u = u |> .address.city\n\n\
         explicit : User -> String\nlet explicit u = (fun r -> r.address.city) u\n\n\
         fetched : (Unit -> User) -> String\nlet fetched fetch = fetch () |> .address.city\n\n\
         deep : { a : { b : { c : Int } } } -> Int\nlet deep r = .a.b.c r\n",
    );
    insta::assert_snapshot!(js);
}

#[test]
fn a_known_projection_is_inlined_where_it_is_applied() {
    let js = app(
        "sugar, explicit, generic, eight, deep_eight",
        "sugar : User -> User -> Int\nlet sugar a b =\n  let get = .address.zip\n  get a + get b\n\n\
         explicit : User -> User -> Int\nlet explicit a b =\n  let get = fun r -> r.address.zip\n  get a + (b |> get)\n\n\
         generic : Int\nlet generic =\n  let value = .value\n  value { value = 3 } + String.length (value { value = \"four\" })\n\n\
         eight : { a : { b : { c : { d : { e : { f : { g : { h : Int } } } } } } } } -> Int\n\
         let eight = .a.b.c.d.e.f.g.h\n\n\
         deep_eight : { a : { b : { c : { d : { e : { f : { g : { h : Int } } } } } } } } -> Int\n\
         let deep_eight r = eight r + eight r\n",
    );
    insta::assert_snapshot!(js);
}

/// The ordinary budgets hold for what is no projection: a body of the size
/// of an eight-step path, a path of nine steps, a local function that
/// computes.
#[test]
fn what_is_no_projection_keeps_the_ordinary_budgets() {
    let js = app(
        "sized, use_sized, nine, use_nine, computes",
        "sized : { a : { b : { c : { d : { e : { f : Int } } } } } } -> Int\n\
         let sized r = r.a.b.c.d.e.f + 1\n\n\
         use_sized : { a : { b : { c : { d : { e : { f : Int } } } } } } -> Int\n\
         let use_sized r = sized r + sized r\n\n\
         nine : { a : { b : { c : { d : { e : { f : { g : { h : { i : Int } } } } } } } } } -> Int\n\
         let nine = .a.b.c.d.e.f.g.h.i\n\n\
         use_nine : { a : { b : { c : { d : { e : { f : { g : { h : { i : Int } } } } } } } } } -> Int\n\
         let use_nine r = nine r + nine r\n\n\
         computes : User -> User -> Int\nlet computes a b =\n  let get = fun r -> r.address.zip + 1\n  get a + get b\n",
    );
    assert!(
        definition(&js, "use_sized").contains("sized(r) + sized(r)"),
        "{js}"
    );
    assert!(
        definition(&js, "use_nine").contains("nine(r) + nine(r)"),
        "{js}"
    );
    assert!(
        definition(&js, "computes").contains("get(a) + get(b)"),
        "{js}"
    );
}

#[test]
fn the_builtin_map_of_a_projection_reads_each_element_in_a_loop() {
    let js = app(
        "sugar, explicit, piped, local, get_city, named, over_for",
        "sugar : List User -> List String\nlet sugar users = List.map .address.city users\n\n\
         explicit : List User -> List String\nlet explicit users = List.map (fun u -> u.address.city) users\n\n\
         piped : List User -> List String\nlet piped users = users |> List.map .address.city\n\n\
         local : List User -> List Int\nlet local users =\n  let get = .address.zip\n  List.map get users\n\n\
         get_city : User -> String\nlet get_city = .address.city\n\n\
         named : List User -> List String\nlet named users = List.map get_city users\n\n\
         over_for : List User -> List String\n\
         let over_for users = List.map get_city [for u <- users if u.address.zip > 1 yield u]\n",
    );
    insta::assert_snapshot!(js);
}

/// A function that is not the built-in `List.map`, whatever its name, and
/// a function that is no projection stay calls.
#[test]
fn another_map_or_function_stays_a_call() {
    let js = app(
        "map, custom, passed, functor, computes",
        "map : (a -> b) -> List a -> List b\n\
         let map f xs = if List.is_empty xs then [] else List.reverse (List.map f xs)\n\n\
         custom : List User -> List String\nlet custom users = map .address.city users\n\n\
         passed : ((User -> String) -> List User -> List String) -> List User -> List String\n\
         let passed map users = map .address.city users\n\n\
         functor : Functor f => f User -> f String\nlet functor users = Prelude.map .address.city users\n\n\
         computes : List User -> List Int\nlet computes users = List.map (fun u -> u.address.zip + 1) users\n",
    );
    let custom = definition(&js, "custom");
    assert!(
        custom.contains("map$2(($x) => $x.address.city, users)"),
        "{js}"
    );
    let passed = definition(&js, "passed");
    assert!(passed.contains("map(") && !passed.contains("for ("), "{js}");
    let functor = definition(&js, "functor");
    assert!(
        functor.contains(".map(") && !functor.contains("for ("),
        "{js}"
    );
    let computes = definition(&js, "computes");
    assert!(
        computes.contains("$m_List.map(") && !computes.contains("for ("),
        "{js}"
    );
}

/// The release JS of `app`, a module of a build with the module `lib`,
/// which is not built in: its exported bodies are given as a build gives
/// them, with the program.
fn with_lib(lib: &str, app: &str) -> String {
    let (lib_ir, lib_interface) = check_text("Lib", lib).expect("lib lowers");
    let mut interfaces = builtins().to_vec();
    interfaces.push(lib_interface.clone());
    let (mut ir, interface) = check_text_with("App", app, &interfaces).expect("app lowers");
    let generics = |module: &str, name: &str| {
        if module == "Lib" {
            super::names_only(&lib_ir, name, &|n| lib_interface.exports_value(n))
        } else {
            builtin_generic(module, name)
        }
    };
    let arity = |module: &str, name: &str| {
        if module == "Lib" {
            (lib_ir.items.iter().filter_map(super::arity))
                .find_map(|(n, arity)| (n == name).then_some(arity))
        } else {
            builtin_arity(module, name)
        }
    };
    super::optimize(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &generics,
        &arity,
    );
    let resolved = resolved_by(&ir, &str::to_owned);
    let lang = lang_items();
    let known = Known::of(&lang);
    let options = Options {
        builtins: &[],
        paths: None,
        packages: &BTreeMap::new(),
        resolved: &resolved,
        typescript: None,
        release: true,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels: Labels::default(),
        rt: false,
        effect_bridge: EffectBridge::Absent,
        lang: &known,
    };
    js(&ir, &options).code
}

const LIB: &str = "module Lib exposing (Address, User, city, label)\n\n\
    type alias Address = { city : String; zip : Int }\n\n\
    type alias User = { name : String; address : Address }\n\n\
    city : User -> String\nlet city = .address.city\n\n\
    label : User -> String\nlet label u = u.name ++ \" of \" ++ u.address.city\n";

/// An exported projection of another module of the build is read where it
/// is used, from the body the build gives; a function there that is no
/// projection stays a call, as before.
#[test]
fn a_projection_of_another_module_is_inlined_from_its_body() {
    let js = with_lib(
        LIB,
        "module App exposing (one, all, labels)\n\n\
         import Lib exposing (User)\n\n\
         one : User -> String\nlet one u = Lib.city u\n\n\
         all : List User -> List String\nlet all users = List.map Lib.city users\n\n\
         labels : List User -> List String\nlet labels users = List.map Lib.label users\n",
    );
    insta::assert_snapshot!(js);
}
