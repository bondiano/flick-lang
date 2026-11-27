//! Delayed task and execution-owned cell roots use explicit ownership.
use std::collections::BTreeMap;

use flick_core::ir::memory::{
    Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::opt::perceus;
use flick_vm::tier0::{Error, Executed, Heap, Value, execute_program};

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
    .expect("ownership insertion")
}
fn program(mut bodies: Vec<Function>) -> Program {
    bodies.insert(
        0,
        function(
            &[],
            vec![bind(
                0,
                Operation::Closure {
                    code: 1,
                    captures: vec![],
                },
            )],
            0,
        ),
    );
    Program {
        functions: bodies
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
fn run(heap: &mut Heap, p: &Program, args: Vec<Value>, fuel: u64) -> Result<Executed, Error> {
    execute_program(heap, p, &Name::new("Main", "main"), args, fuel)
}
include!("memory_tasks_refs/tasks_cells.rs");
include!("memory_tasks_refs/arena_cleanup.rs");
