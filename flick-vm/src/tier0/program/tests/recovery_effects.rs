// Shared raw MIR fixtures for actual current-selection handler transfers.
use flick_core::{
    ir::{
        Lit,
        memory::{
            Block, Exit, Function, Global, HandlerEffect, HandlerOperation, Instruction, Intrinsic,
            Name, Operation, Program, ValueRep,
        },
    },
    opt::perceus,
    types::OpClass,
};

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn function(instructions: Vec<Instruction>, result: u32) -> Function {
    perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("explicit recovery transfer tokens")
}
fn closure(binding: u32, code: u32) -> Instruction {
    bind(
        binding,
        Operation::Closure {
            code,
            captures: vec![],
        },
    )
}
fn text(binding: u32, value: &str) -> Instruction {
    bind(binding, Operation::Literal(Lit::String(value.into())))
}
fn intrinsic(binding: u32, kind: Intrinsic, args: &[u32]) -> Instruction {
    bind(
        binding,
        Operation::Intrinsic {
            kind,
            args: args.to_vec(),
        },
    )
}

fn initializer(abort: bool) -> Function {
    perceus::insert(&Function {
        params: vec![],
        body: Block {
            instructions: vec![
                closure(0, 1),
                closure(1, 2),
                bind(
                    2,
                    Operation::Handle {
                        body: 0,
                        effects: vec![HandlerEffect {
                            effect: "Pair".into(),
                            operations: vec![HandlerOperation {
                                name: "left".into(),
                                class: if abort { OpClass::Abort } else { OpClass::Tail },
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
            exit: Exit::Return(2),
        },
    })
    .expect("outer handler tokens")
}
fn recovery_boundary(handler_errors: bool) -> Function {
    function(
        vec![
            closure(2, 3),
            closure(3, 4),
            bind(
                4,
                if handler_errors {
                    Operation::Handle {
                        body: 2,
                        effects: vec![],
                        ret: None,
                        errors: Some(3),
                    }
                } else {
                    Operation::Intrinsic {
                        kind: Intrinsic::Try,
                        args: vec![2, 3],
                    }
                },
            ),
        ],
        4,
    )
}
fn operation_arm(arm_fails: bool) -> Function {
    function(
        vec![
            text(2, "arm"),
            intrinsic(3, Intrinsic::PrintLine, &[2]),
            text(4, "replacement"),
            if arm_fails {
                intrinsic(5, Intrinsic::Raise, &[4])
            } else {
                bind(5, Operation::Literal(Lit::Int(7)))
            },
        ],
        5,
    )
}
fn selection_guard() -> Function {
    function(
        vec![
            text(2, "guard"),
            intrinsic(3, Intrinsic::PrintLine, &[2]),
            bind(4, Operation::Literal(Lit::Unit)),
            bind(
                5,
                Operation::Perform {
                    effect: "Pair".into(),
                    op: "left".into(),
                    dicts: vec![],
                    args: vec![4],
                },
            ),
            closure(6, 5),
            bind(7, Operation::RecoveryResult { selected: Some(6) }),
        ],
        7,
    )
}
fn selected_body() -> Function {
    function(
        vec![
            text(2, "body"),
            intrinsic(3, Intrinsic::PrintLine, &[2]),
            bind(4, Operation::Literal(Lit::Int(42))),
        ],
        4,
    )
}
pub fn selection_effects(handler_errors: bool, abort: bool, arm_fails: bool) -> Program {
    Program {
        globals: vec![Global {
            name: Name::new("Recovery", "main"),
            initializer: 0,
        }],
        functions: [
            (0, initializer(abort)),
            (1, recovery_boundary(handler_errors)),
            (2, operation_arm(arm_fails)),
            (
                3,
                function(
                    vec![text(2, "original"), intrinsic(3, Intrinsic::Raise, &[2])],
                    3,
                ),
            ),
            (4, selection_guard()),
            (5, selected_body()),
        ]
        .into(),
    }
}

pub fn tail_failure_outer_recovery(handler_errors: bool) -> Program {
    let mut program = selection_effects(handler_errors, false, true);
    program.functions.insert(
        0,
        perceus::insert(&Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    closure(0, 6),
                    closure(1, 7),
                    intrinsic(2, Intrinsic::Try, &[0, 1]),
                ],
                exit: Exit::Return(2),
            },
        })
        .expect("outer recovery initializer"),
    );
    program.functions.insert(
        6,
        function(
            vec![
                closure(2, 1),
                closure(3, 2),
                bind(
                    4,
                    Operation::Handle {
                        body: 2,
                        effects: vec![HandlerEffect {
                            effect: "Pair".into(),
                            operations: vec![HandlerOperation {
                                name: "left".into(),
                                class: OpClass::Tail,
                                dicts: 0,
                                arity: 1,
                                function: 3,
                            }],
                        }],
                        ret: None,
                        errors: None,
                    },
                ),
            ],
            4,
        ),
    );
    program.functions.insert(
        7,
        function(
            vec![
                text(2, "outer"),
                intrinsic(3, Intrinsic::PrintLine, &[2]),
                bind(
                    4,
                    Operation::Closure {
                        code: 8,
                        captures: vec![1],
                    },
                ),
                bind(5, Operation::RecoveryResult { selected: Some(4) }),
            ],
            5,
        ),
    );
    program.functions.insert(
        8,
        function(
            vec![bind(
                2,
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            )],
            2,
        ),
    );
    program
}
