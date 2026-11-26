//! Pattern selection commits owned bindings only after complete success.
use flick_core::ir::memory::{
    Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program,
};
use flick_core::ir::{BinOp, Lit, Pat};
use flick_core::opt::perceus;
use flick_core::types::{Constructor, Rep};
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
    .expect("valid ownership input")
}
fn program(body: Function) -> Program {
    Program {
        functions: [
            (
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
            ),
            (1, body),
        ]
        .into_iter()
        .collect(),
        globals: vec![Global {
            name: Name::new("Main", "main"),
            initializer: 0,
        }],
    }
}
fn run(heap: &mut Heap, program: &Program, input: Value, fuel: u64) -> Result<Value, Error> {
    execute_program(heap, program, &Name::new("Main", "main"), vec![input], fuel)
        .map(|ran| ran.value)
}
include!("memory_patterns/matching.rs");
include!("memory_patterns/updates_roots.rs");
include!("memory_patterns/equality.rs");
include!("memory_patterns/evidence_reuse.rs");
