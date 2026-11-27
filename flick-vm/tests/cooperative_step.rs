//! Cooperative single-threaded stepping (`start_eval`, `step`, `abandon`),
//! byte-buffer image round-trips, and pluggable [`HostCallbacks`] tests (#997).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use flick_core::ir::memory::{
    Block, CodeId, Exit, Function, Global, HostPrimitive, Instruction, Intrinsic, Name, Operation,
    Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_vm::image::build_static_image;
use flick_vm::sandbox::{Capability, LimitKind, ResourceLimits, SandboxConfig};
use flick_vm::session::{EvalOutcome, StepOutcome, VmSession};
use flick_vm::stdlib::{HostCallbacks, HostFetchRequest, HostFetchResponse};
use flick_vm::tier0::{Error, HostMode};

fn fn_def(params: Vec<u32>, instructions: Vec<Instruction>, ret: u32) -> Function {
    Function {
        params,
        body: Block {
            instructions,
            exit: Exit::Return(ret),
        },
    }
}

fn let_op(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn make_print_finalizer(msg: &str) -> Function {
    fn_def(
        vec![0, 1],
        vec![
            Instruction::Drop(0),
            Instruction::Drop(1),
            let_op(2, Operation::Literal(Lit::String(msg.into()))),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![2],
                },
            ),
        ],
        3,
    )
}

fn make_scope_body(fin_1: CodeId, fin_2: CodeId, body_code: CodeId) -> Function {
    fn_def(
        vec![0, 1],
        vec![
            Instruction::Drop(0),
            Instruction::Drop(1),
            let_op(
                2,
                Operation::Task {
                    code: fin_1,
                    captures: vec![],
                },
            ),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::ScopeAddFinalizer,
                    args: vec![2],
                },
            ),
            Instruction::Drop(3),
            let_op(
                4,
                Operation::Task {
                    code: fin_2,
                    captures: vec![],
                },
            ),
            let_op(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::ScopeAddFinalizer,
                    args: vec![4],
                },
            ),
            Instruction::Drop(5),
            let_op(
                6,
                Operation::Call {
                    code: body_code,
                    args: vec![],
                },
            ),
        ],
        6,
    )
}

fn infinite_loop_in_scope(module: &str) -> (Program, Name) {
    const ENTRY: CodeId = 100;
    const SCOPE_BODY: CodeId = 101;
    const FIN_1: CodeId = 102;
    const FIN_2: CodeId = 103;

    let mut functions = BTreeMap::from([
        (
            0,
            fn_def(
                vec![],
                vec![let_op(
                    0,
                    Operation::Loop {
                        id: "inf".into(),
                        code: 1,
                        args: vec![],
                    },
                )],
                0,
            ),
        ),
        (
            1,
            fn_def(
                vec![],
                vec![let_op(
                    0,
                    Operation::LoopExit {
                        id: "inf".into(),
                        value: None,
                    },
                )],
                0,
            ),
        ),
    ]);
    functions.insert(FIN_1, make_print_finalizer("fin-1"));
    functions.insert(FIN_2, make_print_finalizer("fin-2"));
    functions.insert(SCOPE_BODY, make_scope_body(FIN_1, FIN_2, 0));
    functions.insert(
        ENTRY,
        fn_def(
            vec![],
            vec![
                let_op(
                    0,
                    Operation::Closure {
                        code: SCOPE_BODY,
                        captures: vec![],
                    },
                ),
                let_op(
                    1,
                    Operation::Intrinsic {
                        kind: Intrinsic::ScopeRun,
                        args: vec![0],
                    },
                ),
            ],
            1,
        ),
    );
    let entry = Name::new(module, "inf_loop");
    (
        Program {
            functions,
            globals: vec![Global {
                name: entry.clone(),
                initializer: ENTRY,
            }],
            op_spans: BTreeMap::new(),
        },
        entry,
    )
}

fn add_program(module: &str, name: &str, a: i32, b: i32) -> (Program, Name) {
    let entry = Name::new(module, name);
    let f = fn_def(
        vec![],
        vec![
            let_op(0, Operation::Literal(Lit::Int(a))),
            let_op(1, Operation::Literal(Lit::Int(b))),
            let_op(
                2,
                Operation::Binary {
                    op: BinOp::Arith(Arith::Add, Num::Int),
                    left: 0,
                    right: 1,
                },
            ),
        ],
        2,
    );
    (
        Program {
            functions: BTreeMap::from([(0, f)]),
            globals: vec![Global {
                name: entry.clone(),
                initializer: 0,
            }],
            op_spans: BTreeMap::new(),
        },
        entry,
    )
}

#[test]
fn cooperative_infinite_loop_yields_abandons_with_lifo_and_session_survives() {
    let mut session = VmSession::with_sandbox(HostMode::Deterministic, SandboxConfig::untrusted());
    let baseline_bytes = session.heap_live_bytes();

    let (loop_prog, loop_entry) = infinite_loop_in_scope("Coop.Inf");
    session
        .start_eval(&loop_prog, &loop_entry, None, false)
        .expect("start infinite loop");
    assert!(session.has_active_eval());

    // Multiple bounded slices yield cleanly back to the host.
    for _ in 0..4 {
        let out = session.step(120);
        assert!(
            matches!(out, StepOutcome::Yielded { fuel_used } if fuel_used > 0),
            "expected Yielded from infinite loop, got {out:?}"
        );
        assert!(session.has_active_eval());
    }

    // Abandon unwinds open scopes in LIFO order and restores session state.
    let abandoned = session.abandon();
    assert_eq!(
        abandoned,
        EvalOutcome::Interrupted {
            stdout: "fin-2\nfin-1\n".to_owned(),
        }
    );
    assert!(!session.has_active_eval());
    assert_eq!(session.heap_live_bytes(), baseline_bytes);

    // Subsequent evaluation on the same session succeeds cleanly.
    let (ok_prog, ok_entry) = add_program("Coop.Next", "it", 40, 2);
    session
        .start_eval(&ok_prog, &ok_entry, None, false)
        .expect("start follow-up eval");
    let done = session.step(1_000);
    assert_eq!(
        done,
        StepOutcome::Completed(EvalOutcome::Value {
            rendered: "42".to_owned(),
            stdout: String::new(),
        })
    );
    session.shutdown().expect("zero leaked heap slots");
}

#[test]
fn cooperative_step_enforces_total_sandbox_fuel_limit() {
    let mut session = VmSession::with_sandbox(
        HostMode::Deterministic,
        SandboxConfig::untrusted().with_limits(ResourceLimits::unlimited().with_fuel(350)),
    );
    let (loop_prog, loop_entry) = infinite_loop_in_scope("Coop.Lim");
    session
        .start_eval(&loop_prog, &loop_entry, None, false)
        .expect("start");

    assert!(matches!(session.step(100), StepOutcome::Yielded { .. }));
    assert!(matches!(session.step(100), StepOutcome::Yielded { .. }));
    let final_step = session.step(500);
    assert!(
        matches!(
            &final_step,
            StepOutcome::LimitExceeded {
                kind: LimitKind::Fuel,
                stdout,
                ..
            } if stdout == "fin-2\nfin-1\n"
        ),
        "expected Fuel LimitExceeded with LIFO finalizers, got {final_step:?}"
    );
    assert!(!session.has_active_eval());
    session.shutdown().expect("zero leaks");
}

#[derive(Debug, Default)]
struct RecordingHost {
    stdout: Arc<Mutex<String>>,
    millis: f64,
    random: f64,
    fetch_calls: Arc<Mutex<Vec<String>>>,
}

impl HostCallbacks for RecordingHost {
    fn write_stdout(&mut self, text: &str) {
        self.stdout.lock().expect("lock").push_str(text);
    }

    fn current_time_millis(&mut self) -> Option<f64> {
        Some(self.millis)
    }

    fn random_next(&mut self) -> Option<f64> {
        Some(self.random)
    }

    fn fetch(&mut self, req: &HostFetchRequest) -> Result<HostFetchResponse, String> {
        self.fetch_calls
            .lock()
            .expect("lock")
            .push(format!("{} {}", req.method, req.url));
        Ok(HostFetchResponse {
            status: 200,
            headers: vec![("content-type".into(), "text/plain".into())],
            body: b"ok-from-js-host".to_vec(),
        })
    }
}

fn host_cb_program(module: &str, name: &str) -> (Program, Name) {
    let entry = Name::new(module, name);
    let prog = Program {
        functions: BTreeMap::from([(
            0,
            fn_def(
                vec![],
                vec![
                    let_op(0, Operation::Literal(Lit::String("hi-cb".into()))),
                    let_op(
                        1,
                        Operation::Intrinsic {
                            kind: Intrinsic::PrintLine,
                            args: vec![0],
                        },
                    ),
                    Instruction::Drop(1),
                    let_op(2, Operation::Literal(Lit::Unit)),
                    let_op(
                        3,
                        Operation::Intrinsic {
                            kind: Intrinsic::Host(HostPrimitive::CurrentTimeMillis),
                            args: vec![2],
                        },
                    ),
                    let_op(4, Operation::Literal(Lit::Unit)),
                    let_op(
                        5,
                        Operation::Intrinsic {
                            kind: Intrinsic::Host(HostPrimitive::RandomNext),
                            args: vec![4],
                        },
                    ),
                    let_op(
                        6,
                        Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Float),
                            left: 3,
                            right: 5,
                        },
                    ),
                ],
                6,
            ),
        )]),
        globals: vec![Global {
            name: entry.clone(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    (prog, entry)
}

#[test]
fn host_callbacks_and_byte_buffer_image_round_trip() {
    let stdout_log = Arc::new(Mutex::new(String::new()));
    let fetch_log = Arc::new(Mutex::new(Vec::new()));
    let host = RecordingHost {
        stdout: Arc::clone(&stdout_log),
        millis: 1_700_000_001_234.0,
        random: 0.75,
        fetch_calls: Arc::clone(&fetch_log),
    };

    let (prog, entry) = host_cb_program("HostCb", "run");
    let image_bytes = build_static_image(prog, vec![], Some(entry.clone()), None)
        .expect("build static image")
        .encode()
        .expect("encode image bytes");

    let mut session = VmSession::with_sandbox(
        HostMode::LiveOs,
        SandboxConfig::untrusted()
            .allow(Capability::Clock)
            .allow(Capability::Random),
    );
    session.set_host_callbacks(Some(Box::new(host)));
    let loaded = session
        .load_image_bytes(&image_bytes)
        .expect("load image bytes");
    assert_eq!(loaded.loaded_modules, vec!["HostCb".to_owned()]);

    session
        .start_eval_loaded(&entry, false)
        .expect("start loaded entry");
    let step = session.step(10_000);
    assert_eq!(
        step,
        StepOutcome::Completed(EvalOutcome::Value {
            rendered: "1700000001234.75".to_owned(),
            stdout: "hi-cb\n".to_owned(),
        })
    );
    assert_eq!(*stdout_log.lock().expect("lock"), "hi-cb\n");

    // Fetch denied without Capability::Net, allowed once granted.
    let req = HostFetchRequest {
        url: "https://example.invalid/data".into(),
        method: "GET".into(),
        headers: vec![],
        body: None,
    };
    assert!(matches!(
        session.host_fetch(&req),
        Err(Error::CapabilityDenied {
            capability: Capability::Net,
            ..
        })
    ));
    session.set_capabilities(session.capabilities().allow(Capability::Net));
    let resp = session.host_fetch(&req).expect("fetch allowed");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"ok-from-js-host");
    assert_eq!(
        *fetch_log.lock().expect("lock"),
        vec!["GET https://example.invalid/data".to_owned()]
    );

    // Save warm session image to byte buffer and load into a fresh session.
    let warm_bytes = session
        .save_image_bytes(Some(entry.clone()))
        .expect("save warm image bytes");
    session.shutdown().expect("zero leaks session 1");

    let mut session2 = VmSession::with_sandbox(
        HostMode::Deterministic,
        SandboxConfig::untrusted()
            .allow(Capability::Clock)
            .allow(Capability::Random),
    );
    let loaded2 = session2
        .load_image_bytes(&warm_bytes)
        .expect("load warm image bytes");
    assert_eq!(loaded2.restored_values, 1);
    session2
        .start_eval_loaded(&entry, false)
        .expect("eval restored global");
    assert_eq!(
        session2.step(10_000),
        StepOutcome::Completed(EvalOutcome::Value {
            rendered: "1700000001234.75".to_owned(),
            stdout: String::new(),
        })
    );
    session2.shutdown().expect("zero leaks session 2");
}
