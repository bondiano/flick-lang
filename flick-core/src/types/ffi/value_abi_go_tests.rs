//! Unit tests for the official `flick/go` value-ABI table (`flick.go-ffi@0.1.0`),
//! direct Go package checking (`FLICK-F013`), `catch` error mapping (`String -> E`),
//! `flickrt.Value` escape hatches (relative sibling `.go` and `from inline`),
//! `[package.abi]` enforcement (`FLICK-F014`), and EN/RU Markdown rendering.

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::resolve;
use crate::source::FileId;
use crate::syntax;
use crate::types::ffi::{DocLocale, ForeignBackendSpec, ValueAbiSpec, render_value_abi_markdown};
use crate::types::{self, Options, StaticContext, Watched};

fn check_go(source: &str) -> Vec<Diagnostic> {
    let file = FileId::default();
    let parsed = syntax::parse(source, file);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mod_syntax = resolve::module_syntax(&parsed.tree, source, file);
    let module = resolve::Module {
        name: "Probe",
        file,
        text: source,
        tree: &parsed.tree,
        syntax: &mod_syntax,
        generated: resolve::NO_GENERATED,
    };
    let imported = |_: &str| None;
    let (mut interface, resolved) = resolve::resolve(&module, &imported);
    assert!(resolved.is_empty(), "{resolved:?}");
    let local = |name: &str| name == "Probe";
    let watched = Watched::default();
    let backend = ForeignBackendSpec::go();
    let options = Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        watched: &watched,
        stages_by_class: true,
        foreign_backend: &backend,
        validate_foreign_metadata: true,
        native_overrides: None,
    };
    let (diags, _) = types::check_typed_with_static_context(
        &module,
        &imported,
        types::builtins_of(true),
        options,
        &mut interface,
        StaticContext {
            skip_ir_types: true,
            ..StaticContext::default()
        },
    );
    diags
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

#[test]
fn go_value_abi_admits_every_table_entry_and_opaque_handles() {
    let source = r#"module Probe exposing (..)

import Bytes exposing (Bytes)

type Handle
unboxed type Label = Label String
untagged type Mode = Fast = 1 | Slow = 2
type error IoErr = IoErr String

from_err : String -> IoErr
let from_err msg = IoErr msg

hypot : Float -> Float -> Float
foreign pure hypot from "math" "Hypot"

to_upper : String -> String
foreign pure to_upper from "strings" "ToUpper"

read_file : String ->{; IoErr} Bytes
foreign read_file from "os" "ReadFile"
  catch from_err

remove_file : String ->{; IoErr} Unit
foreign remove_file from "os" "Remove"
  catch from_err

wait_ctx : Int ->{Async; IoErr} String
foreign wait_ctx from "example.com/wait" "Wait"
  catch from_err
  cancel context

lookup_env : String ->{} Option String
foreign lookup_env from "os" "LookupEnv"

find_ptr : Int -> Option Int
foreign pure find_ptr from "example.com/opt" "FindPtr"
  nullable

nested_opt : Option (Option Int) -> Option (Option Bool)
foreign pure nested_opt from "example.com/opt" "Nested"

copy_lists : List Int -> List (List String) -> List Bytes
foreign pure copy_lists from "example.com/slice" "Transform"

opaque_and_unboxed : Handle -> Label -> Mode -> Option Handle
foreign pure opaque_and_unboxed from "example.com/handle" "Use"

zero_arg : Unit ->{} String
foreign zero_arg from "os" "Getwd"
  arity 0
"#;
    let diags = check_go(source);
    assert!(
        diags.is_empty(),
        "expected clean Go ABI check, got {diags:?}"
    );
}

#[test]
fn go_value_abi_refuses_out_of_table_types_at_span_with_table_reason() {
    let cases = [
        (
            "bigint",
            "module Probe exposing (f)\nf : BigInt -> Int\nforeign pure f from \"example.com/p\" \"F\"\n",
            "do not convert `BigInt`",
        ),
        (
            "record",
            "module Probe exposing (f)\nf : { x : Int } -> Int\nforeign pure f from \"example.com/p\" \"F\"\n",
            "no stable Go struct layout",
        ),
        (
            "adt",
            "module Probe exposing (f, Box)\ntype Box = Box Int\nf : Int -> Box\nforeign pure f from \"example.com/p\" \"F\"\n",
            "no stable Go layout",
        ),
        (
            "tuple",
            "module Probe exposing (f)\nf : (Int, String) -> Int\nforeign pure f from \"example.com/p\" \"F\"\n",
            "do not convert tuples",
        ),
        (
            "function_param",
            "module Probe exposing (f)\nf : (Int -> Int) -> Int\nforeign pure f from \"example.com/p\" \"F\"\n",
            "higher-order callback",
        ),
        (
            "result",
            "module Probe exposing (f)\nf : String -> Result String Int\nforeign pure f from \"example.com/p\" \"F\"\n",
            "trailing Go `error` via `catch`",
        ),
        (
            "js_value",
            "module Probe exposing (f)\nf : JsValue -> String\nforeign pure f from \"example.com/p\" \"F\"\n",
            "`JsValue` is specific to `flick/js`",
        ),
        (
            "unknown",
            "module Probe exposing (f)\nf : Unknown -> Int\nforeign pure f from \"example.com/p\" \"F\"\n",
            "do not convert `Unknown`",
        ),
        (
            "nested_in_list",
            "module Probe exposing (f)\nf : List { a : Int } -> Int\nforeign pure f from \"example.com/p\" \"F\"\n",
            "no stable Go struct layout",
        ),
        (
            "nested_in_option",
            "module Probe exposing (f)\nf : Option BigInt -> Int\nforeign pure f from \"example.com/p\" \"F\"\n",
            "do not convert `BigInt`",
        ),
        (
            "global_go_symbol",
            "module Probe exposing (f)\nf : { x : Int } -> Int\nforeign pure f from global \"example.Fn\"\n",
            "no stable Go struct layout",
        ),
        (
            "untagged_union_adt",
            "module Probe exposing (f)\nuntagged type Key = ByNum Float | ByName String\nf : Key -> String\nforeign pure f from \"example.com/p\" \"F\"\n",
            "no stable Go layout",
        ),
    ];
    for (label, source, expected) in cases {
        let diags = check_go(source);
        let f013 = diags
            .iter()
            .find(|d| d.code == codes::F013)
            .unwrap_or_else(|| panic!("{label}: expected FLICK-F013, got {diags:?}"));
        assert!(
            f013.message.contains("flick.go-ffi@0.1.0") && f013.message.contains(expected),
            "{label}: expected `{expected}` and `flick.go-ffi@0.1.0` in `{}`",
            f013.message
        );
        let span = f013.span.expect("span");
        let slice = &source[span.start as usize..span.end as usize];
        assert!(
            slice.starts_with("foreign "),
            "{label}: expected span at foreign decl, got `{slice}`"
        );
    }
}

#[test]
fn go_value_abi_admits_literal_enums_and_rejects_untagged_unions() {
    let ok = r#"module Probe exposing (..)

untagged type Mode = Fast = 1 | Slow = 2
untagged type Method = Get = "GET" | Post = "POST"

use_mode : Mode -> Method -> Int
foreign pure use_mode from "example.com/p" "UseMode"
"#;
    let diags = check_go(ok);
    assert!(
        diags.is_empty(),
        "literal enums must remain accepted on flick/go: {diags:?}"
    );
}

#[test]
fn go_sibling_and_inline_bindings_retain_flickrt_value_escape_hatch() {
    let source = r#"module Probe exposing (..)

type Box = Box Int

raw_record : { x : Int } -> Box
foreign pure raw_record from "./abi.mjs" "rawRecord"

raw_js : JsValue -> String
foreign pure raw_js from "./abi.mjs" "kind"

inline_adt : Box -> (Int, BigInt)
foreign pure inline_adt from inline "func(fb *flickrt.Fiber, x flickrt.Value) flickrt.Value { return x }"
"#;
    let diags = check_go(source);
    assert!(
        diags.is_empty(),
        "sibling `.go` and `from inline` must stay allowed for out-of-table types: {diags:?}"
    );
}

#[test]
fn go_value_abi_markdown_renderer_outputs_en_and_ru_tables() {
    let table = ValueAbiSpec::go();
    let en = render_value_abi_markdown(&table, DocLocale::En);
    let ru = render_value_abi_markdown(&table, DocLocale::Ru);
    assert!(en.contains("`flick.go-ffi@0.1.0`"), "{en}");
    assert!(en.contains("`flickrt.Value`"), "{en}");
    assert!(en.contains("`error` (lang item `string`)"), "{en}");
    for key in [
        "unit", "bool", "int", "float", "string", "bytes", "list", "option",
    ] {
        assert!(
            en.contains(&format!("| `{key}` |")),
            "missing `{key}` in EN:\n{en}"
        );
        assert!(
            ru.contains(&format!("| `{key}` |")),
            "missing `{key}` in RU:\n{ru}"
        );
    }
    assert!(ru.contains("Копия среза Go `[]byte`"), "{ru}");
    assert!(ru.contains("прямые привязки отклоняют их"), "{ru}");
}

#[cfg(feature = "native-project")]
#[test]
fn go_package_abi_requirement_matches_and_rejects_mismatch_with_f014() {
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};

    use crate::project::{Host, Project, check, discover};
    use crate::source::SourceMap;

    struct MemHost(BTreeMap<PathBuf, Vec<u8>>);
    impl Host for MemHost {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
        }
        fn display(&self, path: &Path) -> String {
            path.display().to_string()
        }
    }

    let run = |toml: &str| {
        let files = [
            (PathBuf::from("/app/flick.toml"), toml.as_bytes().to_vec()),
            (
                PathBuf::from("/app/src/Main.fl"),
                b"module Main exposing (x)\nx : Int\nlet x = 1\n".to_vec(),
            ),
        ];
        let host = MemHost(files.into_iter().collect());
        let mut sources = SourceMap::default();
        let project: Project = discover(&host, &mut sources, Path::new("/app"), None)
            .expect("parses")
            .expect("found");
        check(
            &host,
            &mut sources,
            Some(&project),
            &[PathBuf::from("/app/src/Main.fl")],
        )
    };

    let ok_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[build]
target = \"go\"
[package]
name = \"demo\"
version = \"0.1.0\"
[package.abi]
\"flick.go-ffi\" = \"^0.1.0\"
";
    let ok = run(ok_toml);
    assert!(ok.is_empty(), "{ok:?}");

    let bad_toml = ok_toml.replace("^0.1.0", "^2.0.0");
    let bad = run(&bad_toml);
    assert!(
        bad.iter()
            .any(|d| d.code == codes::F014 && d.message.contains("flick.go-ffi@0.1.0")),
        "expected FLICK-F014, got {bad:?}"
    );
}
