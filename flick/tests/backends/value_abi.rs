//! End-to-end tests for versioned backend value-ABI tables (#667):
//! - `test/literal` WASM component publishes `test.literal-ffi@0.1.0`, admitting
//!   direct bindings over table types and opaque handles while refusing `BigInt`,
//!   `List`, records, ADTs, and disallowed directions with the backend's reason
//!   at the binding span in both `flick check` and `flick build`.
//! - ABI version mismatches in `[package.abi]` or `foreign-binding.abi` produce
//!   `FLICK-F014` instead of a silent call.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::common::{flick_in, project};
use super::pinned;

fn parse_diagnostics(stderr: &[u8]) -> Vec<Value> {
    String::from_utf8(stderr.to_vec())
        .expect("UTF-8")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("diagnostic json"))
        .collect()
}

fn span_slice<'a>(source: &'a str, diag: &Value) -> &'a str {
    let start =
        usize::try_from(diag["range"]["start"].as_u64().expect("start")).expect("start fits");
    let end = usize::try_from(diag["range"]["end"].as_u64().expect("end")).expect("end fits");
    &source[start..end]
}

fn literal_project_with_extra_toml(name: &str, extra_toml: &str, source: &str) -> PathBuf {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm");
    let config = format!(
        "{}\n{extra_toml}\n[target.literal]\ncase = \"foreign\"\n",
        pinned().replace(&"a".repeat(64), &flick_backend::sha256(&bytes))
    );
    let root = project(name, &[("flick.toml", &config), ("src/App.fl", source)]);
    fs::write(root.join("backend.wasm"), &bytes).expect("write component");
    root
}

#[test]
#[ignore = "requires just wasm-backends"]
fn fixture_backend_value_abi_admits_valid_binding_and_refuses_disallowed_ones_at_span() {
    let valid_source = "\
module App exposing (Handle, ok_call, main)

type Handle
unboxed type UserId = UserId Int

ok_call : Handle -> UserId -> Float -> String -> (Int -> Unit) ->{} Bool
foreign ok_call from \"./native.c\" \"ok_call\"
  arity 5
  callback 4 data 0

main : Unit -> Unit
let main () = ()
";
    let root = literal_project_with_extra_toml(
        "backend_value_abi_valid",
        "[package]\nname = \"abi-ok\"\nversion = \"0.1.0\"\n[package.abi]\n\"test.literal-ffi\" = \"^0.1.0\"",
        valid_source,
    );
    let checked = flick_in(&root, &["check", "--offline"]);
    assert!(checked.status.success(), "check failed: {checked:?}");
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(built.status.success(), "build failed: {built:?}");

    let refused_cases = [
        (
            "bigint_refused",
            "\
module App exposing (bad, main)
// π preserves UTF-8 byte offsets

bad : BigInt -> Int
foreign pure bad from \"./native.c\" \"bad_bigint\"

main : Unit -> Unit
let main () = ()
",
            "foreign pure bad from \"./native.c\" \"bad_bigint\"",
            "no direct arbitrary-precision integer ABI",
        ),
        (
            "record_refused",
            "\
module App exposing (bad, main)
// π preserves UTF-8 byte offsets

bad : { x : Int } -> Int
foreign pure bad from \"./native.c\" \"bad_record\"

main : Unit -> Unit
let main () = ()
",
            "foreign pure bad from \"./native.c\" \"bad_record\"",
            "records have no stable ABI layout",
        ),
        (
            "adt_refused",
            "\
module App exposing (Choice, bad, main)
// π preserves UTF-8 byte offsets

type Choice = Left Int | Right Float

bad : Choice -> Int
foreign pure bad from \"./native.c\" \"bad_adt\"

main : Unit -> Unit
let main () = ()
",
            "foreign pure bad from \"./native.c\" \"bad_adt\"",
            "user-defined ADTs have no stable ABI layout",
        ),
        (
            "function_out_refused",
            "\
module App exposing (bad, main)
// π preserves UTF-8 byte offsets

bad : Int -> (Int -> Int)
foreign pure bad from \"./native.c\" \"bad_out_fn\"
  arity 1

main : Unit -> Unit
let main () = ()
",
            "foreign pure bad from \"./native.c\" \"bad_out_fn\"\n  arity 1",
            "allowed only in `in` direction, not `out`",
        ),
    ];

    for (label, source, expected_span, expected_reason) in refused_cases {
        fs::write(root.join("src/App.fl"), source).expect("write refused source");
        for command in ["check", "build"] {
            let out = flick_in(&root, &[command, "--offline", "--diagnostics", "json"]);
            assert_eq!(
                out.status.code(),
                Some(1),
                "{label} ({command}): expected failure, got {out:?}"
            );
            let diags = parse_diagnostics(&out.stderr);
            let diag = diags
                .iter()
                .find(|d| d["code"] == "FLICK-F013")
                .unwrap_or_else(|| panic!("{label} ({command}): expected FLICK-F013 in {diags:?}"));
            assert_eq!(diag["path"], "src/App.fl", "{label} ({command}): {diag}");
            assert_eq!(
                span_slice(source, diag),
                expected_span,
                "{label} ({command}): wrong span in {diag}"
            );
            let message = diag["message"].as_str().expect("message");
            assert!(
                message.contains(expected_reason),
                "{label} ({command}): expected `{expected_reason}` in `{message}`"
            );
        }
    }

    fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
#[ignore = "requires just wasm-backends"]
fn fixture_backend_reports_f014_on_package_abi_version_mismatch() {
    let source = "\
module App exposing (ok_call, main)

ok_call : Int -> Int
foreign pure ok_call from \"./native.c\" \"ok_call\"

main : Unit -> Unit
let main () = ()
";
    let root = literal_project_with_extra_toml(
        "backend_value_abi_mismatch",
        "[package]\nname = \"abi-bad\"\nversion = \"0.1.0\"\n[package.abi]\n\"test.literal-ffi\" = \"^2.0.0\"",
        source,
    );
    let toml_text = fs::read_to_string(root.join("flick.toml")).expect("read flick.toml");
    for command in ["check", "build"] {
        let out = flick_in(&root, &[command, "--offline", "--diagnostics", "json"]);
        assert_eq!(
            out.status.code(),
            Some(1),
            "mismatch ({command}): expected failure, got {out:?}"
        );
        let diags = parse_diagnostics(&out.stderr);
        let diag = diags
            .iter()
            .find(|d| d["code"] == "FLICK-F014")
            .unwrap_or_else(|| panic!("mismatch ({command}): expected FLICK-F014 in {diags:?}"));
        assert_eq!(diag["path"], "flick.toml", "{diag}");
        assert_eq!(span_slice(&toml_text, diag), "\"^2.0.0\"", "{diag}");
        let message = diag["message"].as_str().expect("message");
        assert!(
            message.contains("test.literal-ffi") && message.contains("0.1.0"),
            "unexpected FLICK-F014 message: {message}"
        );
    }
    fs::remove_dir_all(&root).expect("cleanup");
}

fn literal_abi_mismatch_request(
    pin: flick_backend::BackendPin,
    source: &str,
) -> flick_backend::BuildRequest {
    use std::collections::BTreeMap;

    use flick_backend::{
        Abi, BuildRequest, Environment, ForeignBinding, Input, InputKind, Mode, ModuleInput,
        ModuleRole, OptionValue, Profile, Source, TargetRequest, sha256,
    };
    use flick_ir::{Exports, Initialization, Module, Program};

    let module = Module {
        name: "App".into(),
        types: vec![],
        preambles: vec![],
        items: vec![],
        labels: BTreeMap::new(),
        initialization: Initialization::default(),
        records: BTreeMap::new(),
        package: None,
        exports: Exports::default(),
    };
    let program = Program::new("value abi test", vec![module], vec![], &|_| "App.fl".into());
    BuildRequest {
        backend: pin,
        target: TargetRequest {
            backend_api: flick_backend::VERSION.into(),
            ir_version: flick_ir::VERSION.into(),
            language_version: "0.2.0".into(),
            runtime_abis: vec![],
            environment: Environment {
                backend: "js".into(),
                platform: "node".into(),
            },
            tags: vec!["literal".into()],
            host_package: None,
            profile: Profile::Debug,
            mode: Mode::WholeProgram,
            required_capabilities: vec![],
            options: vec![OptionValue {
                key: "option.case".into(),
                value: "foreign".into(),
            }],
        },
        program: program.to_cbor(),
        module_root: None,
        modules: vec![ModuleInput {
            name: "App".into(),
            package: None,
            role: ModuleRole::Own,
            output: "App.out".into(),
            interface: None,
        }],
        sources: vec![Source {
            name: "App.fl".into(),
            input: "source/App".into(),
            line_starts: vec![0],
        }],
        entries: vec![],
        test_roots: vec![],
        root_handlers: vec![],
        foreign: vec![ForeignBinding {
            module: "App".into(),
            specifier: "./native.c".into(),
            resolved: "./native.c".into(),
            abi: Abi {
                name: "test.literal-ffi".into(),
                version: "9.9.9".into(),
            },
            inputs: vec![],
        }],
        inline_foreign: vec![],
        projections: vec![],
        package_targets: vec![],
        declared_dependencies: None,
        runtime_dependencies: vec![],
        toolchains: vec![],
        inputs: vec![Input {
            id: "source/App".into(),
            kind: InputKind::Source,
            sha256: sha256(source.as_bytes()),
            size: source.len() as u64,
        }],
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
fn fixture_backend_component_rejects_mismatched_foreign_binding_abi_with_f014() {
    use flick_backend::host::{Budget, WasmBackend, runtime};
    use flick_backend::{Backend, BackendId, BackendPin, DeclaredInputs, sha256};

    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    let bytes = fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm");
    let pin = BackendPin {
        identity: BackendId {
            id: "test/literal".into(),
            version: "1.0.0".into(),
        },
        sha256: sha256(&bytes),
    };
    let engine = runtime::engine(None).expect("wasmtime engine");
    let backend = WasmBackend::load(&engine, pin.clone(), &bytes, Budget::default())
        .expect("component loads");

    let source = "module App exposing ()\n";
    let request = literal_abi_mismatch_request(pin, source);
    let mut inputs =
        DeclaredInputs::new(&request.inputs, |_| Some(source.as_bytes().to_vec())).expect("inputs");

    let check_err = backend
        .check_target(&request, &mut inputs)
        .expect_err("WASM component check_target must reject mismatched foreign-binding.abi");
    assert!(
        check_err.iter().any(|d| d.code == "F014"
            && d.message.contains("test.literal-ffi@0.1.0")
            && d.message.contains("test.literal-ffi@9.9.9")),
        "expected F014 from component check_target, got {check_err:?}"
    );
    let gen_err = backend
        .generate(&request, &mut inputs)
        .expect_err("WASM component generate must reject mismatched foreign-binding.abi");
    assert!(
        gen_err.iter().any(|d| d.code == "F014"
            && d.message.contains("test.literal-ffi@0.1.0")
            && d.message.contains("test.literal-ffi@9.9.9")),
        "expected F014 from component generate, got {gen_err:?}"
    );
}
