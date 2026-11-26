#[test]
fn root_arena_detaches_self_and_mutual_closure_cell_cycles() {
    for mutual in [false, true] {
        let mut heap = Heap::new();
        let ran = run(
            &mut heap,
            &cyclic_program(mutual, false, false),
            vec![Value::Unit],
            500,
        )
        .expect("cyclic cells close");
        assert_eq!(ran.value, Value::Unit);
        let count = if mutual { 2 } else { 1 };
        assert_eq!(heap.stats().cells_allocated, count);
        assert_eq!(heap.stats().cells_freed, count);
        heap.drop_value(ran.value).expect("outcome");
        assert_eq!(
            heap.finish()
                .expect("no roots or traced collection")
                .live_objects,
            0
        );
    }
}

#[test]
fn cycle_cleanup_on_runtime_error_and_fuel_does_not_collect_external_aliases() {
    for fuel_failure in [false, true] {
        let mut heap = Heap::new();
        let argument = heap
            .literal(&Lit::String("external".into()))
            .expect("argument");
        let external = heap.dup(&argument).expect("external root");
        let error = run(
            &mut heap,
            &cyclic_program(true, !fuel_failure, fuel_failure),
            vec![argument],
            200,
        )
        .expect_err("failure");
        assert_eq!(
            error,
            if fuel_failure {
                Error::OutOfFuel
            } else {
                Error::ExpectedBoolean
            }
        );
        assert_eq!(heap.stats().cells_allocated, 2);
        assert_eq!(heap.stats().cells_freed, 2);
        assert_eq!(heap.reference_count(&external), Ok(1));
        assert_eq!(heap.string(&external), Ok("external"));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external).expect("external");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn modify_bad_result_or_fuel_cleans_callback_roots_without_committing() {
    for fuel_failure in [false, true] {
        let callback = if fuel_failure {
            function(
                &[0, 1],
                vec![bind(
                    2,
                    Operation::Apply {
                        function: 0,
                        argument: 1,
                    },
                )],
                2,
            )
        } else {
            function(
                &[0, 1],
                vec![bind(
                    2,
                    Operation::Literal(Lit::String("not a pair".into())),
                )],
                2,
            )
        };
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
                ],
                4,
            ),
            callback,
        ]);
        let mut heap = Heap::new();
        let initial = heap
            .literal(&Lit::String("original".into()))
            .expect("initial");
        let alias = heap.dup(&initial).expect("external");
        let error = run(&mut heap, &p, vec![initial], 100).expect_err("callback cannot complete");
        assert_eq!(
            error,
            if fuel_failure {
                Error::OutOfFuel
            } else {
                Error::ExpectedModifyPair
            }
        );
        assert_eq!(heap.stats().cells_allocated, 1);
        assert_eq!(heap.stats().cells_freed, 1);
        assert_eq!(heap.reference_count(&alias), Ok(1));
        assert_eq!(heap.string(&alias), Ok("original"));
        heap.drop_value(alias).expect("external");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn cell_identity_is_stable_and_distinguishes_allocations() {
    let p = program(vec![function(
        &[0, 1],
        vec![
            intrinsic(2, Intrinsic::RefMake, &[1]),
            intrinsic(3, Intrinsic::RefMake, &[1]),
            intrinsic(4, Intrinsic::RefSame, &[2, 2]),
            intrinsic(5, Intrinsic::RefSame, &[2, 3]),
            bind(
                6,
                Operation::Construct {
                    tag: "$tuple".into(),
                    fields: vec![4, 5],
                },
            ),
        ],
        6,
    )]);
    let mut heap = Heap::new();
    let ran = run(&mut heap, &p, vec![Value::Int(3)], 100).expect("cell identities");
    assert_eq!(heap.field(&ran.value, 0), Ok(&Value::Bool(true)));
    assert_eq!(heap.field(&ran.value, 1), Ok(&Value::Bool(false)));
    assert_eq!(heap.stats().cells_allocated, 2);
    assert_eq!(heap.stats().cells_freed, 2);
    heap.drop_value(ran.value).expect("outcome");
    assert!(heap.finish().is_ok());
}

#[test]
fn returned_closed_cell_capability_drops_safely_and_cannot_be_dereferenced_later() {
    let make = program(vec![function(
        &[0, 1],
        vec![intrinsic(2, Intrinsic::RefMake, &[1])],
        2,
    )]);
    let read = program(vec![function(
        &[0, 1],
        vec![intrinsic(2, Intrinsic::RefGet, &[1])],
        2,
    )]);
    let mut heap = Heap::new();
    let value = heap
        .literal(&Lit::String("cell contents".into()))
        .expect("contents");
    let ran = run(&mut heap, &make, vec![value], 100).expect("cell handle result");
    let id = ran.value.cell_id().expect("cell storage identity");
    assert!(
        !heap.is_live(id),
        "storage is reclaimed before the retained handle is dropped"
    );
    assert!(heap.finish().is_ok());
    assert_eq!(
        run(&mut heap, &read, vec![ran.value], 100).expect_err("closed arena"),
        Error::ForeignCell
    );
    assert!(heap.finish().is_ok());
    let ran = run(&mut heap, &make, vec![Value::Int(4)], 100).expect("another retained handle");
    heap.drop_value(ran.value)
        .expect("closed handle release does not dereference cell");
    assert!(heap.finish().is_ok());
}

#[test]
fn returned_task_cannot_rebind_code_or_closed_cell_storage_in_another_execution() {
    let make = program(vec![
        function(
            &[0, 1],
            vec![
                intrinsic(2, Intrinsic::RefMake, &[1]),
                bind(
                    3,
                    Operation::Task {
                        code: 2,
                        captures: vec![2],
                    },
                ),
            ],
            3,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                intrinsic(3, Intrinsic::RefGet, &[2]),
            ],
            3,
        ),
    ]);
    let awaiter = program(vec![function(
        &[0, 1],
        vec![bind(2, Operation::Await(1))],
        2,
    )]);
    let mut heap = Heap::new();
    let ran = run(&mut heap, &make, vec![Value::Int(8)], 100).expect("returned task");
    let cell_id = heap
        .field(&ran.value, 0)
        .expect("task capture")
        .cell_id()
        .expect("cell id");
    assert!(!heap.is_live(cell_id));
    assert_eq!(
        heap.stats().live_objects,
        1,
        "only task remains, not its closed cell"
    );
    assert_eq!(
        run(&mut heap, &awaiter, vec![ran.value], 100).expect_err("foreign task"),
        Error::ForeignTask
    );
    assert!(heap.finish().is_ok());
}

#[test]
fn main_function_returned_task_uses_same_live_cell_arena_until_await_finishes() {
    use flick_vm::tier0::execute_main_program;
    let p = program(vec![
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Literal(Lit::String("live until await".into())),
                ),
                intrinsic(3, Intrinsic::RefMake, &[2]),
                bind(
                    4,
                    Operation::Task {
                        code: 2,
                        captures: vec![3],
                    },
                ),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                intrinsic(3, Intrinsic::RefGet, &[2]),
                intrinsic(4, Intrinsic::PrintLine, &[3]),
            ],
            4,
        ),
    ]);
    let mut heap = Heap::new();
    let ran = execute_main_program(&mut heap, &p, &Name::new("Main", "main"), 200)
        .expect("main function and task share execution");
    assert_eq!(ran.stdout, "live until await\n");
    assert_eq!(ran.value, Value::Unit);
    assert_eq!(heap.stats().cells_allocated, 1);
    assert_eq!(heap.stats().cells_freed, 1);
    heap.drop_value(ran.value).expect("outcome");
    assert!(heap.finish().is_ok());
}

#[test]
fn direct_task_main_is_awaited_once() {
    use flick_vm::tier0::execute_main_program;
    let p = Program {
        functions: BTreeMap::from([
            (
                0,
                function(
                    &[],
                    vec![bind(
                        0,
                        Operation::Task {
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
                    &[0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::String("task main".into()))),
                        intrinsic(3, Intrinsic::PrintLine, &[2]),
                    ],
                    3,
                ),
            ),
        ]),
        globals: vec![Global {
            name: Name::new("Main", "main"),
            initializer: 0,
        }],
    };
    let mut heap = Heap::new();
    let ran = execute_main_program(&mut heap, &p, &Name::new("Main", "main"), 100)
        .expect("direct task main");
    assert_eq!(ran.stdout, "task main\n");
    assert_eq!(ran.value, Value::Unit);
    heap.drop_value(ran.value).expect("outcome");
    assert!(heap.finish().is_ok());
}

#[test]
fn task_storage_cannot_be_reused_as_a_tagged_constructor() {
    let p = program(vec![
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Task {
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
                },
            )],
            2,
        ),
    ]);
    let mut heap = Heap::new();
    let argument = heap
        .literal(&Lit::String("capture".into()))
        .expect("argument");
    let task = run(&mut heap, &p, vec![argument], 100)
        .expect("task result")
        .value;
    let id = task.object_id().expect("task allocation");
    let replacement = heap
        .reuse(task, "Replacement", vec![Value::Unit])
        .expect("fallback allocation");
    assert_ne!(replacement.object_id(), Some(id));
    assert_eq!(heap.tag(&replacement), Ok("Replacement"));
    assert_eq!(heap.stats().reuses, 0);
    assert_eq!(heap.stats().live_objects, 1);
    heap.drop_value(replacement).expect("replacement");
    assert!(heap.finish().is_ok());
}

#[test]
fn modify_callback_type_error_cleans_retained_old_contents_and_callback_capture() {
    let p = program(vec![
        function(
            &[0, 1],
            vec![
                intrinsic(2, Intrinsic::RefMake, &[1]),
                bind(
                    3,
                    Operation::Closure {
                        code: 2,
                        captures: vec![1],
                    },
                ),
                intrinsic(4, Intrinsic::RefModify, &[2, 3]),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                bind(3, Operation::Literal(Lit::Bool(true))),
                bind(
                    4,
                    Operation::Binary {
                        op: BinOp::Concat,
                        left: 2,
                        right: 3,
                    },
                ),
            ],
            4,
        ),
    ]);
    let mut heap = Heap::new();
    let value = heap
        .literal(&Lit::String("original".into()))
        .expect("original");
    let external = heap.dup(&value).expect("external root");
    assert_eq!(
        run(&mut heap, &p, vec![value], 200).expect_err("callback type error"),
        Error::ExpectedString
    );
    assert_eq!(heap.stats().cells_allocated, 1);
    assert_eq!(heap.stats().cells_freed, 1);
    assert_eq!(heap.reference_count(&external), Ok(1));
    assert_eq!(heap.stats().live_objects, 1);
    heap.drop_value(external).expect("external");
    assert!(heap.finish().is_ok());
}

#[test]
fn reusing_a_cell_read_falls_back_and_preserves_the_cells_old_value() {
    let p = program(vec![Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                Instruction::Drop(0),
                intrinsic(2, Intrinsic::RefMake, &[1]),
                Instruction::Dup(2),
                intrinsic(3, Intrinsic::RefGet, &[2]),
                bind(4, Operation::Literal(Lit::Int(9))),
                bind(
                    5,
                    Operation::Reuse {
                        object: 3,
                        tag: "New".into(),
                        fields: vec![4],
                    },
                ),
                intrinsic(6, Intrinsic::RefGet, &[2]),
                bind(
                    7,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![5, 6],
                    },
                ),
            ],
            exit: Exit::Return(7),
        },
    }]);
    let mut heap = Heap::new();
    let old = heap
        .construct("Old", vec![Value::Int(1)])
        .expect("old node");
    let old_id = old.object_id().expect("old id");
    let ran = run(&mut heap, &p, vec![old], 200).expect("cell read and reuse");
    let changed = heap.field(&ran.value, 0).expect("new node");
    let original = heap.field(&ran.value, 1).expect("cell value");
    assert_eq!(heap.tag(changed), Ok("New"));
    assert_eq!(heap.field(changed, 0), Ok(&Value::Int(9)));
    assert_eq!(original.object_id(), Some(old_id));
    assert_eq!(heap.tag(original), Ok("Old"));
    assert_eq!(heap.field(original, 0), Ok(&Value::Int(1)));
    assert_eq!(
        heap.stats().reuses,
        0,
        "the slot is an independent strong owner even when the read local dies"
    );
    assert_eq!(heap.stats().cells_freed, 1);
    heap.drop_value(ran.value).expect("result graph");
    assert!(heap.finish().is_ok());
}
