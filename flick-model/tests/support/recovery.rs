//! Test-only explicit selectors for raw MIR fixtures whose callbacks are bodies.
//! This builds the new MIR protocol; the evaluator has no legacy return fallback.
use flick_core::{
    ir::memory::{Block, Exit, Function, Instruction, Operation, Program, ValueRep},
    opt::perceus,
};

pub fn seal_bodies(program: &mut Program, codes: &[u32]) {
    for &code in codes {
        let body_code = program.functions.keys().next_back().copied().unwrap_or(0) + 1;
        let thunk_code = body_code + 1;
        let original = program
            .functions
            .remove(&code)
            .expect("explicit fixture recovery body");
        assert_eq!(original.params, [0, 1], "unary body fixture");
        program.functions.insert(body_code, original);
        let bind = |binding, value| Instruction::Let { binding, value };
        let owned = |instructions, result| {
            perceus::insert(&Function {
                params: vec![0, 1],
                body: Block {
                    instructions,
                    exit: Exit::Return(result),
                },
            })
            .expect("explicit sealed selector tokens")
        };
        program.functions.insert(
            thunk_code,
            owned(
                vec![
                    bind(
                        2,
                        Operation::Project {
                            object: 0,
                            field: 0,
                            rep: ValueRep::Cell,
                        },
                    ),
                    bind(
                        3,
                        Operation::Project {
                            object: 0,
                            field: 1,
                            rep: ValueRep::Cell,
                        },
                    ),
                    bind(
                        4,
                        Operation::Call {
                            code: body_code,
                            args: vec![2, 3],
                        },
                    ),
                ],
                4,
            ),
        );
        program.functions.insert(
            code,
            owned(
                vec![
                    bind(
                        2,
                        Operation::Closure {
                            code: thunk_code,
                            captures: vec![0, 1],
                        },
                    ),
                    bind(3, Operation::RecoveryResult { selected: Some(2) }),
                ],
                3,
            ),
        );
    }
}
