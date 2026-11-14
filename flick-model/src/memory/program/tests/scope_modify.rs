fn unexpected_outcome(heap: &mut Heap, outcome: ProgramOutcome, context: &str) -> ! {
    let diagnostic = format!("{context}: {outcome:?}");
    outcome
        .release(heap)
        .expect("unexpected owned outcome explicitly released");
    panic!("{diagnostic}")
}

fn function(params: Vec<Local>, instructions: Vec<Instruction>, result: Local) -> memory::Function {
    perceus::insert(&memory::Function {
        params,
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("valid scope-boundary test operation")
}

#[test]
fn scope_reclaims_storage_before_resuming_the_live_caller() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let intrinsic = |binding, kind, args| bind(binding, Operation::Intrinsic { kind, args });
    let program = Program {
        functions: BTreeMap::from([
            (
                0,
                function(
                    vec![],
                    vec![
                        bind(
                            0,
                            Operation::Closure {
                                code: 1,
                                captures: vec![],
                            },
                        ),
                        intrinsic(1, Intrinsic::ScopeRun, vec![0]),
                        bind(2, Operation::Literal(Lit::Unit)),
                    ],
                    2,
                ),
            ),
            (
                1,
                function(
                    vec![0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::Unit)),
                        intrinsic(3, Intrinsic::ArenaMake, vec![2]),
                        bind(4, Operation::Literal(Lit::String("scope-owned".into()))),
                        intrinsic(5, Intrinsic::ArenaAlloc, vec![3, 4]),
                    ],
                    5,
                ),
            ),
        ]),
        globals: vec![memory::Global {
            name: Name::new("Main", "main"),
            initializer: 0,
        }],
    };
    memory::validate_program(&program).expect("valid scope-boundary test operation");
    let mut heap = Heap::new();
    let mut machine = fixture_machine(&program, &mut heap);
    machine.fuel = 100;
    machine
        .global(&Name::new("Main", "main"), None)
        .expect("valid scope-boundary test operation");
    for _ in 0..100 {
        machine.step().expect("valid scope-boundary test operation");
        if machine.owners.get(1).is_some_and(|owner| owner.closed) {
            break;
        }
    }
    assert!(machine.owners[1].closed);
    assert_eq!(machine.frames.len(), 1, "caller has not returned");
    assert!(
        machine.globals[&Name::new("Main", "main")].is_none(),
        "entry is still initializing"
    );
    assert_eq!(machine.heap.stats().arenas_freed, 1);
    assert_eq!(
        machine.heap.stats().live_objects,
        0,
        "arena, slot string and callback already freed before caller proceeds"
    );
    machine
        .cleanup()
        .expect("valid scope-boundary test operation");
    machine
        .heap
        .finish()
        .expect("valid scope-boundary test operation");
}
#[test]
fn modify_rejects_a_reserved_tuple_evidence_suffix_before_committing() {
    let function = |params, instructions, result| {
        perceus::insert(&memory::Function {
            params,
            body: Block {
                instructions,
                exit: Exit::Return(result),
            },
        })
        .expect("valid modify test ownership")
    };
    let bind = |binding, value| Instruction::Let { binding, value };
    let program = Program {
        functions: BTreeMap::from([
            (
                0,
                function(
                    vec![],
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
            (
                1,
                function(
                    vec![0, 1],
                    vec![
                        bind(
                            2,
                            Operation::Intrinsic {
                                kind: Intrinsic::RefMake,
                                args: vec![1],
                            },
                        ),
                        bind(
                            3,
                            Operation::Closure {
                                code: 2,
                                captures: vec![],
                            },
                        ),
                        bind(
                            4,
                            Operation::Intrinsic {
                                kind: Intrinsic::RefModify,
                                args: vec![2, 3],
                            },
                        ),
                    ],
                    4,
                ),
            ),
            (2, function(vec![0, 1], vec![], 1)),
        ]),
        globals: vec![memory::Global {
            name: Name::new("Main", "main"),
            initializer: 0,
        }],
    };
    let mut heap = Heap::new();
    let hidden = heap
        .literal(&Lit::String("owned tuple evidence".into()))
        .expect("evidence");
    let argument = heap.allocate(Data::Tagged {
        tag: "$tuple".into(),
        ordinary_fields: 1,
        fields: vec![Value::Int(7), hidden],
    });
    assert_eq!(
        execute_program(
            &mut heap,
            &program,
            &Name::new("Main", "main"),
            vec![argument],
            1000
        )
        .expect_err("malformed callback pair"),
        Error::ReservedConstructorEvidence("$tuple".into())
    );
    heap.finish()
        .expect("rejected callback pair and actual cell roots all release");
}
