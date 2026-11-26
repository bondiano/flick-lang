use flick_core::ir::memory::ValueRep;
#[test]
fn curried_closure_moves_its_capture_without_self_owning_edges() {
    let p = program(vec![
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
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Closure {
                    code: 2,
                    captures: vec![1],
                },
            )],
            2,
        ),
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            )],
            2,
        ),
    ]);
    let mut heap = Heap::new();
    let held = heap.literal(&Lit::String("held".into())).expect("string");
    let ran = execute_program(
        &mut heap,
        &p,
        &Name::new("Main", "main"),
        vec![held, Value::Unit],
        100,
    )
    .expect("curried call");
    assert_eq!(heap.string(&ran.value), Ok("held"));
    assert_eq!(heap.stats().live_objects, 1);
    heap.drop_value(ran.value).expect("caller result");
    assert!(heap.finish().is_ok());
}

#[test]
fn runtime_type_errors_release_taken_operands_and_suspended_frames() {
    let p = program(vec![
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
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::Bool(true))),
                bind(
                    3,
                    Operation::Binary {
                        op: BinOp::Concat,
                        left: 1,
                        right: 2,
                    },
                ),
            ],
            3,
        ),
    ]);
    let mut heap = Heap::new();
    let argument = heap
        .literal(&Lit::String("must release".into()))
        .expect("argument");
    assert!(
        execute_program(
            &mut heap,
            &p,
            &Name::new("Main", "main"),
            vec![argument],
            100
        )
        .is_err()
    );
    assert!(
        heap.finish().is_ok(),
        "taken operands are still roots until consumed"
    );
}

#[test]
fn global_initializer_cycles_fail_without_leaking_execution_roots() {
    let p = Program {
        functions: BTreeMap::from([
            (
                0,
                function(
                    &[],
                    vec![bind(0, Operation::Global(Name::new("Main", "b")))],
                    0,
                ),
            ),
            (
                1,
                function(
                    &[],
                    vec![bind(0, Operation::Global(Name::new("Main", "main")))],
                    0,
                ),
            ),
        ]),
        globals: vec![
            Global {
                name: Name::new("Main", "main"),
                initializer: 0,
            },
            Global {
                name: Name::new("Main", "b"),
                initializer: 1,
            },
        ],
    };
    let mut heap = Heap::new();
    assert!(matches!(
        execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 100),
        Err(Error::GlobalCycle(_))
    ));
    assert!(heap.finish().is_ok());
}

#[test]
fn captured_value_survives_two_calls_and_is_released_with_the_result() {
    let p = program(vec![
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
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::String("twice".into()))),
                bind(
                    3,
                    Operation::Closure {
                        code: 2,
                        captures: vec![2],
                    },
                ),
                bind(
                    4,
                    Operation::Apply {
                        function: 3,
                        argument: 1,
                    },
                ),
                bind(5, Operation::Literal(Lit::Unit)),
                bind(
                    6,
                    Operation::Apply {
                        function: 3,
                        argument: 5,
                    },
                ),
                bind(
                    7,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![4, 6],
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                ),
            ],
            7,
        ),
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            )],
            2,
        ),
    ]);
    let mut heap = Heap::new();
    let ran = execute_program(
        &mut heap,
        &p,
        &Name::new("Main", "main"),
        vec![Value::Unit],
        100,
    )
    .expect("two calls");
    assert_eq!(
        heap.string(heap.field(&ran.value, 0).expect("first")),
        Ok("twice")
    );
    assert_eq!(
        heap.string(heap.field(&ran.value, 1).expect("second")),
        Ok("twice")
    );
    assert_eq!(
        heap.reference_count(heap.field(&ran.value, 0).expect("shared")),
        Ok(2)
    );
    assert_eq!(heap.stats().live_objects, 2);
    heap.drop_value(ran.value).expect("tuple result");
    assert!(heap.finish().is_ok());
}

#[test]
fn self_recursion_with_a_capture_uses_an_iterative_call_stack() {
    use flick_core::ir::BinOp;
    use flick_core::numeric::{Arith, Num};
    let source = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                ),
                bind(3, Operation::Literal(Lit::Int(0))),
                bind(
                    4,
                    Operation::Binary {
                        op: BinOp::Eq,
                        left: 1,
                        right: 3,
                    },
                ),
            ],
            exit: Exit::Branch {
                condition: 4,
                then_: Box::new(Block {
                    instructions: vec![],
                    exit: Exit::Return(2),
                }),
                else_: Box::new(Block {
                    instructions: vec![
                        bind(5, Operation::Literal(Lit::Int(1))),
                        bind(
                            6,
                            Operation::Binary {
                                op: BinOp::Arith(Arith::Sub, Num::Int),
                                left: 1,
                                right: 5,
                            },
                        ),
                        bind(
                            7,
                            Operation::Apply {
                                function: 0,
                                argument: 6,
                            },
                        ),
                    ],
                    exit: Exit::Return(7),
                }),
            },
        },
    };
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::String("base".into()))),
                bind(
                    1,
                    Operation::Closure {
                        code: 1,
                        captures: vec![0],
                    },
                ),
            ],
            1,
        ),
        perceus::insert(&source).expect("recursive ownership"),
    ]);
    let mut heap = Heap::new();
    let ran = execute_program(
        &mut heap,
        &p,
        &Name::new("Main", "main"),
        vec![Value::Int(20_000)],
        500_000,
    )
    .expect("deep counted recursion");
    assert_eq!(heap.string(&ran.value), Ok("base"));
    assert_eq!(
        heap.stats().allocations,
        2,
        "recursive self is not a newly allocated closure per call"
    );
    assert_eq!(heap.stats().live_objects, 1);
    heap.drop_value(ran.value).expect("base result");
    assert!(heap.finish().is_ok());
}

#[test]
fn direct_branch_helpers_run_only_the_selected_body() {
    for condition in [false, true] {
        let helper = perceus::insert(&Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![],
                exit: Exit::Branch {
                    condition: 0,
                    then_: Box::new(Block {
                        instructions: vec![],
                        exit: Exit::Return(1),
                    }),
                    else_: Box::new(Block {
                        instructions: vec![bind(2, Operation::Literal(Lit::String("no".into())))],
                        exit: Exit::Return(2),
                    }),
                },
            },
        })
        .expect("helper ownership");
        let p = program(vec![
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
            function(
                &[0, 1],
                vec![
                    bind(2, Operation::Literal(Lit::String("yes".into()))),
                    bind(
                        3,
                        Operation::Call {
                            code: 2,
                            args: vec![1, 2],
                        },
                    ),
                ],
                3,
            ),
            helper,
        ]);
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &p,
            &Name::new("Main", "main"),
            vec![Value::Bool(condition)],
            100,
        )
        .expect("helper call");
        assert_eq!(
            heap.string(&ran.value),
            Ok(if condition { "yes" } else { "no" })
        );
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(ran.value).expect("helper result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn globals_are_initialized_once_and_their_cache_is_explicitly_released() {
    use flick_core::ir::memory::Intrinsic;
    let mut p = program(vec![
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
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Global(Name::new("Main", "shared"))),
                bind(3, Operation::Global(Name::new("Main", "shared"))),
                bind(
                    4,
                    Operation::Record {
                        fields: vec![("left".into(), 2), ("right".into(), 3)],
                    },
                ),
            ],
            4,
        ),
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::String("initialize".into()))),
                bind(
                    1,
                    Operation::Intrinsic {
                        kind: Intrinsic::PrintLine,
                        args: vec![0],
                    },
                ),
                bind(2, Operation::Literal(Lit::String("shared".into()))),
            ],
            2,
        ),
    ]);
    p.globals.push(Global {
        name: Name::new("Main", "shared"),
        initializer: 2,
    });
    let mut heap = Heap::new();
    let ran = execute_program(
        &mut heap,
        &p,
        &Name::new("Main", "main"),
        vec![Value::Unit],
        100,
    )
    .expect("cached globals");
    assert_eq!(ran.stdout, "initialize\n");
    let left = heap.named_field(&ran.value, "left").expect("left");
    assert_eq!(heap.string(left), Ok("shared"));
    assert_eq!(
        heap.reference_count(left),
        Ok(2),
        "only the two record edges remain after cache close"
    );
    heap.drop_value(ran.value).expect("record result");
    assert!(heap.finish().is_ok());
}
