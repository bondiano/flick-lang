//! Go codegen unit tests for direct Go package bindings and generated ABI conversions.

use std::collections::BTreeMap;

use super::{Input, Output, go};
use crate::build::builtin_modules;
use crate::build::go::bindings;
use crate::codegen::Known;
use crate::diagnostics::Severity;
use crate::ir::{Module, lang_items};
use crate::plugin::derive::builtin_expansion;
use crate::resolve;
use crate::source::SourceMap;
use crate::types::{self, ForeignBackendSpec, Watched};
use crate::{ir, syntax};

fn lower_go(text: &str) -> Module {
    let mut sources = SourceMap::default();
    let file = sources.add("App.fl", text);
    let parse = syntax::parse(text, file);
    let mod_syntax = resolve::module_syntax(&parse.tree, text, file);
    let module_name = mod_syntax
        .header
        .as_ref()
        .map_or("App", |h| h.name.as_str());
    let (expansion, derived) = builtin_expansion(text, file, &parse.tree, module_name, &mod_syntax);
    assert!(derived.is_empty(), "{derived:?}");
    let module = resolve::Module {
        name: module_name,
        file,
        text: expansion.as_ref().map_or(text, |e| &e.text),
        tree: expansion.as_ref().map_or(&parse.tree, |e| &e.tree),
        syntax: &mod_syntax,
        generated: expansion.as_ref().map_or(resolve::NO_GENERATED, |e| e.len),
    };
    let builtins = types::builtins();
    let builtin = |name: &str| builtins.iter().find(|i| i.module == name).cloned();
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &builtin);
    let local = |n: &str| n == module_name;
    let go_spec = ForeignBackendSpec::go();
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        watched: Watched::none(),
        stages_by_class: false,
        foreign_backend: &go_spec,
        validate_foreign_metadata: true,
        native_overrides: None,
    };
    let (more, typing) = types::check_typed(&module, &builtin, builtins, options, &mut interface);
    diagnostics.extend(parse.diagnostics);
    diagnostics.extend(more);
    diagnostics.retain(|d| d.severity == Severity::Error);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let mut lowered = ir::lower(&module, &typing).expect("lowers");
    lowered.exports = ir::exports(module_name, &interface);
    lowered
}

fn emit_go(text: &str) -> (Output, String) {
    let ir = lower_go(text);
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
    let file = out
        .files
        .iter()
        .find_map(|(name, go)| name.ends_with("m_App.go").then(|| go.clone()))
        .expect("m_App.go");
    (out, file)
}

#[test]
fn direct_go_stdlib_imports_and_scalar_conversions() {
    let (out, go) = emit_go(
        r#"module App exposing (main)

hypot : Float -> Float -> Float
foreign pure hypot from "math" "Hypot"

to_upper : String -> String
foreign pure to_upper from "strings" "ToUpper"

pi : Float
foreign pure pi from "math" "Pi"

main : Unit -> Float
let main () =
  let _ = to_upper "hi"
  hypot 3.0 4.0 + pi
"#,
    );
    assert!(out.go_imports.contains("math"), "{:?}", out.go_imports);
    assert!(out.go_imports.contains("strings"), "{:?}", out.go_imports);
    assert!(go.contains("\"math\""), "{go}");
    assert!(go.contains("\"strings\""), "{go}");
    assert!(go.contains("math.Hypot(a0, a1)"), "{go}");
    assert!(go.contains("strings.ToUpper(a0)"), "{go}");
    assert!(go.contains("math.Pi"), "{go}");
}

#[test]
fn direct_go_bytes_and_nested_list_copy_conversions() {
    let (_, go) = emit_go(
        r#"module App exposing (main)

import Bytes exposing (Bytes)

echo_bytes : Bytes -> Bytes
foreign pure echo_bytes from "example.com/pkg/abi" "EchoBytes"

echo_matrix : List (List Int) -> List (List Int)
foreign pure echo_matrix from "example.com/pkg/abi" "EchoMatrix"

sum_all : List Int -> Int
foreign pure sum_all from "example.com/pkg/abi" "SumAll"
  variadic

main : Unit -> Int
let main () =
  let _ = echo_bytes (Bytes.encode_utf8 "hi")
  let _ = echo_matrix [[1, 2]]
  sum_all [1, 2, 3]
"#,
    );
    assert!(go.contains("flickrt.BytesToGo"), "{go}");
    assert!(go.contains("flickrt.BytesFromGo"), "{go}");
    assert!(go.contains("flickrt.ListToGo"), "{go}");
    assert!(go.contains("flickrt.ListFromGo"), "{go}");
    assert!(
        go.contains("abi.SumAll(flickrt.ListToGo(a0, flickrt.AsInt)...)"),
        "{go}"
    );
}

#[test]
fn direct_go_option_pointer_comma_ok_and_nested_flag_preservation() {
    let (_, go) = emit_go(
        r#"module App exposing (main)

lookup : String -> Option String
foreign pure lookup from "example.com/pkg/abi" "Lookup"

nullable_ptr : Option Int -> Option Int
foreign pure nullable_ptr from "example.com/pkg/abi" "NullablePtr"
  nullable

nested : Option (Option Int) -> Option (Option Int)
foreign pure nested from "example.com/pkg/abi" "Nested"

main : Unit -> Option (Option Int)
let main () =
  let _ = lookup "k"
  let _ = nullable_ptr (Some 1)
  nested (Some None)
"#,
    );
    assert!(go.contains("r0, r1 := abi.Lookup(a0)"), "{go}");
    assert!(go.contains("flickrt.FlagToOpt(r0, r1)"), "{go}");
    assert!(go.contains("flickrt.OptToPtr(a0)"), "{go}");
    assert!(go.contains("flickrt.PtrToOpt(abi.NullablePtr("), "{go}");
    assert!(go.contains("flickrt.OptFlagMap(a0,"), "{go}");
    assert!(go.contains("flickrt.FlagToOptMap("), "{go}");
}

#[test]
fn direct_go_trailing_error_and_async_context_cancellation() {
    let (_, go) = emit_go(
        r#"module App exposing (main)

import Bytes exposing (Bytes)

type error FsErr = FsErr String

from_err : String -> FsErr
let from_err msg = FsErr msg

read_file : String ->{; FsErr} Bytes
foreign read_file from "os" "ReadFile"
  catch from_err

touch : String ->{; FsErr} Unit
foreign touch from "example.com/pkg/abi" "Touch"
  catch from_err

wait_msg : Int ->{Async; FsErr} String
foreign wait_msg from "example.com/pkg/abi" "WaitMsg"
  catch from_err

main : Unit ->{Async; FsErr} String
let main () =
  let _ = read_file "a"
  let _ = touch "b"
  wait_msg 5
"#,
    );
    assert!(go.contains("flickrt.CatchForeign("), "{go}");
    assert!(go.contains("r0, err := os.ReadFile(a0)"), "{go}");
    assert!(go.contains("err := abi.Touch(a0)"), "{go}");
    assert!(go.contains("flickrt.ForeignAsyncCatch("), "{go}");
    assert!(go.contains("r0, err := abi.WaitMsg(ctx, p0)"), "{go}");
}

#[test]
fn direct_go_import_aliasing_handles_major_versions_and_collisions() {
    let (out, go) = emit_go(
        r#"module App exposing (main)

a : Int -> Int
foreign pure a from "example.com/alpha/v2" "Run"

b : Int -> Int
foreign pure b from "example.com/beta/v2" "Run"

c : Int -> Int
foreign pure c from "example.com/type" "Check"

main : Unit -> Int
let main () = a 1 + b 2 + c 3
"#,
    );
    assert!(out.go_imports.contains("example.com/alpha/v2"));
    assert!(out.go_imports.contains("example.com/beta/v2"));
    assert!(out.go_imports.contains("example.com/type"));
    assert!(go.contains("alpha \"example.com/alpha/v2\""), "{go}");
    assert!(go.contains("beta \"example.com/beta/v2\""), "{go}");
    assert!(go.contains("type_0 \"example.com/type\""), "{go}");
    assert!(go.contains("alpha.Run(a0)"), "{go}");
    assert!(go.contains("beta.Run(a0)"), "{go}");
    assert!(go.contains("type_0.Check(a0)"), "{go}");
}
