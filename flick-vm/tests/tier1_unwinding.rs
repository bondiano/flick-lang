//! LIFO scope finalizer unwinding across success, typed failure, abort, and
//! discontinuation, plus `Host.Console` handler interception on `flick_vm::tier1`.

use std::collections::BTreeMap;

use flick_core::ir::Lit;
use flick_core::ir::memory::{
    Block, Exit, Function, Global, HandlerEffect, HandlerOperation, Instruction, Intrinsic, Name,
    Operation, Program,
};
use flick_core::opt::perceus;
use flick_core::types::OpClass;
use flick_vm::tier0::{self, Error, Heap, ProgramOutcome, Value};
use flick_vm::tier1;

fn let_op(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn print_cb(msg: &str) -> Function {
    perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                let_op(2, Operation::Literal(Lit::String(msg.into()))),
                let_op(
                    3,
                    Operation::Intrinsic {
                        kind: Intrinsic::PrintLine,
                        args: vec![2],
                    },
                ),
            ],
            exit: Exit::Return(3),
        },
    })
    .expect("cb")
}

fn err_only_cb(msg: &str) -> Function {
    perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![let_op(
                2,
                Operation::Intrinsic {
                    kind: Intrinsic::ExitIsFailure,
                    args: vec![1],
                },
            )],
            exit: Exit::Branch {
                condition: 2,
                then_: Box::new(Block {
                    instructions: vec![
                        let_op(3, Operation::Literal(Lit::String(msg.into()))),
                        let_op(
                            4,
                            Operation::Intrinsic {
                                kind: Intrinsic::PrintLine,
                                args: vec![3],
                            },
                        ),
                    ],
                    exit: Exit::Return(4),
                }),
                else_: Box::new(Block {
                    instructions: vec![let_op(5, Operation::Literal(Lit::Unit))],
                    exit: Exit::Return(5),
                }),
            },
        },
    })
    .expect("err_only_cb")
}

fn scoped(defer_code: u32, err_code: u32, mut tail: Vec<Instruction>, ret: u32) -> Function {
    let mut ops = vec![
        let_op(
            2,
            Operation::Closure {
                code: defer_code,
                captures: vec![],
            },
        ),
        let_op(
            3,
            Operation::Intrinsic {
                kind: Intrinsic::ScopeAddClose,
                args: vec![2],
            },
        ),
        let_op(
            4,
            Operation::Closure {
                code: err_code,
                captures: vec![],
            },
        ),
        let_op(
            5,
            Operation::Intrinsic {
                kind: Intrinsic::ScopeAddClose,
                args: vec![4],
            },
        ),
    ];
    ops.append(&mut tail);
    perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: ops,
            exit: Exit::Return(ret),
        },
    })
    .expect("scoped")
}

fn wrap_scope_run(body_code: u32) -> Function {
    perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                let_op(
                    2,
                    Operation::Closure {
                        code: body_code,
                        captures: vec![],
                    },
                ),
                let_op(
                    3,
                    Operation::Intrinsic {
                        kind: Intrinsic::ScopeRun,
                        args: vec![2],
                    },
                ),
            ],
            exit: Exit::Return(3),
        },
    })
    .expect("wrap_scope_run")
}

#[test]
#[allow(clippy::too_many_lines)]
fn finalizers_run_once_in_lifo_order_across_success_failure_abort_and_discontinuation() {
    let entry = Name::new("Lifo", "main");
    let fns = BTreeMap::from([
        (1, print_cb("ok:d")),
        (2, err_only_cb("ok:e-BAD")),
        (3, print_cb("fail:d")),
        (4, err_only_cb("fail:e")),
        (5, print_cb("abort:d")),
        (6, err_only_cb("abort:e")),
        (7, print_cb("disc:d")),
        (8, err_only_cb("disc:e")),
        (
            9,
            scoped(
                1,
                2,
                vec![
                    let_op(6, Operation::Literal(Lit::String("ok:body".into()))),
                    let_op(
                        7,
                        Operation::Intrinsic {
                            kind: Intrinsic::PrintLine,
                            args: vec![6],
                        },
                    ),
                    let_op(8, Operation::Literal(Lit::Int(1))),
                ],
                8,
            ),
        ),
        (
            10,
            scoped(
                3,
                4,
                vec![
                    let_op(6, Operation::Literal(Lit::String("boom".into()))),
                    let_op(
                        7,
                        Operation::Intrinsic {
                            kind: Intrinsic::Raise,
                            args: vec![6],
                        },
                    ),
                ],
                7,
            ),
        ),
        (11, wrap_scope_run(10)),
        (12, print_cb("fail:caught")),
        (
            13,
            perceus::insert(&Function {
                params: vec![0, 1],
                body: Block {
                    instructions: vec![
                        let_op(
                            2,
                            Operation::Closure {
                                code: 12,
                                captures: vec![],
                            },
                        ),
                        let_op(3, Operation::RecoveryResult { selected: Some(2) }),
                    ],
                    exit: Exit::Return(3),
                },
            })
            .expect("13"),
        ),
        (
            14,
            scoped(
                5,
                6,
                vec![
                    let_op(6, Operation::Literal(Lit::Unit)),
                    let_op(
                        7,
                        Operation::Perform {
                            effect: "Stop".into(),
                            op: "stop".into(),
                            dicts: vec![],
                            args: vec![6],
                        },
                    ),
                ],
                7,
            ),
        ),
        (15, wrap_scope_run(14)),
        (16, print_cb("abort:arm")),
        (
            17,
            scoped(
                7,
                8,
                vec![
                    let_op(6, Operation::Literal(Lit::Unit)),
                    let_op(
                        7,
                        Operation::Perform {
                            effect: "Yield".into(),
                            op: "emit".into(),
                            dicts: vec![],
                            args: vec![6],
                        },
                    ),
                ],
                7,
            ),
        ),
        (18, wrap_scope_run(17)),
        (19, print_cb("disc:arm")),
        (
            20,
            perceus::insert(&Function {
                params: vec![0, 1],
                body: Block {
                    instructions: vec![let_op(
                        2,
                        Operation::Closure {
                            code: 19,
                            captures: vec![],
                        },
                    )],
                    exit: Exit::Return(2),
                },
            })
            .expect("20"),
        ),
        (
            0,
            perceus::insert(&Function {
                params: vec![],
                body: Block {
                    instructions: vec![
                        let_op(
                            0,
                            Operation::Closure {
                                code: 9,
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
                        let_op(
                            2,
                            Operation::Closure {
                                code: 11,
                                captures: vec![],
                            },
                        ),
                        let_op(
                            3,
                            Operation::Closure {
                                code: 13,
                                captures: vec![],
                            },
                        ),
                        let_op(
                            4,
                            Operation::Intrinsic {
                                kind: Intrinsic::Try,
                                args: vec![2, 3],
                            },
                        ),
                        let_op(
                            5,
                            Operation::Closure {
                                code: 15,
                                captures: vec![],
                            },
                        ),
                        let_op(
                            6,
                            Operation::Closure {
                                code: 16,
                                captures: vec![],
                            },
                        ),
                        let_op(
                            7,
                            Operation::Handle {
                                body: 5,
                                effects: vec![HandlerEffect {
                                    effect: "Stop".into(),
                                    operations: vec![HandlerOperation {
                                        name: "stop".into(),
                                        class: OpClass::Abort,
                                        dicts: 0,
                                        arity: 1,
                                        function: 6,
                                    }],
                                }],
                                ret: None,
                                errors: None,
                            },
                        ),
                        let_op(
                            8,
                            Operation::Closure {
                                code: 18,
                                captures: vec![],
                            },
                        ),
                        let_op(
                            9,
                            Operation::Closure {
                                code: 20,
                                captures: vec![],
                            },
                        ),
                        let_op(
                            10,
                            Operation::Handle {
                                body: 8,
                                effects: vec![HandlerEffect {
                                    effect: "Yield".into(),
                                    operations: vec![HandlerOperation {
                                        name: "emit".into(),
                                        class: OpClass::Control,
                                        dicts: 0,
                                        arity: 1,
                                        function: 9,
                                    }],
                                }],
                                ret: None,
                                errors: None,
                            },
                        ),
                    ],
                    exit: Exit::Return(10),
                },
            })
            .expect("main"),
        ),
    ]);
    let program = Program {
        functions: fns,
        globals: vec![Global {
            name: entry.clone(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    let expected = "\
ok:body
ok:d
fail:e
fail:d
fail:caught
abort:e
abort:d
abort:arm
disc:arm
disc:e
disc:d
";
    let mut heap0 = Heap::new();
    let r0 = tier0::execute_program_outcome(&mut heap0, &program, &entry, vec![], 100_000);
    let mut heap1 = Heap::new();
    let r1 = tier1::execute_program_outcome(&mut heap1, &program, &entry, vec![], 100_000)
        .expect("tier1 prepare");

    assert_eq!(r1.stdout, expected, "tier1 finalizer order mismatch");
    assert_eq!(r0.stdout, r1.stdout, "tier0 vs tier1 mismatch");
    r0.outcome.release(&mut heap0).expect("release0");
    heap0.finish().expect("zero leaks0");
    r1.outcome.release(&mut heap1).expect("release1");
    heap1.finish().expect("zero leaks1");

    for prefix in [1, 7, 23, 60, 120, 250] {
        let mut heap = Heap::new();
        let report = tier1::execute_program_outcome(&mut heap, &program, &entry, vec![], prefix)
            .expect("prep");
        assert!(matches!(
            report.outcome,
            ProgramOutcome::Returned(_) | ProgramOutcome::Fault(Error::OutOfFuel)
        ));
        report.outcome.release(&mut heap).expect("release prefix");
        heap.finish().expect("zero leaks on prefix");
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn scope_add_close_finalizer_exit_and_console_handler_ir_match_tier0() {
    let entry = Name::new("Res", "main");
    let console_body = perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                let_op(2, Operation::Literal(Lit::String("suppressed".into()))),
                let_op(
                    3,
                    Operation::Perform {
                        effect: "Host.Console".into(),
                        op: "stdout_line".into(),
                        dicts: vec![],
                        args: vec![2],
                    },
                ),
            ],
            exit: Exit::Return(3),
        },
    })
    .expect("console_body");
    let scope_body = perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                let_op(2, Operation::Literal(Lit::Unit)),
                let_op(
                    3,
                    Operation::Intrinsic {
                        kind: Intrinsic::ScopeCurrent,
                        args: vec![2],
                    },
                ),
                let_op(
                    4,
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                ),
                let_op(
                    5,
                    Operation::Intrinsic {
                        kind: Intrinsic::ScopeAddClose,
                        args: vec![4],
                    },
                ),
                let_op(
                    6,
                    Operation::Closure {
                        code: 2,
                        captures: vec![],
                    },
                ),
                let_op(
                    7,
                    Operation::Intrinsic {
                        kind: Intrinsic::ScopeAddFinalizerExit,
                        args: vec![6],
                    },
                ),
                let_op(
                    8,
                    Operation::Closure {
                        code: 3,
                        captures: vec![],
                    },
                ),
                let_op(
                    9,
                    Operation::Intrinsic {
                        kind: Intrinsic::ScopeAddClose,
                        args: vec![8],
                    },
                ),
                let_op(
                    10,
                    Operation::Closure {
                        code: 5,
                        captures: vec![],
                    },
                ),
                let_op(
                    11,
                    Operation::Closure {
                        code: 6,
                        captures: vec![],
                    },
                ),
                let_op(
                    12,
                    Operation::Handle {
                        body: 10,
                        effects: vec![HandlerEffect {
                            effect: "Host.Console".into(),
                            operations: vec![HandlerOperation {
                                name: "stdout_line".into(),
                                class: OpClass::Tail,
                                dicts: 0,
                                arity: 1,
                                function: 11,
                            }],
                        }],
                        ret: None,
                        errors: None,
                    },
                ),
                let_op(13, Operation::Literal(Lit::Int(7))),
            ],
            exit: Exit::Return(13),
        },
    })
    .expect("scope_body");
    let main_fn = perceus::insert(&Function {
        params: vec![],
        body: Block {
            instructions: vec![
                let_op(
                    0,
                    Operation::Closure {
                        code: 4,
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
            exit: Exit::Return(1),
        },
    })
    .expect("main_fn");

    let program = Program {
        functions: BTreeMap::from([
            (0, main_fn),
            (1, print_cb("close-1")),
            (2, print_cb("exit-2")),
            (3, print_cb("close-3")),
            (4, scope_body),
            (5, console_body),
            (6, print_cb("intercepted")),
        ]),
        globals: vec![Global {
            name: entry.clone(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };

    let mut heap0 = Heap::new();
    let r0 = tier0::execute_program_outcome(&mut heap0, &program, &entry, vec![], 10_000);
    let mut heap1 = Heap::new();
    let r1 = tier1::execute_program_outcome(&mut heap1, &program, &entry, vec![], 10_000)
        .expect("tier1 prepare");

    assert_eq!(r1.stdout, "intercepted\nclose-3\nexit-2\nclose-1\n");
    assert_eq!(r0.stdout, r1.stdout);
    assert!(matches!(
        r0.outcome,
        ProgramOutcome::Returned(Value::Int(7))
    ));
    assert!(matches!(
        r1.outcome,
        ProgramOutcome::Returned(Value::Int(7))
    ));

    r0.outcome.release(&mut heap0).expect("release0");
    heap0.finish().expect("zero leaks0");
    r1.outcome.release(&mut heap1).expect("release1");
    heap1.finish().expect("zero leaks1");
}
