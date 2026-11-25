//! Unit tests for JS library `index.d.ts` header projection (`library_declaration_file`).

use super::{HostExportSpec, library_declaration_file};
use crate::diagnostics::Severity;
use crate::iface::Interface;
use crate::resolve::{self, Module};
use crate::source::{FileId, SourceMap, Span};
use crate::syntax;
use crate::types::{self, ForeignBackendSpec, Typing, Watched};

fn check_module(name: &str, text: &str) -> (Typing, Interface) {
    let mut sources = SourceMap::default();
    let file = sources.add(format!("{name}.fl"), text);
    let parse = syntax::parse(text, file);
    assert!(parse.diagnostics.is_empty(), "{:?}", parse.diagnostics);
    let syn = resolve::module_syntax(&parse.tree, text, file);
    let module_name = syn.header.as_ref().map_or(name, |h| h.name.as_str());
    let module = Module {
        name: module_name,
        file,
        text,
        tree: &parse.tree,
        syntax: &syn,
        generated: resolve::NO_GENERATED,
    };
    let builtins = types::builtins_of(true);
    let builtin = |m: &str| builtins.iter().find(|i| i.module == m).cloned();
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &builtin);
    let local = |n: &str| n == module_name;
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        watched: Watched::none(),
        stages_by_class: false,
        foreign_backend: ForeignBackendSpec::default_js(),
        validate_foreign_metadata: true,
        native_overrides: None,
    };
    let (more, typing) = types::check_typed(&module, &builtin, builtins, options, &mut interface);
    diagnostics.extend(more);
    diagnostics.retain(|d| d.severity == Severity::Error);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    (typing, interface)
}

#[test]
fn projects_renamed_and_private_exports_with_iife_global() {
    let source = "\
module App.Math exposing (add)

/// Adds two numbers.
@export \"addNumbers\"
add : Int -> Int -> Int
let add a b = a + b

/// Private helper exported to host.
@export \"doubleVal\"
double_val : Int -> Int
let double_val x = x * 2
";
    let (typing, iface) = check_module("App.Math", source);
    let decls = (*typing.declarations).clone();
    let lookup = |m: &str| -> Option<Interface> {
        if m == "App.Math" {
            return Some(iface.clone());
        }
        types::builtins_of(true)
            .iter()
            .find(|i| i.module == m)
            .cloned()
    };
    let lookup_decls = |m: &str| -> Option<Interface> {
        (m == "App.Math").then(|| decls.clone())
    };
    let specs: Vec<HostExportSpec<'_>> = typing
        .host_exports
        .iter()
        .map(|e| HostExportSpec {
            export_name: e.export_name.as_str(),
            module: "App.Math",
            symbol: e.name.as_str(),
            span: e.span,
        })
        .collect();
    let (dts, used, refused) = library_declaration_file(
        &specs,
        &lookup,
        &lookup_decls,
        &|_| Some("demo-lib".to_owned()),
        &|m| format!("./{}.js", m.replace('.', "/")),
        &|_, s| s.to_owned(),
        true,
        Some("DemoMath"),
    );
    assert!(refused.is_empty(), "unexpected refusals: {refused:?}");
    assert!(used.is_empty(), "primitive-only exports need no module imports");
    assert!(dts.contains("/** Adds two numbers. */"), "{dts}");
    assert!(
        dts.contains("export declare const addNumbers: (arg: number) => (arg: number) => number;"),
        "{dts}"
    );
    assert!(dts.contains("/** Private helper exported to host. */"), "{dts}");
    assert!(
        dts.contains("export declare const doubleVal: (arg: number) => number;"),
        "{dts}"
    );
    assert!(dts.contains("declare global {"), "{dts}");
    assert!(dts.contains("var DemoMath: {"), "{dts}");
    assert!(
        dts.contains("readonly addNumbers: (arg: number) => (arg: number) => number;"),
        "{dts}"
    );
    assert!(!dts.contains("any"), "must never weaken to any: {dts}");
}

#[test]
fn refuses_unprojectable_export_at_exact_attribute_span_without_any() {
    let source = "\
module App.Opaque exposing (Cursor, step)

@ts none
type Cursor

@export \"stepCursor\"
step : Cursor -> Int
let step _ = 1
";
    let (typing, iface) = check_module("App.Opaque", source);
    let decls = (*typing.declarations).clone();
    let lookup = |m: &str| -> Option<Interface> {
        if m == "App.Opaque" {
            return Some(iface.clone());
        }
        types::builtins_of(true)
            .iter()
            .find(|i| i.module == m)
            .cloned()
    };
    let lookup_decls = |m: &str| -> Option<Interface> {
        (m == "App.Opaque").then(|| decls.clone())
    };
    let export = &typing.host_exports[0];
    let spec = [HostExportSpec {
        export_name: export.export_name.as_str(),
        module: "App.Opaque",
        symbol: export.name.as_str(),
        span: export.span,
    }];
    let (dts, _used, refused) = library_declaration_file(
        &spec,
        &lookup,
        &lookup_decls,
        &|_| None,
        &|m| format!("./{}.js", m.replace('.', "/")),
        &|_, s| s.to_owned(),
        true,
        None,
    );
    assert_eq!(refused.len(), 1, "expected 1 refusal: {refused:?}");
    assert_eq!(refused[0].export_name, "stepCursor");
    assert_eq!(
        refused[0].span,
        Span::new(FileId::new(0), export.span.start, export.span.end)
    );
    assert_eq!(
        &source[refused[0].span.start as usize..refused[0].span.end as usize],
        "@export \"stepCursor\""
    );
    assert!(!dts.contains("any"), "refused export must never emit any: {dts}");
}
