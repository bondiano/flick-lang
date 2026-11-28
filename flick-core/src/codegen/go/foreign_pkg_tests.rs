//! Go codegen unit tests for direct Go package bindings and generated ABI conversions.

use std::collections::BTreeMap;

use super::{Input, Output, go};
use crate::build::builtin_modules;
use crate::build::go::bindings;
use crate::codegen::Known;
use crate::diagnostics::{Severity, codes};
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
        plugin_attributes: &[],
    };
    let (more, typing) = types::check_typed(&module, &builtin, builtins, options, &mut interface);
    diagnostics.extend(parse.diagnostics);
    diagnostics.extend(more);
    diagnostics.retain(|d| d.severity == Severity::Error);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let mut lowered = ir::lower(&module, &typing).expect("lowers");
    ir::fill_exports(&mut lowered, &interface);
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

#[test]
fn direct_go_non_nil_lang_opaque_nullable_and_nested_options() {
    let (_, go) = emit_go(
        r#"module App exposing (main)

maybe_json : Bool -> Option Json
foreign pure maybe_json from "example.com/pkg/abi" "MaybeJson"
  nullable

nested_json : Option (Option Json) -> Option (Option Json)
foreign pure nested_json from "example.com/pkg/abi" "NestedJson"

main : Unit -> Option (Option Json)
let main () =
  let _ = maybe_json true
  nested_json (Some None)
"#,
    );
    assert!(
        go.contains("func F_App__maybe_json(fb *flickrt.Fiber, a0 bool) flickrt.OptRef"),
        "{go}"
    );
    assert!(
        go.contains("flickrt.NullableToOptRef(abi.MaybeJson(a0))"),
        "{go}"
    );
    assert!(
        go.contains(
            "func F_App__nested_json(fb *flickrt.Fiber, a0 flickrt.Opt[flickrt.OptRef]) flickrt.Opt[flickrt.OptRef]"
        ),
        "{go}"
    );
    assert!(go.contains("flickrt.OptRefToFlag("), "{go}");
    assert!(
        go.contains("flickrt.StructToOptRef[flickrt.Value]("),
        "{go}"
    );
}

#[test]
fn direct_go_typed_opaque_assertions_and_this_methods() {
    let (out, go) = emit_go(
        r#"module App exposing (main)

type error GoErr = GoErr String

from_err : String -> GoErr
let from_err msg = GoErr msg

@go from "strings" "*Replacer"
type Replacer

@go from "io" "Reader"
type Reader

new_replacer : List String ->{} Replacer
foreign new_replacer from "strings" "NewReplacer"
  arity 1
  variadic

replacer_replace : Replacer -> String ->{} String
foreign replacer_replace from "strings" "Replacer.Replace"
  arity 2
  this

reset_reader : Reader ->{} Unit
foreign reset_reader from "example.com/pkg/wrap" "Wrap.Reset"
  arity 1
  this

fetch_with : Replacer -> String ->{Async; GoErr} String
foreign fetch_with from "example.com/pkg/wrap" "Client.Fetch"
  arity 2
  this
  catch from_err
  cancel context

main : Unit ->{Async; GoErr} String
let main () =
  let r = new_replacer ["a", "b"]
  let _ = replacer_replace r "aba"
  fetch_with r "x"
"#,
    );
    assert!(out.go_imports.contains("strings"), "{:?}", out.go_imports);
    assert!(
        !out.go_imports.contains("example.com/pkg/wrap"),
        "this method on typed receiver must not emit unused import of method's package: {:?}",
        out.go_imports
    );
    assert!(
        go.contains("a0.(*strings.Replacer).Replace(a1)"),
        "expected typed Go method call on *strings.Replacer:\n{go}"
    );
    assert!(
        go.contains("p0 := a0.(*strings.Replacer)") && go.contains("p0.Fetch(ctx, p1)"),
        "expected async typed Go method call with ctx:\n{go}"
    );
    assert!(
        !go.contains("\"example.com/pkg/wrap\""),
        "unused package import must not appear in Go file:\n{go}"
    );
    // Keep reset_reader referenced in a separate test to verify void this method on interface.
    let (out2, go2) = emit_go(
        r#"module App exposing (main)

@go from "io" "Reader"
type Reader

@go from "example.com/pkg/wrap" "*Holder"
type Holder

make_holder : Unit ->{} Holder
foreign make_holder from "example.com/pkg/wrap" "NewHolder"
  arity 0

pass_readers : Holder -> List Reader ->{} Unit
foreign pass_readers from "example.com/pkg/wrap" "Holder.Use"
  arity 2
  this

main : Unit ->{} Unit
let main () =
  let h = make_holder ()
  pass_readers h []
"#,
    );
    assert!(out2.go_imports.contains("io"), "{:?}", out2.go_imports);
    assert!(
        go2.contains("a0.(*wrap.Holder).Use(")
            && go2.contains(
                "flickrt.ListToGo(a1, func(v flickrt.Value) io.Reader { return v.(io.Reader) })"
            ),
        "expected typed opaque List conversion and typed method call:\n{go2}"
    );
}

#[test]
fn direct_go_this_method_requires_go_type_form_on_receiver() {
    for src in [
        r#"module App exposing (main)

type Replacer

replacer_replace : Replacer -> String ->{} String
foreign replacer_replace from "strings" "Replacer.Replace"
  arity 2
  this

main : Replacer ->{} String
let main r = replacer_replace r "aba"
"#,
        r#"module App exposing (main)

type Client

fetch : Client -> String ->{Async} String
foreign fetch from "example.com/pkg/wrap" "Client.Fetch"
  arity 2
  this
  cancel context

main : Client ->{Async} String
let main c = fetch c "x"
"#,
    ] {
        let ir = lower_go(src);
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
        assert!(
            out.diagnostics.iter().any(|d| d
                .message
                .contains("without a `@go` type form on its receiver")),
            "expected missing @go receiver diagnostic, got: {:?}",
            out.diagnostics
        );
    }
}

#[test]
fn malformed_go_type_form_with_numeric_args_is_rejected_with_s014() {
    for src in [
        "module App\n\n@go \"string\" 42\ntype Bad1\n",
        "module App\n\n@go -1 \"string\"\ntype Bad2\n",
    ] {
        let mut sources = SourceMap::default();
        let file = sources.add("App.fl", src);
        let parse = syntax::parse(src, file);
        let mod_syntax = resolve::module_syntax(&parse.tree, src, file);
        let module = resolve::Module {
            name: "App",
            file,
            text: src,
            tree: &parse.tree,
            syntax: &mod_syntax,
            generated: resolve::NO_GENERATED,
        };
        let builtins = types::builtins();
        let builtin = |name: &str| builtins.iter().find(|i| i.module == name).cloned();
        let (mut interface, mut diagnostics) = resolve::resolve(&module, &builtin);
        let local = |n: &str| n == "App";
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
            plugin_attributes: &[],
        };
        let (more, _) = types::check_typed(&module, &builtin, builtins, options, &mut interface);
        diagnostics.extend(parse.diagnostics);
        diagnostics.extend(more);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == codes::S014 && d.message.contains("`@go`")),
            "expected FLICK-S014 for `{src}`, got: {diagnostics:?}"
        );
    }
}
