//! Raw graph ownership, right precedence and metered traversal.
use crate::causes;
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
    .expect("valid ownership insertion")
}
fn entry() -> Name {
    Name::new("Merge", "answer")
}
fn program(instructions: Vec<Instruction>, result: u32, callback: Function) -> Program {
    Program {
        functions: [
            (
                0,
                function(
                    &[],
                    vec![bind(
                        0,
                        Operation::Closure {
                            code: 2,
                            captures: vec![],
                        },
                    )],
                    0,
                ),
            ),
            (1, callback),
            (2, function(&[99, 0], instructions, result)),
        ]
        .into(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    }
}
fn perform(effect: &str, op: &str) -> Operation {
    Operation::Perform {
        effect: effect.into(),
        op: op.into(),
        dicts: vec![],
        args: vec![],
    }
}
fn base() -> Vec<Instruction> {
    vec![
        bind(1, Operation::ServiceTag("Read".into())),
        bind(2, Operation::Literal(Lit::Int(10))),
        bind(
            3,
            Operation::Record {
                fields: vec![("read".into(), 2)],
            },
        ),
        intrinsic(4, Intrinsic::LayerSucceed, &[1, 3]),
        bind(5, Operation::Literal(Lit::Int(42))),
        bind(
            6,
            Operation::Record {
                fields: vec![("read".into(), 5)],
            },
        ),
        intrinsic(7, Intrinsic::LayerSucceed, &[1, 6]),
    ]
}
fn callback() -> Function {
    function(&[0, 1], vec![bind(2, perform("Read", "read"))], 2)
}

fn check(program: &Program, expected: &Result<i32, Error>, max: u64) {
    let mut complete = false;
    for fuel in 0..=max {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("external".into()))
            .expect("allocate external alias root");
        let argument = heap.dup(&external).expect("retain caller alias");
        let outcome =
            execute_program_outcome(&mut heap, program, &entry(), vec![argument], fuel).outcome;
        let correct = match outcome {
            ProgramOutcome::Fault(Error::OutOfFuel) => true,
            ProgramOutcome::Fault(ref error) => {
                complete = true;
                expected.as_ref().is_err_and(|want| want == error)
            }
            ProgramOutcome::Returned(value) => {
                complete = true;
                let correct = matches!(value, Value::Int(n) if expected == &Ok(n));
                heap.drop_value(value).expect("release owned outcome");
                correct
            }
            ProgramOutcome::Raised(value) => {
                heap.drop_value(value).expect("release owned outcome");
                false
            }
            ProgramOutcome::Failed(cause) => {
                heap.drop_cause(cause).expect("release unexpected cause");
                false
            }
        };
        assert_eq!(heap.string(&external), Ok("external"));
        assert_eq!(heap.stats().live_objects, 1, "fuel {fuel}");
        heap.drop_value(external)
            .expect("release external alias root");
        let stats = heap.finish().expect("all real graph edges drained");
        assert_eq!(stats.allocations, stats.frees);
        assert_eq!(stats.arenas_allocated, stats.arenas_freed);
        assert!(correct, "fuel {fuel} expected {expected:?}");
    }
    assert!(complete, "must include terminal outcome");
}

#[test]
fn rightmost_duplicate_and_nested_same_graph_aliases() {
    let mut ops = base();
    ops.extend([
        intrinsic(8, Intrinsic::LayerMerge, &[4, 7]),
        intrinsic(9, Intrinsic::LayerMerge, &[8, 8]),
        intrinsic(10, Intrinsic::LayerMerge, &[9, 4]),
        intrinsic(11, Intrinsic::LayerMerge, &[10, 8]),
        bind(
            12,
            Operation::Closure {
                code: 1,
                captures: vec![],
            },
        ),
        intrinsic(13, Intrinsic::LayerRun, &[11, 12]),
    ]);
    check(&program(ops, 13, callback()), &Ok(42), 200);
}

#[test]
fn invalid_merge_operand_preserves_other_owned_inputs_and_aliases() {
    for bad_left in [false, true] {
        let mut ops = base();
        ops.push(intrinsic(
            8,
            Intrinsic::LayerMerge,
            if bad_left { &[0, 4] } else { &[4, 0] },
        ));
        check(
            &program(ops, 8, callback()),
            &Err(Error::ExpectedLayer),
            100,
        );
    }
}

#[test]
fn right_provider_is_complete_not_a_missing_method_fallback() {
    let mut ops = base();
    ops.extend([
        intrinsic(8, Intrinsic::LayerSucceed, &[1, 0]),
        intrinsic(9, Intrinsic::LayerMerge, &[4, 8]),
        bind(
            10,
            Operation::Closure {
                code: 1,
                captures: vec![],
            },
        ),
        intrinsic(11, Intrinsic::LayerRun, &[9, 10]),
    ]);
    check(
        &program(ops, 11, callback()),
        &Err(Error::ExpectedRecord),
        200,
    );
}

#[test]
fn shared_dag_missing_service_traversal_is_fuel_metered() {
    let mut ops = base();
    let mut last = 4;
    // 2^30 paths, but only thirty additional graph nodes: lookup must not
    // recursively overflow or perform unbounded work outside the fuel budget.
    for next in 8..38 {
        ops.push(intrinsic(next, Intrinsic::LayerMerge, &[last, last]));
        last = next;
    }
    ops.extend([
        bind(
            38,
            Operation::Closure {
                code: 1,
                captures: vec![],
            },
        ),
        intrinsic(39, Intrinsic::LayerRun, &[last, 38]),
    ]);
    let p = program(
        ops,
        39,
        function(&[0, 1], vec![bind(2, perform("Missing", "read"))], 2),
    );
    let mut heap = Heap::new();
    let outcome = execute_program_outcome(&mut heap, &p, &entry(), vec![Value::Unit], 1000).outcome;
    {
        let diagnostic = format!("{outcome:?}");
        let observed_outcome = outcome;
        let correct = matches!(&observed_outcome, ProgramOutcome::Fault(Error::OutOfFuel));
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    let stats = heap.finish().expect("no actual objects remain");
    assert_eq!(stats.allocations, stats.frees);
}

fn run_owner_program() -> Program {
    let mut ops = vec![
        bind(1, Operation::Literal(Lit::Unit)),
        intrinsic(2, Intrinsic::ScopeCurrent, &[1]),
        bind(
            3,
            Operation::Closure {
                code: 3,
                captures: vec![],
            },
        ),
        bind(
            4,
            Operation::Record {
                fields: vec![("owner".into(), 3)],
            },
        ),
        bind(5, Operation::ServiceTag("Left".into())),
        intrinsic(6, Intrinsic::LayerSucceed, &[5, 4]),
        bind(7, Operation::ServiceTag("Right".into())),
        intrinsic(8, Intrinsic::LayerSucceed, &[7, 4]),
        intrinsic(9, Intrinsic::LayerMerge, &[6, 8]),
        bind(
            10,
            Operation::Closure {
                code: 1,
                captures: vec![],
            },
        ),
        intrinsic(11, Intrinsic::LayerRun, &[9, 10]),
    ];
    ops.push(bind(
        12,
        Operation::Construct {
            tag: "$tuple".into(),
            fields: vec![2, 11],
        },
    ));
    let perform_owner = |effect: &str| Operation::Perform {
        effect: effect.into(),
        op: "owner".into(),
        dicts: vec![],
        args: vec![1],
    };
    let callback = function(
        &[0, 1],
        vec![
            intrinsic(2, Intrinsic::ScopeCurrent, &[1]),
            bind(3, perform_owner("Left")),
            bind(4, perform_owner("Right")),
            bind(
                5,
                Operation::Construct {
                    tag: "$tuple".into(),
                    fields: vec![2, 3, 4],
                },
            ),
        ],
        5,
    );
    let mut p = program(ops, 12, callback);
    p.functions.insert(
        3,
        function(
            &[0, 1],
            vec![intrinsic(2, Intrinsic::ScopeCurrent, &[1])],
            2,
        ),
    );
    p
}

#[test]
fn both_branches_observe_the_same_run_owner_not_nested_provider_owners() {
    let p = run_owner_program();
    let mut reached = false;
    for fuel in 0..=250 {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("external".into()))
            .expect("allocate external alias root");
        let arg = heap.dup(&external).expect("retain caller alias");
        match execute_program_outcome(&mut heap, &p, &entry(), vec![arg], fuel).outcome {
            ProgramOutcome::Returned(value) => {
                let Value::Owner(outer) = heap.field(&value, 0).expect("owned first field") else {
                    panic!("caller owner")
                };
                let owners = heap.field(&value, 1).expect("provider owner tuple");
                let Value::Owner(run) = heap.field(owners, 0).expect("run owner field") else {
                    panic!("run owner")
                };
                let Value::Owner(left) = heap.field(owners, 1).expect("left branch owner field")
                else {
                    panic!("left owner")
                };
                let Value::Owner(right) = heap.field(owners, 2).expect("right branch owner field")
                else {
                    panic!("right owner")
                };
                assert_ne!(outer, run);
                assert_eq!(run, left);
                assert_eq!(run, right);
                heap.drop_value(value).expect("release owned outcome");
                reached = true;
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            other => {
                let diagnostic = format!("fuel {fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external)
            .expect("release external alias root");
        heap.finish().expect("no actual objects remain");
    }
    assert!(reached);
}

#[test]
fn construction_is_inert_and_deep_shared_graph_destruction_is_iterative() {
    let mut ops = vec![
        bind(0, Operation::ServiceTag("Read".into())),
        bind(1, Operation::Literal(Lit::Int(42))),
        intrinsic(2, Intrinsic::LayerSucceed, &[0, 1]),
    ];
    let mut last = 2;
    for next in 3..20_003 {
        ops.push(Instruction::Dup(last));
        ops.push(intrinsic(next, Intrinsic::LayerMerge, &[last, last]));
        last = next;
    }
    let p = Program {
        functions: [(
            0,
            Function {
                params: vec![],
                body: Block {
                    instructions: ops,
                    exit: Exit::Return(last),
                },
            },
        )]
        .into(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    };
    let mut heap = Heap::new();
    let value = match execute_program_outcome(&mut heap, &p, &entry(), vec![], 100_000).outcome {
        ProgramOutcome::Returned(value) => value,
        other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
    };
    assert_eq!(heap.stats().arenas_allocated, 0);
    let alias = heap.dup(&value).expect("retain shared graph alias");
    heap.drop_value(value).expect("release owned outcome");
    assert_eq!(heap.stats().live_objects, 20_002);
    heap.drop_value(alias).expect("release final graph alias");
    let stats = heap.finish().expect("no actual objects remain");
    assert_eq!(stats.allocations, stats.frees);
}
