//! Sandbox resource-limit, LIFO-finalizer unwind, and dynamic capability
//! revocation integration tests (#992).

use std::collections::BTreeMap;
use std::sync::Arc;

use flick_core::ir::memory::{
    Block, CodeId, Exit, Function, Global, HostPrimitive, Instruction, Intrinsic, Name, Operation,
    Program, ValueRep,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_vm::image::build_static_image;
use flick_vm::sandbox::{
    Capability, CapabilitySet, LimitKind, MockWallClock, ResourceLimits, SandboxConfig,
};
use flick_vm::session::{EvalOutcome, VmSession};
use flick_vm::tier0::HostMode;

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

/// Wraps `body_code` inside `Scope.run` registering two LIFO finalizers
/// printing `"fin-2"` then `"fin-1"` on unwind.
fn wrap_in_scope_with_lifo_finalizers(
    module: &str,
    name: &str,
    mut functions: BTreeMap<CodeId, Function>,
    body_code: CodeId,
) -> (Program, Name) {
    const ENTRY: CodeId = 100;
    const SCOPE_BODY: CodeId = 101;
    const FIN_1: CodeId = 102;
    const FIN_2: CodeId = 103;

    functions.insert(FIN_1, make_print_finalizer("fin-1"));
    functions.insert(FIN_2, make_print_finalizer("fin-2"));
    functions.insert(
        SCOPE_BODY,
        fn_def(
            vec![0, 1],
            vec![
                Instruction::Drop(0),
                Instruction::Drop(1),
                let_op(
                    2,
                    Operation::Task {
                        code: FIN_1,
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
                        code: FIN_2,
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
        ),
    );
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

    let entry = Name::new(module, name);
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

fn infinite_loop_in_scope(module: &str) -> (Program, Name) {
    let functions = BTreeMap::from([
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
    wrap_in_scope_with_lifo_finalizers(module, "inf_loop", functions, 0)
}

fn alloc_loop_in_scope(module: &str) -> (Program, Name) {
    let functions = BTreeMap::from([
        (
            0,
            fn_def(
                vec![],
                vec![
                    let_op(0, Operation::Literal(Lit::String("x".repeat(256)))),
                    let_op(1, Operation::VarMake(0)),
                    let_op(
                        2,
                        Operation::Loop {
                            id: "alloc".into(),
                            code: 1,
                            args: vec![1],
                        },
                    ),
                ],
                2,
            ),
        ),
        (
            1,
            fn_def(
                vec![0],
                vec![
                    Instruction::Dup(0),
                    let_op(1, Operation::VarGet(0)),
                    let_op(2, Operation::Literal(Lit::String("a".repeat(256)))),
                    let_op(
                        3,
                        Operation::Construct {
                            tag: "Cons".into(),
                            fields: vec![2, 1],
                            rep: ValueRep::Cell,
                            layout: None,
                        },
                    ),
                    let_op(4, Operation::VarSet { var: 0, value: 3 }),
                    Instruction::Drop(4),
                    let_op(
                        5,
                        Operation::LoopExit {
                            id: "alloc".into(),
                            value: None,
                        },
                    ),
                ],
                5,
            ),
        ),
    ]);
    wrap_in_scope_with_lifo_finalizers(module, "alloc_loop", functions, 0)
}

fn deep_recursion_in_scope(module: &str) -> (Program, Name) {
    let functions = BTreeMap::from([
        (
            0,
            fn_def(
                vec![],
                vec![
                    let_op(0, Operation::Literal(Lit::Int(0))),
                    let_op(
                        1,
                        Operation::Call {
                            code: 1,
                            args: vec![0],
                        },
                    ),
                ],
                1,
            ),
        ),
        (
            1,
            fn_def(
                vec![0],
                vec![
                    let_op(1, Operation::Literal(Lit::Int(1))),
                    let_op(
                        2,
                        Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 0,
                            right: 1,
                        },
                    ),
                    let_op(
                        3,
                        Operation::Call {
                            code: 1,
                            args: vec![2],
                        },
                    ),
                    let_op(4, Operation::Literal(Lit::Int(1))),
                    let_op(
                        5,
                        Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 3,
                            right: 4,
                        },
                    ),
                ],
                5,
            ),
        ),
    ]);
    wrap_in_scope_with_lifo_finalizers(module, "deep_rec", functions, 0)
}

fn assert_limit_with_lifo(
    session: &mut VmSession,
    prog: &Program,
    entry: &Name,
    expected: LimitKind,
) {
    let out = session.eval_phrase(prog, entry, None, false, None);
    assert!(
        matches!(
            &out,
            EvalOutcome::LimitExceeded { kind, stdout, .. }
                if *kind == expected && stdout == "fin-2\nfin-1\n"
        ),
        "expected {expected:?} LimitExceeded with LIFO finalizers, got {out:?}"
    );
}

#[test]
fn ac2_all_four_limits_run_lifo_finalizers_with_zero_leaks_and_session_recovers() {
    let mut session = VmSession::with_sandbox(HostMode::Deterministic, SandboxConfig::untrusted());
    let assert_ok = |session: &mut VmSession, tag: &str, n: i32| {
        let (p, e) = int_phrase(tag, "it", n);
        assert_eq!(
            session.eval_phrase(&p, &e, None, false, None),
            EvalOutcome::Value {
                rendered: n.to_string(),
                stdout: String::new(),
            }
        );
    };

    // 1. Infinite loop under `fuel`.
    session.set_limits(ResourceLimits::unlimited().with_fuel(500));
    let (fuel_prog, fuel_entry) = infinite_loop_in_scope("Lim.Fuel");
    assert_limit_with_lifo(&mut session, &fuel_prog, &fuel_entry, LimitKind::Fuel);
    assert_ok(&mut session, "Lim.Ok1", 101);

    // 2. Allocation loop under `heap_bytes`.
    let baseline_bytes = session.heap_live_bytes();
    session.set_limits(ResourceLimits::unlimited().with_heap_bytes(baseline_bytes + 4_096));
    let (heap_prog, heap_entry) = alloc_loop_in_scope("Lim.Heap");
    assert_limit_with_lifo(&mut session, &heap_prog, &heap_entry, LimitKind::HeapMemory);
    assert_eq!(session.heap_live_bytes(), baseline_bytes);
    assert_ok(&mut session, "Lim.Ok2", 202);

    // 3. Deep recursion under `max_stack_depth`.
    session.set_limits(ResourceLimits::unlimited().with_max_stack_depth(24));
    let (stack_prog, stack_entry) = deep_recursion_in_scope("Lim.Stack");
    assert_limit_with_lifo(
        &mut session,
        &stack_prog,
        &stack_entry,
        LimitKind::StackDepth,
    );
    assert_ok(&mut session, "Lim.Ok3", 303);

    // 4. Slow loop under `wall_time_ms` (native clock + pluggable MockWallClock for wasm32 parity).
    session.set_limits(ResourceLimits::unlimited().with_wall_time_ms(20));
    let (wall_prog, wall_entry) = infinite_loop_in_scope("Lim.Wall");
    assert_limit_with_lifo(&mut session, &wall_prog, &wall_entry, LimitKind::WallTime);

    let mock_clock = Arc::new(MockWallClock::new(1_000));
    mock_clock.advance(50);
    session.set_limits(
        ResourceLimits::unlimited()
            .with_wall_time_ms(0)
            .with_clock(mock_clock),
    );
    let (mock_prog, mock_entry) = infinite_loop_in_scope("Lim.WallMock");
    assert!(matches!(
        session.eval_phrase(&mock_prog, &mock_entry, None, false, None),
        EvalOutcome::LimitExceeded {
            kind: LimitKind::WallTime,
            ..
        }
    ));

    session.set_limits(ResourceLimits::unlimited());
    assert_ok(&mut session, "Lim.Ok4", 404);
    session.shutdown().expect("zero leaks after all 4 limits");
}

#[test]
fn dynamic_capability_revocation_unwinds_lifo_finalizers_and_leaves_session_intact() {
    let functions = BTreeMap::from([(
        0,
        fn_def(
            vec![],
            vec![
                let_op(0, Operation::Literal(Lit::Unit)),
                let_op(
                    1,
                    Operation::Intrinsic {
                        kind: Intrinsic::Host(HostPrimitive::CurrentTimeMillis),
                        args: vec![0],
                    },
                ),
            ],
            1,
        ),
    )]);
    let (clock_prog, clock_entry) =
        wrap_in_scope_with_lifo_finalizers("Dyn.Clock", "read_now", functions, 0);

    let mut session = VmSession::with_sandbox(
        HostMode::Deterministic,
        SandboxConfig::untrusted().allow(Capability::Clock),
    );
    let img =
        build_static_image(clock_prog, vec![], Some(clock_entry.clone()), None).expect("image");
    session.load_image(&img).expect("loads with Clock granted");
    session.set_capabilities(CapabilitySet::pure());

    let caller_entry = Name::new("Dyn.Caller", "it");
    let caller_prog = Program {
        functions: BTreeMap::from([(
            0,
            fn_def(vec![], vec![let_op(0, Operation::Global(clock_entry))], 0),
        )]),
        globals: vec![Global {
            name: caller_entry.clone(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    let out = session.eval_phrase(&caller_prog, &caller_entry, None, false, None);
    assert!(
        matches!(
            &out,
            EvalOutcome::CapabilityDenied {
                capability: Capability::Clock,
                stdout,
                ..
            } if stdout == "fin-2\nfin-1\n"
        ),
        "expected dynamic Clock denial with LIFO finalizers, got {out:?}"
    );
    let (ok_prog, ok_entry) = int_phrase("Dyn.Ok", "it", 7);
    assert_eq!(
        session.eval_phrase(&ok_prog, &ok_entry, None, false, None),
        EvalOutcome::Value {
            rendered: "7".into(),
            stdout: String::new(),
        }
    );
    session.shutdown().expect("zero leaks");
}
