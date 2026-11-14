//! Direct component calls obey input budgets and cancellation during preparation.

use super::{common, fixture};
use flick_backend::host::runtime::Cancellation;
use flick_backend::host::{Budget, WasmBackend};
use flick_backend::{
    Backend, BuildRequest, Diagnostic, Failure, Input, InputError, InputKind, Inputs, Limits,
    OptionValue, Policy, generate, sha256,
};
use flick_ir::Program;
use flick_test_backend::Literal;

#[derive(Default)]
struct RecordingInputs(usize);

impl Inputs for RecordingInputs {
    fn read(&mut self, _: &str) -> Result<Vec<u8>, InputError> {
        self.0 += 1;
        Ok(common::TEXT.as_bytes().to_vec())
    }
}

fn refused(
    backend: &WasmBackend,
    request: &BuildRequest,
    inputs: &mut dyn Inputs,
    generation: bool,
) -> Vec<Diagnostic> {
    if generation {
        backend.generate(request, inputs).expect_err("refused call")
    } else {
        backend
            .check_target(request, inputs)
            .expect_err("refused call")
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn direct_call_metadata_is_bounded_before_inputs_are_read() {
    for generation in [false, true] {
        let (backend, mut request) = fixture(Budget {
            inputs: 4096,
            ..Budget::default()
        });
        let mut input = RecordingInputs::default();
        // The ordinary envelope fits and is callable with this host budget.
        assert!(backend.check_target(&request, &mut input).is_ok());
        request.target.options.push(OptionValue {
            key: "padding".into(),
            value: "x".repeat(8192),
        });
        let mut input = RecordingInputs::default();
        let diagnostics = refused(&backend, &request, &mut input, generation);
        assert_eq!(diagnostics[0].code, "P013");
        assert!(diagnostics[0].message.contains("input"));
        assert_eq!(input.0, 0);
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn cancelled_calls_do_not_prepare_inputs() {
    for generation in [false, true] {
        let (backend, request) = fixture(Budget::default());
        backend.cancellation().cancel();
        let mut input = RecordingInputs::default();
        let diagnostics = refused(&backend, &request, &mut input, generation);
        assert_eq!(diagnostics[0].code, "P013");
        assert!(diagnostics[0].message.contains("cancelled"));
        assert_eq!(input.0, 0);
    }
}

struct CancelDuringRead {
    cancellation: Cancellation,
    reads: usize,
    unavailable: bool,
}

impl Inputs for CancelDuringRead {
    fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError> {
        self.reads += 1;
        self.cancellation.cancel();
        if self.unavailable {
            Err(InputError::Unavailable)
        } else {
            Ok(if id == "source/App" {
                common::TEXT.as_bytes().to_vec()
            } else {
                vec![]
            })
        }
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn cancellation_stops_preparation_and_takes_priority_over_input_errors() {
    for generation in [false, true] {
        for unavailable in [false, true] {
            let (backend, mut request) = fixture(Budget::default());
            request.inputs.push(Input {
                id: "unused".into(),
                kind: InputKind::Resource,
                sha256: sha256(b""),
                size: 0,
            });
            let mut inputs = CancelDuringRead {
                cancellation: backend.cancellation(),
                reads: 0,
                unavailable,
            };
            let diagnostics = refused(&backend, &request, &mut inputs, generation);
            assert_eq!(diagnostics[0].code, "P013");
            assert!(
                diagnostics[0].message.contains("cancelled"),
                "{diagnostics:?}"
            );
            assert_eq!(inputs.reads, 1);
        }
    }
}

fn empty_read_request(mut request: BuildRequest) -> BuildRequest {
    let mut program = Program::from_cbor(&request.program).expect("program");
    program.modules.clear();
    program.files.clear();
    request.program = program.to_cbor();
    request.modules.clear();
    request.sources.clear();
    request.entries.clear();
    request.inputs.clear();
    request.target.options.push(OptionValue {
        key: "fixture.case".into(),
        value: "reads".into(),
    });
    request
}

#[test]
fn native_read_case_without_sources_returns_a_diagnostic() {
    let request = empty_read_request(common::request());
    let mut inputs = RecordingInputs::default();
    let diagnostics = Literal
        .generate(&request, &mut inputs)
        .expect_err("missing read input");
    assert_eq!(diagnostics[0].code, "P012");
    assert_eq!(
        diagnostics[0].message,
        "literal read case requires a source input"
    );
    assert_eq!(inputs.0, 0);
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn component_read_case_without_sources_returns_a_diagnostic() {
    let (backend, request) = fixture(Budget::default());
    let request = empty_read_request(request);
    for generation in [false, true] {
        let mut inputs = RecordingInputs::default();
        let diagnostics = refused(&backend, &request, &mut inputs, generation);
        assert_eq!(diagnostics[0].code, "P012");
        assert_eq!(
            diagnostics[0].message,
            "literal read case requires a source input"
        );
        assert_eq!(inputs.0, 0);
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn driver_cancellation_stops_input_snapshots_and_preserves_the_diagnostic() {
    for already_cancelled in [true, false] {
        for unavailable in [false, true] {
            let (backend, mut request) = fixture(Budget::default());
            request.inputs.push(Input {
                id: "unused".into(),
                kind: InputKind::Resource,
                sha256: sha256(b""),
                size: 0,
            });
            let token = backend.cancellation();
            if already_cancelled {
                token.cancel();
            }
            let policy = Policy {
                toolchains: &request.toolchains,
                environment: &[],
                reserved_paths: &[],
                authorize_step: &common::authorize,
                limits: Limits::default(),
            };
            let mut reads = 0;
            let result = generate(
                &backend,
                &request,
                &policy,
                |id| {
                    reads += 1;
                    token.cancel();
                    (!unavailable).then(|| {
                        if id == "source/App" {
                            common::TEXT.as_bytes().to_vec()
                        } else {
                            vec![]
                        }
                    })
                },
                true,
            );
            let Failure::Rejected(diagnostics) = result.err().expect("cancelled build") else {
                panic!("input availability must not mask cancellation")
            };
            assert_eq!(diagnostics[0].code, "P013");
            assert!(
                diagnostics[0].message.contains("cancelled"),
                "{diagnostics:?}"
            );
            assert_eq!(reads, usize::from(!already_cancelled));
        }
    }
}
