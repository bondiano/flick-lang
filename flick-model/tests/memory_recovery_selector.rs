//! Sealed selection is distinct from executing an ordinary recovery body.
#[path = "support/causes.rs"]
pub mod causes;
use flick_core::ir::memory::validate_program;
use flick_core::{
    ir::{
        Lit,
        memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
    },
    opt::perceus,
};
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
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
    .expect("valid owned recovery MIR")
}
fn fault_or_fuel(heap: &mut Heap, outcome: ProgramOutcome, expected: &Error) -> bool {
    let reached = matches!(&outcome, ProgramOutcome::Fault(actual) if actual == expected);
    let valid = reached || matches!(&outcome, ProgramOutcome::Fault(Error::OutOfFuel));
    let diagnostic = format!("expected {expected:?} or fuel: {outcome:?}");
    outcome
        .release(heap)
        .expect("all observed outcomes explicitly released");
    assert!(valid, "{diagnostic}");
    reached
}

fn ordinary_selector() -> Program {
    let closure = |binding, code| {
        bind(
            binding,
            Operation::Closure {
                code,
                captures: vec![],
            },
        )
    };
    Program {
        globals: vec![Global {
            name: Name::new("Recovery", "main"),
            initializer: 0,
        }],
        functions: [
            (
                0,
                function(
                    &[],
                    vec![
                        closure(0, 1),
                        closure(1, 2),
                        intrinsic(2, Intrinsic::Try, &[0, 1]),
                    ],
                    2,
                ),
            ),
            (
                1,
                function(
                    &[0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::String("original".into()))),
                        intrinsic(3, Intrinsic::Raise, &[2]),
                    ],
                    3,
                ),
            ),
            (
                2,
                function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(42)))], 2),
            ),
        ]
        .into(),
    }
}
include!("memory_recovery_selector/sealed_results.rs");
include!("memory_recovery_selector/authority_boundaries.rs");
#[path = "support/recovery_effects.rs"]
mod effects;
use effects::selection_effects;
include!("memory_recovery_selector/callback_boundaries.rs");
include!("memory_recovery_selector/foreign_selection.rs");
include!("memory_recovery_selector/payload_lookalikes.rs");
