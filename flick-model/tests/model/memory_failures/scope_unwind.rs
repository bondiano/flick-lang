#[test]
fn failed_modify_does_not_commit_and_catch_can_read_original_cell() {
    let mut p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::String("original".into()))),
                intrinsic(1, Intrinsic::RefMake, &[0]),
                bind(
                    2,
                    Operation::Closure {
                        code: 1,
                        captures: vec![1],
                    },
                ),
                bind(
                    3,
                    Operation::Closure {
                        code: 3,
                        captures: vec![],
                    },
                ),
                intrinsic(4, Intrinsic::Try, &[2, 3]),
                intrinsic(5, Intrinsic::RefGet, &[1]),
            ],
            5,
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
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::String("failed".into()))),
                intrinsic(3, Intrinsic::Raise, &[2]),
            ],
            3,
        ),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Unit))], 2),
    ]);
    recovery::seal_bodies(&mut p, &[3]);
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                assert_eq!(heap.string(&value), Ok("original"));
                heap.drop_value(value).expect("valid typed-failure test");
            }
            ProgramOutcome::Fault(e) => assert_eq!(e, Error::OutOfFuel),
            other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
        }
        heap.finish().expect("valid typed-failure test");
    }
}

#[test]
fn caught_initializer_failure_removes_the_in_progress_marker_for_retry() {
    let retry = Name::new("Main", "retry");
    let mut p = program(vec![
        function(
            &[],
            vec![
                bind(
                    0,
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                ),
                bind(
                    1,
                    Operation::Closure {
                        code: 2,
                        captures: vec![],
                    },
                ),
                intrinsic(2, Intrinsic::Try, &[0, 1]),
                intrinsic(3, Intrinsic::Try, &[0, 1]),
            ],
            3,
        ),
        function(&[0, 1], vec![bind(2, Operation::Global(retry.clone()))], 2),
        function(&[0, 1], vec![], 1),
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::String("retry".into()))),
                intrinsic(1, Intrinsic::PrintLine, &[0]),
                intrinsic(2, Intrinsic::Raise, &[0]),
            ],
            2,
        ),
    ]);
    recovery::seal_bodies(&mut p, &[2]);
    p.globals.push(Global {
        name: retry,
        initializer: 3,
    });
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000);
    assert_eq!(report.stdout, "retry\nretry\n");
    let ProgramOutcome::Returned(value) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    assert_eq!(heap.string(&value), Ok("retry"));
    heap.drop_value(value).expect("valid typed-failure test");
    heap.finish().expect("valid typed-failure test");
}

#[test]
fn unwinding_scope_closes_its_arena_before_dispatching_outer_catch() {
    let mut p = program(vec![
        function(
            &[],
            vec![
                bind(
                    0,
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                ),
                bind(
                    1,
                    Operation::Closure {
                        code: 3,
                        captures: vec![],
                    },
                ),
                intrinsic(2, Intrinsic::Try, &[0, 1]),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Closure {
                        code: 2,
                        captures: vec![],
                    },
                ),
                intrinsic(3, Intrinsic::ScopeRun, &[2]),
            ],
            3,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::Unit)),
                intrinsic(3, Intrinsic::ArenaMake, &[2]),
                bind(4, Operation::Literal(Lit::String("closed slot".into()))),
                intrinsic(5, Intrinsic::ArenaAlloc, &[3, 4]),
                intrinsic(6, Intrinsic::Raise, &[5]),
            ],
            6,
        ),
        function(&[0, 1], vec![intrinsic(2, Intrinsic::ArenaGet, &[1])], 2),
    ]);
    recovery::seal_bodies(&mut p, &[3]);
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel);
        match report.outcome {
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            ProgramOutcome::Failed(cause) => {
                assert_eq!(causes::defect_error(&mut heap, cause), Error::ClosedArena);
            }
            other => causes::unexpected(&mut heap, other, "exact arena Die or fuel fault"),
        }
        heap.finish().expect("valid typed-failure test");
    }
}

#[test]
fn selected_outer_owner_survives_inner_scope_failure() {
    let mut p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Unit)),
                intrinsic(1, Intrinsic::ArenaMake, &[0]),
                bind(2, Operation::Literal(Lit::String("outer survives".into()))),
                intrinsic(3, Intrinsic::ArenaAlloc, &[1, 2]),
                intrinsic(4, Intrinsic::ScopeCurrent, &[0]),
                bind(
                    5,
                    Operation::Closure {
                        code: 1,
                        captures: vec![4, 3],
                    },
                ),
                bind(
                    6,
                    Operation::Closure {
                        code: 4,
                        captures: vec![],
                    },
                ),
                intrinsic(7, Intrinsic::Try, &[5, 6]),
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
                    },
                ),
                bind(
                    3,
                    Operation::Project {
                        object: 0,
                        field: 1,
                    },
                ),
                bind(
                    4,
                    Operation::Closure {
                        code: 2,
                        captures: vec![2, 3],
                    },
                ),
                intrinsic(5, Intrinsic::ScopeRun, &[4]),
            ],
            5,
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
                bind(
                    3,
                    Operation::Project {
                        object: 0,
                        field: 1,
                    },
                ),
                bind(
                    4,
                    Operation::Within {
                        owner: 2,
                        code: 3,
                        args: vec![3],
                    },
                ),
            ],
            4,
        ),
        function(&[0], vec![intrinsic(1, Intrinsic::Raise, &[0])], 1),
        function(&[0, 1], vec![intrinsic(2, Intrinsic::ArenaGet, &[1])], 2),
    ]);
    recovery::seal_bodies(&mut p, &[4]);
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                assert_eq!(heap.string(&value), Ok("outer survives"));
                heap.drop_value(value).expect("valid typed-failure test");
            }
            ProgramOutcome::Fault(e) => assert_eq!(e, Error::OutOfFuel),
            other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
        }
        heap.finish().expect("valid typed-failure test");
    }
}

#[test]
fn standalone_executor_requires_program_for_typed_failure_intrinsics() {
    for kind in [Intrinsic::Try, Intrinsic::FromResult, Intrinsic::Raise] {
        let f = function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Unit)),
                intrinsic(1, kind, &vec![0; kind.arity()]),
            ],
            1,
        );
        let mut heap = Heap::new();
        assert_eq!(execute(&mut heap, &f, vec![]), Err(Error::NeedsProgram));
        heap.finish().expect("valid typed-failure test");
    }
}

#[test]
fn standalone_rejection_preserves_its_existing_leak_diagnostic_boundary() {
    // Unlike execute_program_outcome, the standalone annotation checker does
    // not unwind errors. Its existing diagnostic roots must not be swept.
    let f = function(
        &[],
        vec![
            bind(0, Operation::Literal(Lit::String("diagnostic root".into()))),
            intrinsic(1, Intrinsic::Raise, &[0]),
        ],
        1,
    );
    let mut heap = Heap::new();
    assert_eq!(execute(&mut heap, &f, vec![]), Err(Error::NeedsProgram));
    assert_eq!(heap.finish(), Err(Error::LiveObjects(1)));
}
