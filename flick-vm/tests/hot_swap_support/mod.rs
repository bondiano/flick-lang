//! Shared IR and type-scheme builders for `hot_swap` integration tests.
#![allow(dead_code)]

use std::collections::BTreeMap;

use flick_core::ir::memory::{
    Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program, ValueRep,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::types::{Effects, Elem, Row, Type};

pub fn int_ty() -> Type {
    Type::con("Prelude", "Int")
}

pub fn bool_ty() -> Type {
    Type::con("Prelude", "Bool")
}

pub fn string_ty() -> Type {
    Type::con("Prelude", "String")
}

pub fn unit_ty() -> Type {
    Type::con("Prelude", "Unit")
}

pub fn pair_ty(a: Type, b: Type) -> Type {
    Type::Tuple(vec![a, b])
}

pub fn pure_fun(param: Type, result: Type) -> Type {
    Type::Fun {
        param: Box::new(param),
        effects: None,
        result: Box::new(result),
    }
}

pub fn rt_fun(param: Type, services: Type, errors: Type, result: Type) -> Type {
    Type::Fun {
        param: Box::new(param),
        effects: Some(Box::new(Effects::new(services, errors))),
        result: Box::new(result),
    }
}

pub fn named_row(module: &str, name: &str) -> Type {
    Type::Row(Row {
        entries: vec![(
            Elem::Named(format!("{module}.{name}")),
            Type::con(module, name),
        )],
        tail: None,
        ..Row::default()
    })
}

pub fn add_const_program(module: &str, name: &str, delta: i32) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                }],
                exit: Exit::Return(0),
            },
        },
    );
    functions.insert(
        1,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Literal(Lit::Int(delta)),
                    },
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 1,
                            right: 2,
                        },
                    },
                ],
                exit: Exit::Return(3),
            },
        },
    );
    Program {
        functions,
        globals: vec![Global {
            name: Name::new(module, name),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

pub fn int_global_program(module: &str, name: &str, value: i32) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Literal(Lit::Int(value)),
                }],
                exit: Exit::Return(0),
            },
        },
    );
    Program {
        functions,
        globals: vec![Global {
            name: Name::new(module, name),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

pub fn unary_closure_program(module: &str, name: &str, body: Block) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                }],
                exit: Exit::Return(0),
            },
        },
    );
    functions.insert(
        1,
        Function {
            params: vec![0, 1],
            body,
        },
    );
    Program {
        functions,
        globals: vec![Global {
            name: Name::new(module, name),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

pub fn migrate_int_to_pair_program() -> Program {
    unary_closure_program(
        "Mig",
        "int_to_pair",
        Block {
            instructions: vec![
                Instruction::Drop(0),
                Instruction::Let {
                    binding: 2,
                    value: Operation::Literal(Lit::Int(1)),
                },
                Instruction::Let {
                    binding: 3,
                    value: Operation::Binary {
                        op: BinOp::Arith(Arith::Add, Num::Int),
                        left: 1,
                        right: 2,
                    },
                },
                Instruction::Let {
                    binding: 4,
                    value: Operation::Literal(Lit::Bool(true)),
                },
                Instruction::Let {
                    binding: 5,
                    value: Operation::Construct {
                        tag: "$tuple".to_owned(),
                        fields: vec![3, 4],
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                },
            ],
            exit: Exit::Return(5),
        },
    )
}

pub fn migrate_die_program() -> Program {
    unary_closure_program(
        "Mig",
        "die",
        Block {
            instructions: vec![
                Instruction::Drop(0),
                Instruction::Drop(1),
                Instruction::Let {
                    binding: 2,
                    value: Operation::Literal(Lit::String("bad state".to_owned())),
                },
                Instruction::Let {
                    binding: 3,
                    value: Operation::Intrinsic {
                        kind: Intrinsic::EffectDie,
                        args: vec![2],
                    },
                },
            ],
            exit: Exit::Return(3),
        },
    )
}

pub fn migrate_raise_program() -> Program {
    unary_closure_program(
        "Mig",
        "raise",
        Block {
            instructions: vec![
                Instruction::Drop(0),
                Instruction::Drop(1),
                Instruction::Let {
                    binding: 2,
                    value: Operation::Literal(Lit::String("typed err".to_owned())),
                },
                Instruction::Let {
                    binding: 3,
                    value: Operation::Intrinsic {
                        kind: Intrinsic::Raise,
                        args: vec![2],
                    },
                },
            ],
            exit: Exit::Return(3),
        },
    )
}

pub fn migrate_side_effect_program() -> Program {
    unary_closure_program(
        "Mig",
        "io",
        Block {
            instructions: vec![
                Instruction::Drop(0),
                Instruction::Let {
                    binding: 2,
                    value: Operation::Literal(Lit::String("side effect".to_owned())),
                },
                Instruction::Let {
                    binding: 3,
                    value: Operation::Intrinsic {
                        kind: Intrinsic::PrintLine,
                        args: vec![2],
                    },
                },
                Instruction::Drop(3),
            ],
            exit: Exit::Return(1),
        },
    )
}

#[allow(clippy::too_many_lines)]
pub fn render_two_version_program(v1_delta: i32) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                }],
                exit: Exit::Return(0),
            },
        },
    );
    functions.insert(
        1,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Literal(Lit::Int(1)),
                    },
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 1,
                            right: 2,
                        },
                    },
                    Instruction::Let {
                        binding: 4,
                        value: Operation::Literal(Lit::Int(v1_delta - 1)),
                    },
                    Instruction::Let {
                        binding: 5,
                        value: Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 3,
                            right: 4,
                        },
                    },
                ],
                exit: Exit::Return(5),
            },
        },
    );
    functions.insert(
        2,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Global(Name::new("Ui", "render")),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Global(Name::new("Ui", "render")),
                    },
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Literal(Lit::Int(10)),
                    },
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Apply {
                            function: 1,
                            argument: 2,
                        },
                    },
                    Instruction::Let {
                        binding: 4,
                        value: Operation::Global(Name::new("Ui", "render")),
                    },
                    Instruction::Let {
                        binding: 5,
                        value: Operation::Literal(Lit::Int(10)),
                    },
                    Instruction::Let {
                        binding: 6,
                        value: Operation::Apply {
                            function: 4,
                            argument: 5,
                        },
                    },
                    Instruction::Let {
                        binding: 7,
                        value: Operation::Literal(Lit::Int(10)),
                    },
                    Instruction::Let {
                        binding: 8,
                        value: Operation::Apply {
                            function: 0,
                            argument: 7,
                        },
                    },
                    Instruction::Let {
                        binding: 9,
                        value: Operation::Construct {
                            tag: "$tuple".to_owned(),
                            fields: vec![3, 6, 8],
                            rep: ValueRep::Cell,
                            layout: None,
                        },
                    },
                ],
                exit: Exit::Return(9),
            },
        },
    );
    Program {
        functions,
        globals: vec![
            Global {
                name: Name::new("Ui", "render"),
                initializer: 0,
            },
            Global {
                name: Name::new("App", "driver"),
                initializer: 2,
            },
        ],
        op_spans: BTreeMap::new(),
    }
}

pub fn render_handler_program() -> Program {
    let mut h_funcs = BTreeMap::new();
    h_funcs.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                }],
                exit: Exit::Return(0),
            },
        },
    );
    h_funcs.insert(
        1,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Literal(Lit::Unit),
                    },
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Apply {
                            function: 1,
                            argument: 2,
                        },
                    },
                    Instruction::Let {
                        binding: 4,
                        value: Operation::Global(Name::new("Ui", "render")),
                    },
                    Instruction::Let {
                        binding: 5,
                        value: Operation::Apply {
                            function: 4,
                            argument: 3,
                        },
                    },
                ],
                exit: Exit::Return(5),
            },
        },
    );
    Program {
        functions: h_funcs,
        globals: vec![Global {
            name: Name::new("Repl.H1", "h"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

pub fn int_task_phrase(mod_name: &str, input: i32) -> Program {
    let mut f = BTreeMap::new();
    f.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Task {
                        code: 1,
                        captures: vec![],
                    },
                }],
                exit: Exit::Return(0),
            },
        },
    );
    f.insert(
        1,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    Instruction::Drop(1),
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Literal(Lit::Int(input)),
                    },
                ],
                exit: Exit::Return(2),
            },
        },
    );
    Program {
        functions: f,
        globals: vec![Global {
            name: Name::new(mod_name, "it"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}
