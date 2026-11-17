//! Constant providers own real counted values and preserve caller execution.
use crate::causes;
use crate::recovery;
use flick_core::ir::{
    Lit,
    memory::{
        Block, Exit, Function, Global, HandlerEffect, HandlerOperation, Instruction, Intrinsic,
        Local, Name, Operation, Program,
    },
};
use flick_core::opt::perceus;
use flick_core::types::OpClass;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
fn bind(binding: Local, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn function(params: &[Local], instructions: Vec<Instruction>, result: Local) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("layer ownership")
}
fn entry() -> Name {
    Name::new("Main", "main")
}
fn program(functions: Vec<Function>) -> Program {
    Program {
        functions: functions
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                (
                    u32::try_from(i).expect("valid counted Layer test operation"),
                    f,
                )
            })
            .collect(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    }
}
fn closure(binding: Local, code: u32) -> Instruction {
    bind(
        binding,
        Operation::Closure {
            code,
            captures: vec![],
        },
    )
}
fn intrinsic(binding: Local, kind: Intrinsic, args: &[Local]) -> Instruction {
    bind(
        binding,
        Operation::Intrinsic {
            kind,
            args: args.to_vec(),
        },
    )
}
fn constant_layer() -> Program {
    program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                bind(1, Operation::Literal(Lit::Int(42))),
                bind(
                    2,
                    Operation::Record {
                        fields: vec![("answer".into(), 1)],
                    },
                ),
                intrinsic(3, Intrinsic::LayerSucceed, &[0, 2]),
                closure(4, 1),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Perform {
                    effect: "Service".into(),
                    op: "answer".into(),
                    dicts: vec![],
                    args: vec![],
                },
            )],
            2,
        ),
    ])
}
include!("memory_layers/provider_values.rs");
include!("memory_layers/method_dispatch.rs");
include!("memory_layers/scope_outcomes.rs");
include!("memory_layers/integrity.rs");
include!("memory_layers/parked_providers.rs");
