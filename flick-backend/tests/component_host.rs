#![cfg(feature = "host")]

mod common;
#[path = "component_host/preparation.rs"]
mod preparation;

use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use std::{env, fs};
use wasmtime::Engine;

use flick_backend::host::{Budget, WasmBackend, runtime};
use flick_backend::{
    Backend, BackendPin, BuildRequest, Failure, Generated, InputError, Inputs, Limits, Mode,
    OptionValue, Policy, generate, sha256,
};
use flick_ir::{ExprKind, ItemKind, Program, Stage};
use flick_test_backend::Literal;

fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| runtime::engine(None).expect("engine"))
}

fn component() -> (BackendPin, Vec<u8>) {
    let path = env::var_os("FLICK_TEST_BACKEND_COMPONENT").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../target/wasm32-wasip2/release/flick_test_backend.wasm")
        },
        PathBuf::from,
    );
    let bytes = fs::read(&path).expect("build the component fixture with `just wasm-backends`");
    let pin = BackendPin {
        identity: Literal.describe().identity,
        sha256: sha256(&bytes),
    };
    (pin, bytes)
}

fn fixture(budget: Budget) -> (WasmBackend, BuildRequest) {
    let (pin, bytes) = component();
    let backend = WasmBackend::load(engine(), pin.clone(), &bytes, budget).expect("component");
    let mut request = common::request();
    request.backend = pin;
    (backend, request)
}

fn compile(backend: &dyn Backend, request: &BuildRequest) -> Result<Generated, Failure> {
    let policy = Policy {
        toolchains: &request.toolchains,
        environment: &[],
        reserved_paths: &[],
        authorize_step: &common::authorize,
        limits: Limits::default(),
    };
    generate(
        backend,
        request,
        &policy,
        |_| Some(common::TEXT.as_bytes().to_vec()),
        true,
    )
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn mismatched_request_pin_is_rejected_by_the_driver() {
    let (backend, mut request) = fixture(Budget::default());
    request.backend.sha256 = sha256(b"another component");
    let policy = Policy {
        toolchains: &request.toolchains,
        environment: &[],
        reserved_paths: &[],
        authorize_step: &common::authorize,
        limits: Limits::default(),
    };
    for available in [true, false] {
        let mut reads = 0;
        let error = generate(
            &backend,
            &request,
            &policy,
            |_| {
                reads += 1;
                available.then(|| common::TEXT.as_bytes().to_vec())
            },
            true,
        )
        .err()
        .expect("mismatched pin");
        let Failure::Rejected(diagnostics) = error else {
            panic!("expected the loaded component to reject a different request pin")
        };
        assert_eq!(diagnostics[0].code, "P013");
        assert!(diagnostics[0].message.contains("request backend pin"));
        assert_eq!(reads, 0);
    }
}

#[derive(Default)]
struct UnreadableInputs(usize);

impl Inputs for UnreadableInputs {
    fn read(&mut self, _: &str) -> Result<Vec<u8>, InputError> {
        self.0 += 1;
        Err(InputError::Unavailable)
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn mismatched_request_pin_is_rejected_before_direct_input_preparation() {
    let (backend, mut request) = fixture(Budget::default());
    request.backend.sha256 = sha256(b"another component");
    for generation in [false, true] {
        let mut inputs = UnreadableInputs::default();
        let diagnostics = if generation {
            backend
                .generate(&request, &mut inputs)
                .expect_err("mismatched pin")
        } else {
            backend
                .check_target(&request, &mut inputs)
                .expect_err("mismatched pin")
        };
        assert_eq!(diagnostics[0].code, "P013");
        assert!(diagnostics[0].message.contains("request backend pin"));
        assert_eq!(inputs.0, 0);
    }
}

#[test]
fn the_literal_fixture_refuses_runtime_stages_instead_of_emitting_a_pure_function() {
    let mut request = common::request();
    request.backend.identity = Literal.describe().identity;
    runtime_main(&mut request);
    let error = compile(&Literal, &request)
        .err()
        .expect("a runtime stage is unsupported");
    let Failure::Rejected(diagnostics) = error else {
        panic!("expected unsupported diagnostic")
    };
    assert_eq!(diagnostics[0].code, "P012");
}

fn runtime_main(request: &mut BuildRequest) {
    let mut program = Program::from_cbor(&request.program).expect("IR");
    let ItemKind::Value { body, .. } = &mut program.modules[0].items[0].kind else {
        panic!("main")
    };
    let ExprKind::Lambda { params, .. } = &mut body.kind else {
        panic!("function")
    };
    params[0].stage = Stage::Runtime;
    request.program = program.to_cbor();
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn unsupported_ir_is_refused_in_preflight_before_a_trapping_generator() {
    let (backend, mut request) = fixture(Budget::default());
    runtime_main(&mut request);
    request.target.options.push(OptionValue {
        key: "fixture.case".into(),
        value: "trap".into(),
    });
    let error = compile(&backend, &request).err().expect("unsupported IR");
    let Failure::Rejected(diagnostics) = error else {
        panic!("expected unsupported diagnostic")
    };
    assert_eq!(diagnostics[0].code, "P012");
    assert_eq!(
        diagnostics[0].message,
        "literal backend supports only a pure Unit main"
    );
    assert_eq!(diagnostics[0].span.expect("source span").file, 0);
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn native_and_component_agree_on_independently_expected_binary_and_unit_js() {
    let (backend, mut request) = fixture(Budget::default());
    for mode in [Mode::WholeProgram, Mode::PerModule] {
        request.target.mode = mode;
        request.module_root = (mode == Mode::PerModule).then(|| "App".into());
        let native = compile(&Literal, &request).expect("native");
        let wasm = compile(&backend, &request).expect("component");
        assert_eq!(wasm.result(), native.result());
        assert_eq!(
            wasm.result().artifacts[0].bytes,
            b"export const main = (_) => undefined;\n"
        );
        assert_eq!(wasm.result().artifacts[1].bytes, [0, 255, 128]);
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn each_call_has_fresh_guest_state_even_when_compiled_code_is_cached() {
    let (backend, mut request) = fixture(Budget::default());
    request.target.options.push(OptionValue {
        key: "fixture.case".into(),
        value: "state".into(),
    });
    let result = compile(&backend, &request).expect("fresh instance for each generation");
    assert_eq!(
        result
            .result()
            .artifacts
            .iter()
            .find(|a| a.id == "state")
            .expect("counter")
            .bytes,
        [0; 4]
    );
    assert!(result.cache_key().is_some());
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn component_failures_are_rejected_before_any_generation_is_returned() {
    let (backend, request) = fixture(Budget::default());
    for case in ["undeclared", "path", "duplicate", "span", "trap"] {
        let mut request = request.clone();
        request.target.options.push(OptionValue {
            key: "fixture.case".into(),
            value: case.into(),
        });
        assert!(compile(&backend, &request).is_err(), "{case}");
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn instruction_fuel_stops_a_component_loop() {
    let (backend, mut request) = fixture(Budget {
        fuel: 2_000_000,
        ..Budget::default()
    });
    request.target.options.push(OptionValue {
        key: "fixture.case".into(),
        value: "fuel".into(),
    });
    let error = compile(&backend, &request).err().expect("fuel exhaustion");
    let Failure::Rejected(diagnostics) = error else {
        panic!("expected a component diagnostic")
    };
    assert!(
        diagnostics[0].message.contains("ran out of its fuel"),
        "{diagnostics:?}"
    );
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn cancellation_stops_an_active_component_loop() {
    let (backend, mut request) = fixture(Budget::default());
    let cancellation = backend.cancellation();
    request.target.options.push(OptionValue {
        key: "fixture.case".into(),
        value: "fuel".into(),
    });
    let worker = thread::spawn(move || compile(&backend, &request));
    thread::sleep(Duration::from_millis(20));
    cancellation.cancel();
    let error = worker
        .join()
        .expect("worker")
        .err()
        .expect("cancelled generation");
    let Failure::Rejected(diagnostics) = error else {
        panic!("expected cancellation diagnostic")
    };
    assert!(
        diagnostics[0].message.contains("cancelled"),
        "{diagnostics:?}"
    );
    let (unrelated, request) = fixture(Budget::default());
    compile(&unrelated, &request).expect("another store on the same engine is unaffected");
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn descriptor_lifting_and_initial_memory_obey_host_budgets() {
    let (pin, bytes) = component();
    let output = WasmBackend::load(
        engine(),
        pin.clone(),
        &bytes,
        Budget {
            output: 0,
            ..Budget::default()
        },
    )
    .err()
    .expect("output limit");
    assert!(output.contains("exceeded its output limit"), "{output}");
    assert!(
        WasmBackend::load(
            engine(),
            pin,
            &bytes,
            Budget {
                memory: 0,
                ..Budget::default()
            }
        )
        .is_err()
    );
}

#[test]
fn an_incompatible_wit_import_is_refused_before_instantiation() {
    let bytes = wat::parse_str(r#"(component (import "flick:backend/inputs@0.2.0" (instance)))"#)
        .expect("component WAT");
    let mut pin = common::request().backend;
    pin.sha256 = sha256(&bytes);
    let error = WasmBackend::load(engine(), pin, &bytes, Budget::default())
        .err()
        .expect("incompatible WIT");
    assert_eq!(
        error,
        "incompatible backend import `flick:backend/inputs@0.2.0`"
    );
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn the_component_digest_id_and_version_must_match_the_project_pin() {
    let (pin, bytes) = component();
    for field in ["digest", "id", "version"] {
        let mut bad = pin.clone();
        match field {
            "digest" => bad.sha256 = sha256(b"another component"),
            "id" => bad.identity.id = "test/another".into(),
            _ => bad.identity.version = "2.0.0".into(),
        }
        let error = WasmBackend::load(engine(), bad, &bytes, Budget::default())
            .err()
            .expect("pin mismatch");
        assert!(error.contains("pin"), "{field}: {error}");
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn ignored_read_errors_cannot_bypass_input_call_or_transfer_limits() {
    for (budget, expected) in [
        (
            Budget {
                input_calls: 2,
                ..Budget::default()
            },
            "side-input call limit",
        ),
        (
            Budget {
                inputs: 4096,
                ..Budget::default()
            },
            "side-input transfer limit",
        ),
    ] {
        let (backend, mut request) = fixture(budget);
        request.target.options.push(OptionValue {
            key: "fixture.case".into(),
            value: "reads".into(),
        });
        let Failure::Rejected(diagnostics) = compile(&backend, &request).err().expect("read limit")
        else {
            panic!("expected a component diagnostic")
        };
        assert!(diagnostics[0].message.contains(expected), "{diagnostics:?}");
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn an_ungranted_wasi_clock_traps_instead_of_becoming_an_implicit_input() {
    let (backend, mut request) = fixture(Budget::default());
    request.target.options.push(OptionValue {
        key: "fixture.case".into(),
        value: "clock".into(),
    });
    let Failure::Rejected(diagnostics) = compile(&backend, &request).err().expect("no clock grant")
    else {
        panic!("expected a component diagnostic")
    };
    assert!(
        diagnostics[0].message.contains("trapped"),
        "{diagnostics:?}"
    );
}
