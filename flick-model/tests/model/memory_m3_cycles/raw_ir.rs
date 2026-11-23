//! Raw memory-IR tests for `Ref` cycle overwrite reclamation and external aliases.

use flick_core::ir::{
    Lit,
    memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
};
use flick_core::opt::perceus;
use flick_model::memory::{Heap, ProgramOutcome, Value, execute_program, execute_program_outcome};

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

pub(super) fn broken_cycle_raw_program() -> (Name, Program) {
    let body = perceus::insert(&Function {
        params: vec![],
        body: Block {
            instructions: vec![
                bind(1, Operation::Literal(Lit::Unit)),
                bind(
                    2,
                    Operation::Intrinsic {
                        kind: Intrinsic::RefMake,
                        args: vec![1],
                    },
                ),
                bind(3, Operation::Literal(Lit::String("cyclic".into()))),
                bind(
                    4,
                    Operation::Construct {
                        tag: "M.Ring.Ring".into(),
                        fields: vec![3, 2],
                    },
                ),
                bind(
                    5,
                    Operation::Intrinsic {
                        kind: Intrinsic::RefSet,
                        args: vec![2, 4],
                    },
                ),
                bind(6, Operation::Literal(Lit::Unit)),
                bind(
                    7,
                    Operation::Intrinsic {
                        kind: Intrinsic::RefSet,
                        args: vec![2, 6],
                    },
                ),
            ],
            exit: Exit::Return(7),
        },
    })
    .expect("valid perceus IR");
    let name = Name::new("Cycles", "main");
    let program = Program {
        functions: [(0, body)].into_iter().collect(),
        globals: vec![Global {
            name: name.clone(),
            initializer: 0,
        }],
    };
    (name, program)
}

#[test]
fn broken_cycle_raw_vm_reclaims_payload_before_teardown() {
    let (name, program) = broken_cycle_raw_program();
    let mut heap = Heap::new();
    let res = execute_program(&mut heap, &program, &name, vec![], 10_000).expect("broken cycle");
    heap.drop_value(res.value).expect("drop unit");
    let stats = heap.finish().expect("zero leaks");
    assert_eq!(stats.cells_allocated, 1);
    assert_eq!(stats.cells_freed, 1);
    assert_eq!(
        stats.cells_detached_on_teardown, 0,
        "overwriting with Unit broke the cycle before teardown, so zero heap objects remained in cells at teardown"
    );
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn broken_cycle_frees_old_payload_mid_execution_while_unbroken_retains_until_teardown() {
    let make_program = |break_before_return: bool| {
        let mut instructions = vec![
            bind(1, Operation::Literal(Lit::Unit)),
            bind(
                2,
                Operation::Intrinsic {
                    kind: Intrinsic::RefMake,
                    args: vec![1],
                },
            ),
            bind(3, Operation::Literal(Lit::String("ring-payload".into()))),
            bind(
                4,
                Operation::Construct {
                    tag: "M.Ring.Ring".into(),
                    fields: vec![3, 2],
                },
            ),
            bind(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::RefSet,
                    args: vec![2, 4],
                },
            ),
        ];
        let ret = if break_before_return {
            instructions.push(bind(6, Operation::Literal(Lit::Unit)));
            instructions.push(bind(
                7,
                Operation::Intrinsic {
                    kind: Intrinsic::RefSet,
                    args: vec![2, 6],
                },
            ));
            7
        } else {
            5
        };
        let body = perceus::insert(&Function {
            params: vec![0, 10],
            body: Block {
                instructions,
                exit: Exit::Return(ret),
            },
        })
        .expect("valid IR");
        let init = perceus::insert(&Function {
            params: vec![],
            body: Block {
                instructions: vec![bind(
                    0,
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                )],
                exit: Exit::Return(0),
            },
        })
        .expect("init IR");
        let name = Name::new("Cycles", "main");
        (
            name.clone(),
            Program {
                functions: [(0, init), (1, body)].into_iter().collect(),
                globals: vec![Global {
                    name,
                    initializer: 0,
                }],
            },
        )
    };

    let (name, unbroken) = make_program(false);
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &unbroken, &name, vec![Value::Unit], 10_000);
    report.outcome.release(&mut heap).expect("release");
    let unbroken_stats = heap.finish().expect("unbroken finish");
    assert_eq!(unbroken_stats.cells_detached_on_teardown, 1);

    let (name, broken) = make_program(true);
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &broken, &name, vec![Value::Unit], 10_000);
    report.outcome.release(&mut heap).expect("release");
    let broken_stats = heap.finish().expect("broken finish");
    assert_eq!(broken_stats.cells_detached_on_teardown, 0);
}

#[test]
fn external_alias_stored_in_cyclic_ref_survives_teardown_untouched() {
    let body = perceus::insert(&Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                bind(2, Operation::Literal(Lit::Unit)),
                bind(
                    3,
                    Operation::Intrinsic {
                        kind: Intrinsic::RefMake,
                        args: vec![2],
                    },
                ),
                bind(
                    4,
                    Operation::Construct {
                        tag: "M.Ring.Ring".into(),
                        fields: vec![1, 3],
                    },
                ),
                bind(
                    5,
                    Operation::Intrinsic {
                        kind: Intrinsic::RefSet,
                        args: vec![3, 4],
                    },
                ),
            ],
            exit: Exit::Return(5),
        },
    })
    .expect("valid IR");
    let init = perceus::insert(&Function {
        params: vec![],
        body: Block {
            instructions: vec![bind(
                0,
                Operation::Closure {
                    code: 1,
                    captures: vec![],
                },
            )],
            exit: Exit::Return(0),
        },
    })
    .expect("init IR");
    let name = Name::new("Cycles", "main");
    let program = Program {
        functions: [(0, init), (1, body)].into_iter().collect(),
        globals: vec![Global {
            name: name.clone(),
            initializer: 0,
        }],
    };

    let mut completed = false;
    for fuel in 0..=200 {
        let mut heap = Heap::new();
        let input = heap
            .literal(&Lit::String("shared-across-cycle".into()))
            .expect("input");
        let external = heap.dup(&input).expect("external alias");
        let ext_id = external.object_id().expect("external id");

        let report = execute_program_outcome(&mut heap, &program, &name, vec![input], fuel);
        if matches!(report.outcome, ProgramOutcome::Returned(Value::Unit)) {
            completed = true;
            assert_eq!(heap.stats().cells_detached_on_teardown, 1);
        }
        report.outcome.release(&mut heap).expect("release");
        assert!(
            heap.is_live(ext_id),
            "fuel={fuel}: external alias must survive cyclic Ref teardown"
        );
        assert_eq!(heap.string(&external), Ok("shared-across-cycle"));
        assert_eq!(
            heap.stats().live_objects,
            1,
            "fuel={fuel}: only external alias may remain live after teardown"
        );
        heap.drop_value(external).expect("drop external");
        let stats = heap.finish().expect("empty heap");
        assert_eq!(stats.allocations, stats.frees);
        assert_eq!(stats.cells_allocated, stats.cells_freed);
        if completed {
            break;
        }
    }
    assert!(completed, "external alias cycle test never completed");
}
