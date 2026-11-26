//! Typed failure is an owned outcome, distinct from an uncatchable VM fault.
use crate::causes;
use crate::recovery;
use flick_core::ir::{
    Lit,
    memory::{
        Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program, ValueRep,
    },
};
use flick_core::opt::perceus;
use flick_vm::tier0::{
    Error, Heap, ProgramOutcome, Value, execute, execute_program, execute_program_outcome,
};

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
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
fn function(params: &[u32], instructions: Vec<Instruction>, result: u32) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("valid typed-failure test")
}
fn program(functions: Vec<Function>) -> Program {
    Program {
        functions: functions
            .into_iter()
            .enumerate()
            .map(|(i, f)| (u32::try_from(i).expect("small test program"), f))
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
fn failure(caught: bool, malformed: bool) -> Program {
    let body = function(
        &[0, 1],
        vec![
            bind(2, Operation::Literal(Lit::String("before".into()))),
            intrinsic(3, Intrinsic::PrintLine, &[2]),
            bind(4, Operation::Literal(Lit::String("payload".into()))),
            bind(
                5,
                Operation::Construct {
                    tag: if malformed {
                        "Other.Err"
                    } else {
                        "Prelude.Result.Err"
                    }
                    .into(),
                    fields: vec![4],
                    rep: ValueRep::Cell,
                    layout: None,
                },
            ),
            intrinsic(6, Intrinsic::FromResult, &[5]),
        ],
        6,
    );
    if caught {
        let mut p = program(vec![
            function(
                &[],
                vec![
                    bind(
                        0,
                        Operation::Closure {
                            code: 1,
                            captures: vec![],
                        },
                    ),
                    bind(
                        1,
                        Operation::Closure {
                            code: 2,
                            captures: vec![],
                        },
                    ),
                    intrinsic(2, Intrinsic::Try, &[0, 1]),
                ],
                2,
            ),
            body,
            function(&[0, 1], vec![], 1),
        ]);
        recovery::seal_bodies(&mut p, &[2]);
        p
    } else {
        program(vec![
            function(
                &[],
                vec![
                    bind(
                        0,
                        Operation::Closure {
                            code: 1,
                            captures: vec![],
                        },
                    ),
                    bind(1, Operation::Literal(Lit::Unit)),
                    bind(
                        2,
                        Operation::Apply {
                            function: 0,
                            argument: 1,
                        },
                    ),
                ],
                2,
            ),
            body,
        ])
    }
}
include!("memory_failures/outcomes.rs");
include!("memory_failures/scope_unwind.rs");
