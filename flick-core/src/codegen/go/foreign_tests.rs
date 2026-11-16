//! Tests for Go codegen foreign metadata lowering (catch, optional, variadic, this, callbacks, nullable).

use std::collections::BTreeMap;

use super::{Input, go};
use crate::build::builtin_modules;
use crate::build::go::bindings;
use crate::codegen::Known;
use crate::ir::tests::lower_text;
use crate::ir::{Module, lang_items};

fn app_with_foreign(text: &str, foreigns: &[(&str, &str, &str)]) -> String {
    let ir = lower_text("App", text).expect("lowers");
    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let mut foreign_map = BTreeMap::new();
    for (module, spec, content) in foreigns {
        foreign_map.insert(
            (module.to_string(), spec.to_string()),
            ("sibling.go".to_string(), content.to_string()),
        );
    }
    let out = go(&Input {
        own: &[&ir],
        builtins: &builtins,
        entry: &ir.name,
        suites: &[],
        test: false,
        paths: &BTreeMap::new(),
        lines: &[],
        foreign: &foreign_map,
        bindings: bindings(),
        lang: &Known::of(&lang_items()),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    out.files
        .into_iter()
        .find_map(|(name, go)| name.ends_with("m_App.go").then_some(go))
        .expect("the file of App")
}

#[test]
fn foreign_metadata_catch_lowering() {
    let code = "module App exposing (main)\ntype error DivErr = DivErr String\nfrom_err : JsValue -> DivErr\nlet from_err _ = DivErr \"div zero\"\nsafe_div : Float -> Float ->{; DivErr} Float\nforeign safe_div from \"./math.mjs\" \"SafeDiv\"\n  catch from_err\nmain : Unit ->{; DivErr} Float\nlet main () = safe_div 10.0 2.0\n";
    let go = app_with_foreign(code, &[("App", "./math.mjs", "package main\n")]);
    assert!(go.contains("flickrt.CatchForeign"), "{go}");
    assert!(go.contains("SafeDiv(fb, a0, a1)"), "{go}");
    assert!(go.contains("F_App__from_err"), "{go}");
}

#[test]
fn foreign_metadata_optional_lowering() {
    let code = "module App exposing (main)\ngreet : String -> Option String -> String\nforeign pure greet from \"./text.mjs\" \"Greet\"\n  optional 1\nmain : Unit -> String\nlet main () = greet \"Alice\" None\n";
    let go = app_with_foreign(code, &[("App", "./text.mjs", "package main\n")]);
    assert!(go.contains("Greet(fb, a0, flickrt.ToOptVal(a1))"), "{go}");
}

#[test]
fn foreign_metadata_variadic_lowering() {
    let code = "module App exposing (main)\nsum : List Float -> Float\nforeign pure sum from \"./math.mjs\" \"Sum\"\n  variadic\nmain : Unit -> Float\nlet main () = sum [1.0, 2.0]\n";
    let go = app_with_foreign(code, &[("App", "./math.mjs", "package main\n")]);
    assert!(go.contains("Sum(fb, flickrt.List(a0)...)"), "{go}");
}

#[test]
fn foreign_metadata_receiver_lowering() {
    let code = "module App exposing (main)\ntype alias Counter = String\ninc : Counter -> Int ->{} Counter\nforeign inc from \"./counter.mjs\" \"Inc\"\n  this\nmain : Unit ->{} Counter\nlet main () = inc \"c1\" 1\n";
    let go = app_with_foreign(code, &[("App", "./counter.mjs", "package main\n")]);
    assert!(
        go.contains("flickrt.CallMethod(fb, a0, \"Inc\", []flickrt.Value{a1})"),
        "{go}"
    );
}

#[test]
fn foreign_metadata_receiver_catch_lowering() {
    let code = "module App exposing (main)\ntype error MethodErr = MethodErr String\nfrom_err : JsValue -> MethodErr\nlet from_err _ = MethodErr \"err\"\ntype alias Counter = String\ninc : Counter -> Int ->{; MethodErr} Counter\nforeign inc from \"./counter.mjs\" \"Inc\"\n  this\n  catch from_err\nmain : Unit ->{; MethodErr} Counter\nlet main () = inc \"c1\" 1\n";
    let go = app_with_foreign(code, &[("App", "./counter.mjs", "package main\n")]);
    assert!(go.contains("flickrt.CatchForeign"), "{go}");
    assert!(
        go.contains("return flickrt.CallMethod(fb, a0, \"Inc\", []flickrt.Value{a1}), nil"),
        "{go}"
    );
    assert!(go.contains("F_App__from_err"), "{go}");
}

#[test]
fn foreign_metadata_callback_lowering() {
    let code = "module App exposing (main)\ncombine : (Float -> Float -> Float) -> Float -> Float -> Float\nforeign pure combine from \"./math.mjs\" \"Combine\"\nmain : Unit -> Float\nlet main () = combine (fun x y -> x + y) 1.0 2.0\n";
    let go = app_with_foreign(code, &[("App", "./math.mjs", "package main\n")]);
    assert!(
        go.contains("Combine(fb, flickrt.WrapCallback(fb, a0, 2), a1, a2)"),
        "{go}"
    );
}

#[test]
fn foreign_metadata_nullable_lowering() {
    let code = "module App exposing (main)\nfind : String -> Option String\nforeign pure find from \"./lookup.mjs\" \"Find\"\n  nullable\nmain : Unit -> Option String\nlet main () = find \"k\"\n";
    let go = app_with_foreign(code, &[("App", "./lookup.mjs", "package main\n")]);
    assert!(go.contains("flickrt.FromNullable(Find(fb, a0))"), "{go}");
}

#[test]
fn foreign_zero_arity_passes_fiber() {
    let code = "module App exposing (main)\ncyclic : Unit -> Unknown\nforeign pure cyclic from \"./inputs.mjs\" \"cyclic\"\n  arity 0\nmain : Unit -> Unknown\nlet main () = cyclic ()\n";
    let go = app_with_foreign(code, &[("App", "./inputs.mjs", "package main\n")]);
    assert!(go.contains("cyclic(fb)"), "{go}");
}

#[test]
fn foreign_zero_arity_async_passes_context() {
    let code = "module App exposing (main)\nstart : Unit ->{Async} Float\nforeign start from \"./receiver.mjs\" \"start\"\n  arity 0\nmain : Unit ->{Async} Float\nlet main () = start ()\n";
    let go = app_with_foreign(code, &[("App", "./receiver.mjs", "package main\n")]);
    assert!(go.contains("start(ctx)"), "{go}");
}
