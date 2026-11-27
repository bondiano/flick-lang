//! Executes ownership-annotated code, closures and explicit global roots.
use std::collections::BTreeMap;

use flick_core::ir::memory::{
    Block, Exit, Function, Global, Instruction, Name, Operation, Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::opt::perceus;
use flick_vm::tier0::{Error, Heap, Value, execute_program};

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn function(params: &[u32], instructions: Vec<Instruction>, result: u32) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("ownership insertion")
}
fn program(functions: Vec<Function>) -> Program {
    Program {
        functions: functions
            .into_iter()
            .enumerate()
            .map(|(i, f)| (u32::try_from(i).expect("small program"), f))
            .collect::<BTreeMap<_, _>>(),
        globals: vec![Global {
            name: Name::new("Main", "main"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}
include!("memory_program/closures_globals.rs");
include!("memory_program/unwind_ownership.rs");
include!("memory_program/primitive_values.rs");
