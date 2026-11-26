use flick_core::ir::memory::ValueRep;
#[test]
fn task_body_is_delayed_and_replayed_with_shared_capture() {
    let p = program(vec![
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Task {
                        code: 2,
                        captures: vec![1],
                    },
                ),
                bind(3, Operation::Literal(Lit::String("built".into()))),
                intrinsic(4, Intrinsic::PrintLine, &[3]),
                bind(5, Operation::Await(2)),
                bind(6, Operation::Await(2)),
                bind(
                    7,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![5, 6],
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                ),
            ],
            7,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                ),
                intrinsic(3, Intrinsic::PrintLine, &[2]),
            ],
            2,
        ),
    ]);
    let mut heap = Heap::new();
    let held = heap.literal(&Lit::String("body".into())).expect("capture");
    let ran = run(&mut heap, &p, vec![held], 200).expect("task replay");
    assert_eq!(ran.stdout, "built\nbody\nbody\n");
    assert_eq!(
        heap.string(heap.field(&ran.value, 0).expect("first")),
        Ok("body")
    );
    assert_eq!(
        heap.string(heap.field(&ran.value, 1).expect("second")),
        Ok("body")
    );
    assert_eq!(
        heap.reference_count(heap.field(&ran.value, 0).expect("first")),
        Ok(2)
    );
    heap.drop_value(ran.value).expect("result");
    assert!(heap.finish().is_ok());
}

#[test]
fn tasks_and_closures_have_distinct_invocation_contracts() {
    for await_closure in [false, true] {
        let produced = if await_closure {
            Operation::Closure {
                code: 2,
                captures: vec![],
            }
        } else {
            Operation::Task {
                code: 2,
                captures: vec![],
            }
        };
        let call = if await_closure {
            Operation::Await(2)
        } else {
            Operation::Apply {
                function: 2,
                argument: 1,
            }
        };
        let p = program(vec![
            function(&[0, 1], vec![bind(2, produced), bind(3, call)], 3),
            function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Unit))], 2),
        ]);
        let mut heap = Heap::new();
        let error = run(&mut heap, &p, vec![Value::Unit], 100).expect_err("wrong callable kind");
        if await_closure {
            assert_eq!(error, Error::ExpectedTask);
        } else {
            assert_eq!(error, Error::ExpectedClosure);
        }
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn replacing_cell_contents_releases_the_old_owned_value() {
    let p = program(vec![function(
        &[0, 1],
        vec![
            intrinsic(2, Intrinsic::RefMake, &[1]),
            bind(3, Operation::Literal(Lit::String("new".into()))),
            intrinsic(4, Intrinsic::RefSet, &[2, 3]),
            intrinsic(5, Intrinsic::RefGet, &[2]),
        ],
        5,
    )]);
    let mut heap = Heap::new();
    let old = heap.literal(&Lit::String("old".into())).expect("old");
    let old_id = old.object_id().expect("identity");
    let ran = run(&mut heap, &p, vec![old], 100).expect("cell set/get");
    assert_eq!(heap.string(&ran.value), Ok("new"));
    assert!(!heap.is_live(old_id), "replacement dropped old contents");
    assert_eq!(heap.stats().cells_allocated, 1);
    assert_eq!(heap.stats().cells_freed, 1);
    assert_eq!(heap.stats().live_objects, 1, "only returned string remains");
    heap.drop_value(ran.value).expect("result");
    assert!(heap.finish().is_ok());
}

#[test]
fn same_value_assignment_and_cell_reads_preserve_external_aliases() {
    let p = program(vec![function(
        &[0, 1],
        vec![
            intrinsic(2, Intrinsic::RefMake, &[1]),
            intrinsic(3, Intrinsic::RefGet, &[2]),
            intrinsic(4, Intrinsic::RefSet, &[2, 3]),
            intrinsic(5, Intrinsic::RefGet, &[2]),
        ],
        5,
    )]);
    let mut heap = Heap::new();
    let value = heap.literal(&Lit::String("shared".into())).expect("value");
    let external = heap.dup(&value).expect("external alias");
    let ran = run(&mut heap, &p, vec![value], 100).expect("same value set");
    assert_eq!(heap.reference_count(&external), Ok(2));
    assert_eq!(heap.string(&ran.value), Ok("shared"));
    heap.drop_value(ran.value).expect("result");
    assert_eq!(heap.reference_count(&external), Ok(1));
    heap.drop_value(external).expect("external");
    assert!(heap.finish().is_ok());
}

#[test]
fn modify_returns_first_tuple_field_and_stores_second() {
    let p = program(vec![
        function(
            &[0, 1],
            vec![
                intrinsic(2, Intrinsic::RefMake, &[1]),
                bind(
                    3,
                    Operation::Closure {
                        code: 2,
                        captures: vec![],
                    },
                ),
                intrinsic(4, Intrinsic::RefModify, &[2, 3]),
                intrinsic(5, Intrinsic::RefGet, &[2]),
                bind(
                    6,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![4, 5],
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                ),
            ],
            6,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::Int(10))),
                bind(
                    3,
                    Operation::Binary {
                        op: BinOp::Arith(Arith::Add, Num::Int),
                        left: 1,
                        right: 2,
                    },
                ),
                bind(
                    4,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![1, 3],
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                ),
            ],
            4,
        ),
    ]);
    let mut heap = Heap::new();
    let ran = run(&mut heap, &p, vec![Value::Int(3)], 200).expect("modify");
    assert_eq!(heap.field(&ran.value, 0), Ok(&Value::Int(3)));
    assert_eq!(heap.field(&ran.value, 1), Ok(&Value::Int(13)));
    heap.drop_value(ran.value).expect("result");
    assert!(heap.finish().is_ok());
}

fn cyclic_program(mutual: bool, failure: bool, endless: bool) -> Program {
    let mut body = vec![
        bind(
            2,
            Operation::Closure {
                code: 2,
                captures: vec![],
            },
        ),
        intrinsic(3, Intrinsic::RefMake, &[2]),
    ];
    if mutual {
        body.extend([
            intrinsic(4, Intrinsic::RefMake, &[2]),
            bind(
                5,
                Operation::Closure {
                    code: 3,
                    captures: vec![4],
                },
            ),
            intrinsic(6, Intrinsic::RefSet, &[3, 5]),
            bind(
                7,
                Operation::Closure {
                    code: 3,
                    captures: vec![3],
                },
            ),
            intrinsic(8, Intrinsic::RefSet, &[4, 7]),
        ]);
    } else {
        body.extend([
            bind(
                5,
                Operation::Closure {
                    code: 3,
                    captures: vec![3],
                },
            ),
            intrinsic(6, Intrinsic::RefSet, &[3, 5]),
        ]);
    }
    let result = if failure {
        body.extend([
            bind(9, Operation::Literal(Lit::String("not Bool".into()))),
            bind(10, Operation::Not(9)),
        ]);
        10
    } else if endless {
        body.push(bind(
            9,
            Operation::Call {
                code: 4,
                args: vec![],
            },
        ));
        9
    } else {
        body.push(bind(9, Operation::Literal(Lit::Unit)));
        9
    };
    program(vec![
        function(&[0, 1], body, result),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Unit))], 2),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                ),
                intrinsic(3, Intrinsic::RefGet, &[2]),
                bind(4, Operation::Literal(Lit::Unit)),
            ],
            4,
        ),
        function(
            &[],
            vec![bind(
                0,
                Operation::Call {
                    code: 4,
                    args: vec![],
                },
            )],
            0,
        ),
    ])
}
