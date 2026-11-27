//! Unit tests for tier-1 handler lookup epoch caching and invalidation.

use std::collections::BTreeMap;

use flick_core::ir::memory::{
    Block, Exit, Function, Global, HandlerEffect, HandlerOperation, Instruction, Name, Operation,
    Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::opt::perceus;
use flick_core::types::OpClass;

use crate::tier0::{Heap, ProgramOutcome, Value};
use crate::tier1;
use crate::tier1::machine::execute_prepared_cache_stats;

fn let_op(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn const_handler(val: i32) -> Function {
    perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![let_op(2, Operation::Literal(Lit::Int(val)))],
            exit: Exit::Return(2),
        },
    })
    .expect("valid handler fn")
}

fn ask_effect(function: u32) -> Vec<HandlerEffect> {
    vec![HandlerEffect {
        effect: "Ask".into(),
        operations: vec![HandlerOperation {
            name: "ask".into(),
            class: OpClass::Tail,
            dicts: 0,
            arity: 1,
            function,
        }],
    }]
}

#[test]
#[allow(clippy::too_many_lines)]
fn handler_lookup_cache_hits_and_invalidates_on_epoch_change() {
    let entry = Name::new("Cache", "main");
    let inner_body = perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                let_op(2, Operation::Literal(Lit::Unit)),
                let_op(
                    3,
                    Operation::Perform {
                        effect: "Ask".into(),
                        op: "ask".into(),
                        dicts: vec![],
                        args: vec![2],
                    },
                ),
            ],
            exit: Exit::Return(3),
        },
    })
    .expect("inner body");
    let outer_body = perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                let_op(2, Operation::Literal(Lit::Unit)),
                let_op(
                    3,
                    Operation::Perform {
                        effect: "Ask".into(),
                        op: "ask".into(),
                        dicts: vec![],
                        args: vec![2],
                    },
                ),
                let_op(4, Operation::Literal(Lit::Unit)),
                let_op(
                    5,
                    Operation::Perform {
                        effect: "Ask".into(),
                        op: "ask".into(),
                        dicts: vec![],
                        args: vec![4],
                    },
                ),
                let_op(6, Operation::Literal(Lit::Unit)),
                let_op(
                    7,
                    Operation::Perform {
                        effect: "Ask".into(),
                        op: "ask".into(),
                        dicts: vec![],
                        args: vec![6],
                    },
                ),
                let_op(
                    8,
                    Operation::Closure {
                        code: 3,
                        captures: vec![],
                    },
                ),
                let_op(
                    9,
                    Operation::Closure {
                        code: 4,
                        captures: vec![],
                    },
                ),
                let_op(
                    10,
                    Operation::Handle {
                        body: 8,
                        effects: ask_effect(9),
                        ret: None,
                        errors: None,
                    },
                ),
                let_op(11, Operation::Literal(Lit::Unit)),
                let_op(
                    12,
                    Operation::Perform {
                        effect: "Ask".into(),
                        op: "ask".into(),
                        dicts: vec![],
                        args: vec![11],
                    },
                ),
                let_op(
                    13,
                    Operation::Binary {
                        op: BinOp::Arith(Arith::Add, Num::Int),
                        left: 3,
                        right: 5,
                    },
                ),
                let_op(
                    14,
                    Operation::Binary {
                        op: BinOp::Arith(Arith::Add, Num::Int),
                        left: 13,
                        right: 7,
                    },
                ),
                let_op(
                    15,
                    Operation::Binary {
                        op: BinOp::Arith(Arith::Add, Num::Int),
                        left: 14,
                        right: 10,
                    },
                ),
                let_op(
                    16,
                    Operation::Binary {
                        op: BinOp::Arith(Arith::Add, Num::Int),
                        left: 15,
                        right: 12,
                    },
                ),
            ],
            exit: Exit::Return(16),
        },
    })
    .expect("outer body");
    let main_fn = perceus::insert(&Function {
        params: vec![],
        body: Block {
            instructions: vec![
                let_op(
                    0,
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                ),
                let_op(
                    1,
                    Operation::Closure {
                        code: 2,
                        captures: vec![],
                    },
                ),
                let_op(
                    2,
                    Operation::Handle {
                        body: 0,
                        effects: ask_effect(1),
                        ret: None,
                        errors: None,
                    },
                ),
            ],
            exit: Exit::Return(2),
        },
    })
    .expect("main fn");
    let program = Program {
        functions: BTreeMap::from([
            (0, main_fn),
            (1, outer_body),
            (2, const_handler(10)),
            (3, inner_body),
            (4, const_handler(1)),
        ]),
        globals: vec![Global {
            name: entry.clone(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    };
    let prepared = tier1::prepare(&program, &entry).expect("prepare");
    let mut heap = Heap::new();
    let (report, hits, misses, epoch) = execute_prepared_cache_stats(&mut heap, &prepared, 10_000);
    // 10 + 10 + 10 + 1 + 10 = 41
    assert!(matches!(
        report.outcome,
        ProgramOutcome::Returned(Value::Int(41))
    ));
    // First outer Ask.ask misses; 2nd and 3rd outer Ask.ask hit the epoch cache.
    // Inner handler install bumps epoch -> miss for inner Ask.ask;
    // Inner handler exit bumps epoch -> miss for final outer Ask.ask.
    assert_eq!(
        hits, 2,
        "expected 2 cache hits on repeated tail performs, got hits={hits} misses={misses}"
    );
    assert_eq!(
        misses, 3,
        "expected 3 cache misses across outer/inner/outer epochs, got hits={hits} misses={misses}"
    );
    assert!(epoch > 0, "expected epoch advances, got {epoch}");
    report.outcome.release(&mut heap).expect("release");
    heap.finish().expect("zero leaks");
}
