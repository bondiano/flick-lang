//! Owned handler boundaries and operation transfer have independent expected values.
#[path = "support/causes.rs"]
pub mod causes;
#[path = "support/recovery.rs"]
mod recovery;
use flick_core::ir::{
    BinOp, Lit,
    memory::{
        Block, CodeId, Exit, Function, Global, HandlerEffect, HandlerOperation, Instruction,
        Intrinsic, Local, Name, Operation, Program,
    },
};
use flick_core::numeric::{Arith, Num};
use flick_core::opt::perceus;
use flick_core::types::OpClass;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};

fn bind(binding: Local, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn closure(binding: Local, code: CodeId) -> Instruction {
    bind(
        binding,
        Operation::Closure {
            code,
            captures: vec![],
        },
    )
}
fn function(params: &[Local], instructions: Vec<Instruction>, result: Local) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("handler test ownership")
}
fn program(functions: Vec<Function>) -> Program {
    Program {
        functions: functions
            .into_iter()
            .enumerate()
            .map(|(n, f)| (u32::try_from(n).expect("valid handler test operation"), f))
            .collect(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    }
}
fn entry() -> Name {
    Name::new("Main", "main")
}
fn handled(operations: &[(&str, Local)]) -> Vec<HandlerEffect> {
    vec![HandlerEffect {
        effect: "Pair".into(),
        operations: operations
            .iter()
            .map(|(name, function)| HandlerOperation {
                name: (*name).into(),
                class: OpClass::Tail,
                dicts: 0,
                arity: 1,
                function: *function,
            })
            .collect(),
    }]
}
fn handle(binding: Local, body: Local, operations: &[(&str, Local)]) -> Instruction {
    bind(
        binding,
        Operation::Handle {
            body,
            effects: handled(operations),
            ret: None,
            errors: None,
        },
    )
}
fn perform(binding: Local, op: &str, args: &[Local]) -> Instruction {
    bind(
        binding,
        Operation::Perform {
            effect: "Pair".into(),
            op: op.into(),
            dicts: vec![],
            args: args.to_vec(),
        },
    )
}
fn constant(n: i32) -> Function {
    function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(n)))], 2)
}
fn partial_handlers() -> Program {
    program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 2),
                closure(2, 3),
                handle(3, 0, &[("left", 1), ("right", 2)]),
            ],
            3,
        ),
        function(
            &[0, 1],
            vec![closure(2, 4), closure(3, 5), handle(4, 2, &[("left", 3)])],
            4,
        ),
        constant(10),
        constant(20),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::Unit)),
                perform(3, "left", &[2]),
                perform(4, "right", &[2]),
                bind(
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
        constant(1),
    ])
}
include!("memory_handlers/dispatch.rs");
include!("memory_handlers/operation_ownership.rs");
include!("memory_handlers/abandonment.rs");
