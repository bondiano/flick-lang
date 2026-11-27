//! Sandbox capability and bytecode/image verifier tests (#992).

use std::collections::BTreeMap;
use std::slice;

use flick_core::ir::memory::{
    Block, Exit, ForeignCallSpec, Function, Global, HandlerEffect, HandlerOperation, HostPrimitive,
    HostServiceIntrinsic, Instruction, Intrinsic, MemorySpan, Name, Operation, Program, WireHint,
};
use flick_core::ir::{Adapter, ForeignMode, Lit};
use flick_core::types::OpClass;
use flick_vm::image::{ImageError, VmImage, build_static_image};
use flick_vm::sandbox::bytecode_fuzz::malformed_bytecode_corpus;
use flick_vm::sandbox::{
    Capability, CapabilitySet, NATIVE_REGION_ARENA_LIMIT, SandboxConfig, SandboxError,
    check_program_capabilities, verify_bytecode, verify_cbor_budget, verify_image,
    verify_image_bytes,
};
use flick_vm::session::{EvalOutcome, VmSession};
use flick_vm::stdlib::{HostContext, eval_host_primitive, eval_host_service, eval_intrinsic};
use flick_vm::tier0::{Error, Heap, HostMode, ProgramOutcome, Value};
use flick_vm::tier1;

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

fn int_phrase(module: &str, name: &str, n: i32) -> (Program, Name) {
    let entry = Name::new(module, name);
    let f = fn_def(vec![], vec![let_op(0, Operation::Literal(Lit::Int(n)))], 0);
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

fn net_listener_program(module: &str, span: MemorySpan) -> (Program, Name) {
    let functions = BTreeMap::from([
        (
            0,
            fn_def(
                vec![],
                vec![
                    let_op(
                        0,
                        Operation::Closure {
                            code: 1,
                            captures: vec![],
                        },
                    ),
                    let_op(
                        1,
                        Operation::Closure {
                            code: 2,
                            captures: vec![],
                        },
                    ),
                    let_op(
                        2,
                        Operation::Handle {
                            body: 0,
                            effects: vec![HandlerEffect {
                                effect: "Net".into(),
                                operations: vec![HandlerOperation {
                                    name: "listen".into(),
                                    class: OpClass::Tail,
                                    dicts: 0,
                                    arity: 1,
                                    function: 1,
                                }],
                            }],
                            ret: None,
                            errors: None,
                        },
                    ),
                ],
                2,
            ),
        ),
        (
            1,
            fn_def(
                vec![0, 1],
                vec![
                    Instruction::Drop(0),
                    Instruction::Drop(1),
                    let_op(2, Operation::Literal(Lit::Int(8080))),
                    let_op(
                        3,
                        Operation::Perform {
                            effect: "Net".into(),
                            op: "listen".into(),
                            dicts: vec![],
                            args: vec![2],
                        },
                    ),
                ],
                3,
            ),
        ),
        (2, fn_def(vec![0, 1], vec![Instruction::Drop(0)], 1)),
    ]);
    let entry = Name::new(module, "open_listener");
    (
        Program {
            functions,
            globals: vec![Global {
                name: entry.clone(),
                initializer: 0,
            }],
            op_spans: BTreeMap::from([((1, 3), span)]),
        },
        entry,
    )
}

#[test]
fn ac1_net_listener_denied_without_net_and_allowed_with_net() {
    let expected_span = MemorySpan {
        file: 7,
        start: 42,
        end: 64,
    };
    let (net_prog, entry) = net_listener_program("App.Server", expected_span);

    let mut denied_session =
        VmSession::with_sandbox(HostMode::Deterministic, SandboxConfig::untrusted());
    assert_eq!(
        denied_session.eval_phrase(&net_prog, &entry, None, false, None),
        EvalOutcome::CapabilityDenied {
            capability: Capability::Net,
            binding: "Net.listen".into(),
            span: Some(expected_span),
            stdout: String::new(),
        }
    );
    let (ok_prog, ok_entry) = int_phrase("App.Pure", "it", 99);
    assert_eq!(
        denied_session.eval_phrase(&ok_prog, &ok_entry, None, false, None),
        EvalOutcome::Value {
            rendered: "99".into(),
            stdout: String::new(),
        }
    );

    let net_image =
        build_static_image(net_prog.clone(), vec![], Some(entry.clone()), None).expect("image");
    assert_eq!(
        denied_session.load_image(&net_image).expect_err("refused"),
        ImageError::Sandbox(SandboxError::CapabilityDenied {
            capability: Capability::Net,
            binding: "Net.listen".into(),
            span: Some(expected_span),
        })
    );
    denied_session.shutdown().expect("zero leaks");

    let mut net_session = VmSession::with_sandbox(
        HostMode::Deterministic,
        SandboxConfig::untrusted().allow(Capability::Net),
    );
    let report = net_session.load_image(&net_image).expect("loads with net");
    assert_eq!(report.loaded_modules, vec!["App.Server".to_owned()]);
    assert_eq!(
        net_session.eval_phrase(&net_prog, &entry, None, false, None),
        EvalOutcome::Value {
            rendered: "8080".into(),
            stdout: String::new(),
        }
    );
    net_session.shutdown().expect("zero leaks");
}

fn foreign_spec(module: &str, from: &str, name: &str) -> Box<ForeignCallSpec> {
    Box::new(ForeignCallSpec {
        module: module.into(),
        source_dir: None,
        from: Some(from.into()),
        name: name.into(),
        mode: ForeignMode::Pure,
        dicts: 0,
        arity: 0,
        taken: 0,
        runtime: false,
        has_catch: false,
        adapter: Adapter::default(),
        arg_hints: vec![],
        result_hint: WireHint::default(),
        remaining_stages: vec![],
    })
}

#[test]
fn static_capabilities_cover_all_eight_capabilities() {
    let span = MemorySpan {
        file: 1,
        start: 10,
        end: 25,
    };
    let make_single = |op: Operation| {
        let entry = Name::new("CapTest", "run");
        (
            Program {
                functions: BTreeMap::from([(0, fn_def(vec![], vec![let_op(0, op)], 0))]),
                globals: vec![Global {
                    name: entry.clone(),
                    initializer: 0,
                }],
                op_spans: BTreeMap::from([((0, 0), span)]),
            },
            entry,
        )
    };

    let cases: Vec<(Capability, Operation)> = vec![
        (
            Capability::Fs,
            Operation::Intrinsic {
                kind: Intrinsic::HostService(HostServiceIntrinsic::FsExists),
                args: vec![],
            },
        ),
        (
            Capability::Net,
            Operation::ForeignCall {
                spec: foreign_spec("Net", "node:net", "createServer"),
                args: vec![],
            },
        ),
        (
            Capability::Process,
            Operation::Intrinsic {
                kind: Intrinsic::Host(HostPrimitive::ProcessSpawn),
                args: vec![],
            },
        ),
        (
            Capability::Env,
            Operation::Intrinsic {
                kind: Intrinsic::Host(HostPrimitive::Vars),
                args: vec![],
            },
        ),
        (
            Capability::Clock,
            Operation::Intrinsic {
                kind: Intrinsic::Host(HostPrimitive::CurrentTimeMillis),
                args: vec![],
            },
        ),
        (
            Capability::Random,
            Operation::Intrinsic {
                kind: Intrinsic::Host(HostPrimitive::RandomNext),
                args: vec![],
            },
        ),
        (
            Capability::Ffi,
            Operation::ForeignCall {
                spec: foreign_spec("Custom", "./helper.mjs", "doWork"),
                args: vec![],
            },
        ),
        (
            Capability::Plugins,
            Operation::Perform {
                effect: "Plugins".into(),
                op: "invoke".into(),
                dicts: vec![],
                args: vec![],
            },
        ),
    ];

    for (expected_cap, op) in cases {
        let (prog, entry) = make_single(op);
        let err = check_program_capabilities(
            &prog,
            Some(slice::from_ref(&entry)),
            CapabilitySet::default_untrusted(),
        )
        .expect_err("denied under pure");
        assert!(matches!(
            err,
            SandboxError::CapabilityDenied { capability, span: got, .. }
                if capability == expected_cap && got == Some(span)
        ));
        check_program_capabilities(
            &prog,
            Some(slice::from_ref(&entry)),
            CapabilitySet::pure().allow(expected_cap),
        )
        .expect("allowed when granted");
    }
}

#[test]
fn dynamic_capabilities_always_allow_console_and_deny_ungranted_host_ops() {
    let mut heap = Heap::new();
    let mut fuel = 1_000;
    let mut stdout = String::new();
    let mut stderr = String::new();
    let msg = heap.literal(&Lit::String("hello".into())).expect("str");
    {
        let mut ctx = HostContext::new(&mut fuel, HostMode::Deterministic, &mut stdout)
            .with_stderr(&mut stderr)
            .with_capabilities(CapabilitySet::pure());

        let u1 = eval_intrinsic(&mut heap, Intrinsic::PrintLine, &msg, &mut ctx).expect("print");
        let u2 = eval_host_primitive(&mut heap, HostPrimitive::StdoutWrite, &msg, &mut ctx)
            .expect("stdout");
        let u3 = eval_host_primitive(&mut heap, HostPrimitive::StderrLine, &msg, &mut ctx)
            .expect("stderr");
        assert_eq!((u1, u2, u3), (Value::Unit, Value::Unit, Value::Unit));

        assert!(matches!(
            eval_host_primitive(
                &mut heap,
                HostPrimitive::CurrentTimeMillis,
                &Value::Unit,
                &mut ctx,
            ),
            Err(Error::CapabilityDenied {
                capability: Capability::Clock,
                ..
            })
        ));
        assert!(matches!(
            eval_host_service(
                &mut heap,
                HostServiceIntrinsic::FsExists,
                slice::from_ref(&msg),
                &mut ctx,
            ),
            Err(Error::CapabilityDenied {
                capability: Capability::Fs,
                ..
            })
        ));
    }
    assert_eq!(stdout, "hello\nhello");
    assert_eq!(stderr, "hello\n");
    heap.drop_value(msg).expect("drop msg");
    heap.finish().expect("zero leaks");
}

#[test]
fn bytecode_and_image_verifiers_enforce_security_boundary_and_constants() {
    assert_eq!(NATIVE_REGION_ARENA_LIMIT, 64 * 1024 * 1024);

    let (prog, entry) = int_phrase("Verify.Ok", "main", 123);
    let prepared = tier1::prepare(&prog, &entry).expect("prepare");
    verify_bytecode(&prepared).expect("valid bytecode verifies");

    let mut heap = Heap::new();
    let report = tier1::execute_prepared_outcome(
        &mut heap,
        &prepared,
        vec![],
        1_000,
        HostMode::Deterministic,
    );
    assert!(matches!(
        report.outcome,
        ProgramOutcome::Returned(Value::Int(123))
    ));
    heap.finish().expect("zero leaks");

    let corpus = malformed_bytecode_corpus();
    assert!(corpus.len() >= 20);
    for (name, bad) in corpus {
        assert!(
            verify_bytecode(&bad).is_err(),
            "`{name}` must fail verify_bytecode"
        );
        let mut h = Heap::new();
        let rep =
            tier1::execute_prepared_outcome(&mut h, &bad, vec![], 100, HostMode::Deterministic);
        assert!(
            matches!(
                rep.outcome,
                ProgramOutcome::Fault(Error::VerificationFailed(_))
            ),
            "`{name}` must fail execute_prepared_outcome: {:?}",
            rep.outcome
        );
        h.finish().expect("zero leaks");
    }

    let image = build_static_image(prog, vec![], Some(entry), None).expect("build image");
    verify_image(&image).expect("valid image verifies");
    let encoded = image.encode().expect("encode");
    let decoded = verify_image_bytes(&encoded).expect("decode & verify");
    assert_eq!(decoded.modules(), &["Verify.Ok".to_owned()]);

    let mut bomb = vec![0x81_u8; 256];
    bomb.push(0x00);
    assert!(verify_cbor_budget(&bomb, "bomb").is_err());
    assert!(VmImage::decode(b"").is_err());
    assert!(VmImage::decode(b"NOPE").is_err());
}
