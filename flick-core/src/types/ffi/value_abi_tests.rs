//! Unit tests for backend value-ABI checking (`FLICK-F013`), `catch` host error
//! lang items, `[package] abi` mismatch diagnostics (`FLICK-F014`), and EN/RU
//! Markdown rendering from value-ABI data.

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::resolve;
use crate::source::FileId;
use crate::syntax;
use crate::types::ffi::{
    AbiConversion, AbiDirection, ForeignBackendSpec, ValueAbiEntrySpec, ValueAbiId,
    ValueAbiRefusalSpec, ValueAbiSpec,
};
use crate::types::{self, Options, StaticContext, Watched};

fn strict_backend() -> ForeignBackendSpec {
    let row = |key: &str,
               flick_type: &str,
               host_type: &str,
               conversion: AbiConversion,
               direction: AbiDirection| ValueAbiEntrySpec {
        key: key.into(),
        flick_type: flick_type.into(),
        host_type: host_type.into(),
        conversion,
        direction,
        notes: String::new(),
    };
    ForeignBackendSpec {
        backend_id: "test/strict".into(),
        adapters: Vec::new(),
        type_form_attributes: Vec::new(),
        value_abi: Some(ValueAbiSpec {
            abi: ValueAbiId {
                name: "test.strict-ffi".into(),
                version: "0.1.0".into(),
            },
            opaque_type: "void *".into(),
            catch_error_type: "const char *".into(),
            catch_lang_item: Some("string".into()),
            entries: vec![
                row(
                    "unit",
                    "Unit",
                    "void",
                    AbiConversion::None,
                    AbiDirection::Both,
                ),
                row(
                    "bool",
                    "Bool",
                    "bool",
                    AbiConversion::None,
                    AbiDirection::Both,
                ),
                row(
                    "int",
                    "Int",
                    "int32_t",
                    AbiConversion::None,
                    AbiDirection::Both,
                ),
                row(
                    "float",
                    "Float",
                    "double",
                    AbiConversion::None,
                    AbiDirection::Both,
                ),
                row(
                    "string",
                    "String",
                    "const char *",
                    AbiConversion::View,
                    AbiDirection::In,
                ),
                row(
                    "function",
                    "a -> b",
                    "void (*)(...)",
                    AbiConversion::None,
                    AbiDirection::In,
                ),
            ],
            unsupported: vec![
                ValueAbiRefusalSpec {
                    key: "bigint".into(),
                    reason: "strict backend has no BigInt ABI".into(),
                },
                ValueAbiRefusalSpec {
                    key: "record".into(),
                    reason: "strict backend forbids records".into(),
                },
                ValueAbiRefusalSpec {
                    key: "adt".into(),
                    reason: "strict backend forbids ADTs".into(),
                },
            ],
            fallback_reason: "type is outside test.strict-ffi".into(),
            admits_copied_data: false,
        }),
    }
}

fn check_with(source: &str, backend: &ForeignBackendSpec) -> Vec<Diagnostic> {
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
    let options = Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        watched: &watched,
        stages_by_class: true,
        foreign_backend: backend,
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
fn strict_value_abi_admits_table_types_opaque_handles_unboxed_and_optional() {
    let source = r#"module Probe exposing (..)

type Handle
opaque type alias Token = Int
unboxed type UserId = UserId Int
type alias Score = Float
untagged type Mode = Fast = 1 | Slow = 2
type error NativeErr = NativeErr String

from_err : String -> NativeErr
let from_err msg = NativeErr msg

ok_direct : Handle -> Token -> UserId -> Score -> Mode -> String -> (Int -> Unit) ->{; NativeErr} Int
foreign ok_direct from "./lib.c" "ok_direct"
  arity 7
  catch from_err

ok_optional_nullable : Option Int -> Option Float
foreign pure ok_optional_nullable from "./lib.c" "ok_opt"
  optional 0
  nullable

ok_zero_arity : Unit ->{} Int
foreign ok_zero_arity from "./lib.c" "ok_zero"
  arity 0

ok_poly : forall a. a -> a
foreign pure ok_poly from "./lib.c" "ok_poly"
"#;
    let diags = check_with(source, &strict_backend());
    assert!(diags.is_empty(), "expected clean check, got {diags:?}");
}

#[test]
fn strict_value_abi_refuses_disallowed_types_and_direction_mismatches() {
    let cases = [
        (
            "bigint_param",
            "module Probe exposing (f)\nf : BigInt -> Int\nforeign pure f from \"./lib.c\" \"f\"\n",
            "strict backend has no BigInt ABI",
        ),
        (
            "local_opaque_alias_to_bigint",
            "module Probe exposing (f, Secret)\nopaque type alias Secret = BigInt\nf : Secret -> Int\nforeign pure f from \"./lib.c\" \"f\"\n",
            "strict backend has no BigInt ABI",
        ),
        (
            "record_param",
            "module Probe exposing (f)\nf : { x : Int } -> Int\nforeign pure f from \"./lib.c\" \"f\"\n",
            "strict backend forbids records",
        ),
        (
            "adt_result",
            "module Probe exposing (f, Box)\ntype Box = Box Int\nf : Int -> Box\nforeign pure f from \"./lib.c\" \"f\"\n",
            "strict backend forbids ADTs",
        ),
        (
            "list_fallback",
            "module Probe exposing (f)\nf : List Int -> Int\nforeign pure f from \"./lib.c\" \"f\"\n",
            "type is outside test.strict-ffi",
        ),
        (
            "unboxed_bad_inner",
            "module Probe exposing (f, BigId)\nunboxed type BigId = BigId BigInt\nf : BigId -> Int\nforeign pure f from \"./lib.c\" \"f\"\n",
            "strict backend has no BigInt ABI",
        ),
        (
            "direction_out_refused_for_in_only_string",
            "module Probe exposing (f)\nf : Int -> String\nforeign pure f from \"./lib.c\" \"f\"\n",
            "allowed only in `in` direction, not `out`",
        ),
        (
            "direction_out_refused_for_in_only_function",
            "module Probe exposing (f)\nf : Int -> (Int -> Int)\nforeign pure f from \"./lib.c\" \"f\"\n  arity 1\n",
            "allowed only in `in` direction, not `out`",
        ),
        (
            "callback_flipped_polarity_rejects_in_only_string_param",
            "module Probe exposing (f)\nf : (String -> Unit) ->{} Unit\nforeign f from \"./lib.c\" \"f\"\n",
            "allowed only in `in` direction, not `out`",
        ),
        (
            "callback_result_checks_in_direction",
            "module Probe exposing (f)\nf : (Int -> BigInt) ->{} Unit\nforeign f from \"./lib.c\" \"f\"\n",
            "strict backend has no BigInt ABI",
        ),
    ];
    let backend = strict_backend();
    for (label, source, expected) in cases {
        let diags = check_with(source, &backend);
        let f013 = diags
            .iter()
            .find(|d| d.code == codes::F013)
            .unwrap_or_else(|| panic!("{label}: expected FLICK-F013, got {diags:?}"));
        assert!(
            f013.message.contains(expected),
            "{label}: expected `{expected}` in `{}`",
            f013.message
        );
        let span = f013.span.expect("span");
        assert!(
            source[span.start as usize..span.end as usize].starts_with("foreign "),
            "{label}: unexpected span `{}`",
            &source[span.start as usize..span.end as usize]
        );
    }
}

#[test]
fn js_value_abi_admits_copied_records_and_adts() {
    let source = r#"module Probe exposing (..)

type alias User = { name : String; tags : List String }
type Deep = Leaf | Branch (List Deep)

rename : User ->{} Unit
foreign rename from "./bindings.mjs" "rename"

deep : Deep -> Deep
foreign pure deep from "./bindings.mjs" "deep"
"#;
    let diags = check_with(source, &ForeignBackendSpec::js());
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn nonregular_recursive_opaque_aliases_terminate_value_abi_check() {
    let source = r#"module Probe exposing (make, make_idle)

type alias Reveal a = Errors {a | none}
opaque type alias Repeat a s = Eff {Scope s} none (Reveal (Repeat (List a) s))
opaque type alias Idle a = forall b. Idle (List a)
opaque type alias IdleTask s = Eff {Scope s} none (Idle Int)

make : Fiber.Owner s -> Repeat Int s
foreign pure make from "./owner.mjs" "make"
  retention call

make_idle : Fiber.Owner s -> IdleTask s
foreign pure make_idle from "./owner.mjs" "make_idle"
  retention call
"#;
    let diags = check_with(source, &ForeignBackendSpec::js());
    assert!(
        diags.is_empty(),
        "JS value ABI check should terminate cleanly on recursive opaque aliases: {diags:?}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn value_abi_markdown_renderer_outputs_en_and_ru_tables_from_data() {
    use crate::types::ffi::{DocLocale, render_value_abi_markdown, replace_value_abi_block};

    let js = ValueAbiSpec::js();
    let en = render_value_abi_markdown(&js, DocLocale::En);
    let ru = render_value_abi_markdown(&js, DocLocale::Ru);
    assert!(en.contains("`flick.javascript-ffi@0.1.0`"));
    assert!(en.contains("| `unit` | `Unit` | `undefined` | `none` | `both` |"));
    assert!(en.contains("| `function` |"));
    assert!(!en.contains("| `record` |"));
    assert!(!en.contains("| `adt` |"));
    assert!(ru.contains("`flick.javascript-ffi@0.1.0`"));
    assert!(ru.contains("| `unit` | `Unit` | `undefined` | `none` | `both` |"));
    assert!(ru.contains("Записи и пользовательские ADT"));

    let doc = "before\n<!-- value-abi:flick/js:en:start -->\nold\n<!-- value-abi:flick/js:en:end -->\nafter\n";
    let synced = replace_value_abi_block(doc, "flick/js:en", &en).expect("markers found");
    assert!(synced.starts_with("before\n<!-- value-abi:flick/js:en:start -->\n"));
    assert!(synced.ends_with("<!-- value-abi:flick/js:en:end -->\nafter\n"));
    assert!(synced.contains("| `int` | `Int` | `number` | `none` | `both` |"));
}

#[cfg(feature = "native-project")]
mod package_abi_tests {
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};

    use crate::diagnostics::{Diagnostic, codes};
    use crate::project::{Host, Project, check, discover};
    use crate::source::SourceMap;

    struct MemHost(BTreeMap<PathBuf, Vec<u8>>);

    impl MemHost {
        fn new(files: &[(&str, &str)]) -> Self {
            Self(
                files
                    .iter()
                    .map(|(p, t)| (PathBuf::from(p), t.as_bytes().to_vec()))
                    .collect(),
            )
        }
    }

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

    fn run(files: &[(&str, &str)]) -> (Vec<Diagnostic>, SourceMap) {
        let host = MemHost::new(files);
        let mut sources = SourceMap::default();
        let project: Project = discover(&host, &mut sources, Path::new("/app"), None)
            .expect("manifest parses")
            .expect("project found");
        let diags = check(
            &host,
            &mut sources,
            Some(&project),
            &[PathBuf::from("/app/src/Main.fl")],
        );
        (diags, sources)
    }

    #[test]
    fn matching_package_abi_passes_and_mismatch_emits_f014_at_manifest_span() {
        let main_src = "module Main exposing (x)\nx : Int\nlet x = 1\n";
        let ok_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[package]
name = \"demo\"
version = \"0.1.0\"
[package.abi]
\"flick.javascript-ffi\" = \"^0.1.0\"
";
        let (ok_diags, _) = run(&[("/app/flick.toml", ok_toml), ("/app/src/Main.fl", main_src)]);
        assert!(ok_diags.is_empty(), "{ok_diags:?}");

        let bad_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[package]
name = \"demo\"
version = \"0.1.0\"
[package.abi]
\"flick.javascript-ffi\" = \"^2.0.0\"
";
        let (bad_diags, sources) = run(&[
            ("/app/flick.toml", bad_toml),
            ("/app/src/Main.fl", main_src),
        ]);
        let f014 = bad_diags
            .iter()
            .find(|d| d.code == codes::F014)
            .unwrap_or_else(|| panic!("expected FLICK-F014, got {bad_diags:?}"));
        assert!(
            f014.message.contains("flick.javascript-ffi"),
            "{}",
            f014.message
        );
        assert!(f014.message.contains("^2.0.0"), "{}", f014.message);
        assert!(f014.message.contains("0.1.0"), "{}", f014.message);
        let span = f014.span.expect("span");
        let text = &sources.get(span.file).text[span.start as usize..span.end as usize];
        assert_eq!(text, "\"^2.0.0\"");
    }

    #[test]
    fn dependency_package_abi_mismatch_emits_f014_and_inactive_when_package_is_skipped() {
        let app_toml = "\
schema-version = 1
[project]
kind = \"lib\"
[dependencies]
dep = { path = \"../dep\" }
";
        let main_src = "module Main exposing (x)\nx : Int\nlet x = 1\n";
        let dep_src = "module Dep exposing (y)\ny : Int\nlet y = 2\n";

        let bad_dep = "\
schema-version = 1
[project]
kind = \"lib\"
[package]
name = \"dep\"
version = \"0.1.0\"
[package.abi]
\"flick.javascript-ffi\" = \"^9.0.0\"
";
        let (diags, _) = run(&[
            ("/app/flick.toml", app_toml),
            ("/app/src/Main.fl", main_src),
            ("/dep/flick.toml", bad_dep),
            ("/dep/src/Dep.fl", dep_src),
        ]);
        assert!(
            diags.iter().any(|d| d.code == codes::F014),
            "expected FLICK-F014 for dependency mismatch, got {diags:?}"
        );

        let inactive_dep = "\
schema-version = 1
[project]
kind = \"lib\"
[package]
name = \"dep\"
version = \"0.1.0\"
when = \"go\"
[package.abi]
\"test.other-ffi\" = \"^9.0.0\"
";
        let (ok_diags, _) = run(&[
            ("/app/flick.toml", app_toml),
            ("/app/src/Main.fl", main_src),
            ("/dep/flick.toml", inactive_dep),
            ("/dep/src/Dep.fl", dep_src),
        ]);
        assert!(ok_diags.is_empty(), "{ok_diags:?}");
    }
}
