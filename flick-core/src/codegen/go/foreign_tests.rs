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
        sources: &[],
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

mod bindings_tests {
    use crate::codegen::go::Bindings;
    use crate::ir::ItemKind;
    use crate::ir::tests::lower_text;

    #[test]
    fn staged_bindings_return_the_remaining_functions_and_others_still_flatten() {
        let ir = lower_text("App", "module App exposing (..)\nmount : Int -> Int -> Int\nforeign pure mount from \"mounts\" \"mount\"\n  arity 1\nflat : Int -> Int -> Int\nforeign pure flat from \"mounts\" \"flat\"\n  arity 1\n").expect("lowers");
        let bindings = Bindings::parse(["[mounts]\nmount *Mount@1\nflat Flat\n"]);
        let mut checked = 0;
        for item in &ir.items {
            if let ItemKind::Foreign(f) = &item.kind {
                assert_eq!(
                    bindings.parameters("App", f),
                    if f.name == "mount" { 1 } else { 2 }
                );
                if f.name == "mount" {
                    let mut global = f.clone();
                    global.name = "local".to_owned();
                    global.binding = flick_ir::Binding::Global {
                        name: "external".to_owned(),
                    };
                    let globals = Bindings::parse(["[global App]\nlocal Global@1\n"]);
                    assert_eq!(globals.parameters("App", &global), 1);
                    assert!(globals.lookup("App", "local", None, "external").is_some());
                }
                checked += 1;
            }
        }
        assert_eq!(checked, 2);
        assert_eq!(
            bindings
                .lookup("App", "mount", Some("mounts"), "mount")
                .map(|(target, _)| target),
            Some("*flickrt.Mount")
        );
    }
}

mod owner_identity_tests {
    use crate::codegen::go::{Bindings, Types};
    use crate::ir::ItemKind;
    use crate::ir::tests::lower_text;

    #[test]
    fn raw_owner_shim_retains_global_foreign_table_identity() {
        let module = lower_text("Test", "module Test exposing (acquire)\nacquire : Unit ->{Scope s} Float\nforeign effect acquire from global \"Object.acquire\"\n").expect("global scoped foreign");
        let raw = module
            .items
            .iter()
            .find_map(|item| match &item.kind {
                ItemKind::Foreign(f) => Some(f),
                _ => None,
            })
            .expect("raw item");
        assert_ne!(raw.name, "acquire");
        assert_eq!(raw.source_name.as_deref(), Some("acquire"));
        assert_eq!(raw.binding.global_name(), Some("Object.acquire"));
        let bindings = Bindings::parse(["[global Test]\nacquire NativeAcquire(float64) float64\n"]);
        assert_ne!(
            bindings.types("Test", raw),
            Types::default(),
            "lookup must use original source name, not emitted raw symbol"
        );
        let staged = Bindings::parse(["[global Test]\nacquire NativeAcquire@0\n"]);
        assert_eq!(staged.parameters("Test", raw), 0);
    }

    #[test]
    fn flattened_raw_owner_shim_uses_the_flattened_source_identity() {
        let module = lower_text("Test", "module Test exposing (make)\nmake : Unit ->{Scope s} Float\nforeign effect make from global \"Object.make\"\n").expect("scoped foreign");
        let mut raw = module
            .items
            .iter()
            .find_map(|item| match &item.kind {
                ItemKind::Foreign(foreign) => Some(foreign.clone()),
                _ => None,
            })
            .expect("raw item");
        raw.name = format!("Inner${}", raw.name);
        raw.source_name = Some("Inner$make".to_owned());
        let bindings = Bindings::parse([
            "[global Test]\nmake ParentMake(string) string\nInner$make InnerMake(float64) float64\n",
        ]);
        let inner = bindings.types("Test", &raw);
        raw.source_name = Some("make".to_owned());
        assert_ne!(inner, Types::default());
        assert_ne!(
            inner,
            bindings.types("Test", &raw),
            "inline source identity must not select a parent homonym"
        );
        assert_eq!(
            raw.binding.global_name(),
            Some("Object.make"),
            "relocation leaves the host binding unchanged"
        );
    }
}
