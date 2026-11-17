#[test]
fn tail_arm_failure_discards_body_catches_and_closes_scope_before_outer_catch() {
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &tail_failure_scope(), &entry(), vec![], 2000);
    assert_eq!(report.stdout, "armfailed\n");
    causes::assert_defect(&mut heap, report.outcome, &Error::ClosedArena);
    assert_eq!(heap.stats().arenas_freed, 1);
    heap.finish().expect("all execution roots released");
}

fn nested_tail_abort() -> Program {
    let mut outer = handled(&[("left", 1)]);
    outer[0].operations[0].class = OpClass::Abort;
    program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 2),
                bind(
                    2,
                    Operation::Handle {
                        body: 0,
                        effects: outer,
                        ret: None,
                        errors: None,
                    },
                ),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![closure(2, 3), closure(3, 4), handle(4, 2, &[("right", 3)])],
            4,
        ),
        constant(20),
        function(
            &[0, 1],
            vec![closure(2, 5), intrinsic(3, Intrinsic::ScopeRun, &[2])],
            3,
        ),
        function(&[0, 1], vec![unit(2), perform(3, "left", &[2])], 3),
        function(
            &[0, 1],
            vec![
                unit(2),
                intrinsic(3, Intrinsic::ArenaMake, &[2]),
                bind(4, Operation::Literal(Lit::String("parked-inner".into()))),
                intrinsic(5, Intrinsic::ArenaAlloc, &[3, 4]),
                perform(6, "right", &[2]),
            ],
            6,
        ),
    ])
}
#[test]
fn outer_abort_retires_nested_tail_segments_and_invocation_queues_once() {
    run_int(&nested_tail_abort(), 20);
    for p in [nested_tail_abort(), tail_failure_scope()] {
        for fuel in 0..220 {
            let mut heap = Heap::new();
            let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel);
            match report.outcome {
                ProgramOutcome::Returned(value) | ProgramOutcome::Raised(value) => {
                    heap.drop_value(value)
                        .expect("valid handler test operation");
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                ProgramOutcome::Failed(cause) => {
                    assert_eq!(causes::defect_error(&mut heap, cause), Error::ClosedArena);
                }
                other @ ProgramOutcome::Fault(_) => {
                    let diagnostic = format!("fuel {fuel}: {other:?}");
                    causes::unexpected(&mut heap, other, &diagnostic)
                }
            }
            heap.finish()
                .unwrap_or_else(|e| panic!("fuel {fuel}: {e:?}"));
        }
    }
}

#[test]
fn abort_removes_an_abandoned_global_initializer_marker_before_retry() {
    let mut first = handled(&[("left", 1)]);
    first[0].operations[0].class = OpClass::Abort;
    let global = Name::new("Main", "retry");
    let mut p = program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 2),
                bind(
                    2,
                    Operation::Handle {
                        body: 0,
                        effects: first,
                        ret: None,
                        errors: None,
                    },
                ),
                closure(3, 1),
                closure(4, 3),
                handle(5, 3, &[("left", 4)]),
                add(6, 2, 5),
            ],
            6,
        ),
        function(&[0, 1], vec![bind(2, Operation::Global(global.clone()))], 2),
        constant(20),
        constant(7),
        function(&[], vec![unit(0), perform(1, "left", &[0])], 1),
    ]);
    p.globals.push(Global {
        name: global,
        initializer: 4,
    });
    run_int(&p, 27);
}

#[test]
fn abort_of_ref_modify_callback_does_not_commit_or_invent_a_pair() {
    let mut effects = handled(&[("left", 3)]);
    effects[0].operations[0].class = OpClass::Abort;
    let p = program(vec![
        function(
            &[],
            vec![
                int(0, 5),
                intrinsic(1, Intrinsic::RefMake, &[0]),
                capture(2, 1, &[1]),
                closure(3, 2),
                bind(
                    4,
                    Operation::Handle {
                        body: 2,
                        effects,
                        ret: None,
                        errors: None,
                    },
                ),
                intrinsic(5, Intrinsic::RefGet, &[1]),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![
                project(2, 0, 0),
                closure(3, 3),
                intrinsic(4, Intrinsic::RefModify, &[2, 3]),
            ],
            4,
        ),
        constant(20),
        function(&[0, 1], vec![unit(2), perform(3, "left", &[2])], 3),
    ]);
    run_int(&p, 5);
}

fn selected_owner_abort() -> Program {
    let mut effects = handled(&[("left", 4)]);
    effects[0].operations[0].class = OpClass::Abort;
    program(vec![
        function(
            &[],
            vec![closure(0, 1), intrinsic(1, Intrinsic::ScopeRun, &[0])],
            1,
        ),
        function(
            &[0, 1],
            vec![capture(2, 2, &[1]), intrinsic(3, Intrinsic::ScopeRun, &[2])],
            3,
        ),
        function(
            &[0, 1],
            vec![
                project(2, 0, 0),
                capture(3, 3, &[2]),
                closure(4, 6),
                bind(
                    5,
                    Operation::Handle {
                        body: 3,
                        effects,
                        ret: None,
                        errors: None,
                    },
                ),
                intrinsic(6, Intrinsic::ArenaGet, &[5]),
            ],
            6,
        ),
        function(
            &[0, 1],
            vec![
                project(2, 0, 0),
                bind(
                    3,
                    Operation::Within {
                        owner: 2,
                        code: 4,
                        args: vec![],
                    },
                ),
            ],
            3,
        ),
        function(
            &[],
            vec![
                unit(0),
                intrinsic(1, Intrinsic::ArenaMake, &[0]),
                bind(2, Operation::Literal(Lit::String("outer-owner".into()))),
                intrinsic(3, Intrinsic::ArenaAlloc, &[1, 2]),
                capture(4, 5, &[3]),
                intrinsic(5, Intrinsic::ScopeRun, &[4]),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![
                unit(2),
                intrinsic(3, Intrinsic::ArenaMake, &[2]),
                bind(4, Operation::Literal(Lit::String("departed-inner".into()))),
                intrinsic(5, Intrinsic::ArenaAlloc, &[3, 4]),
                project(6, 0, 0),
                perform(7, "left", &[6]),
            ],
            7,
        ),
        function(&[0, 1], vec![], 1),
    ])
}

#[test]
fn abort_closes_exact_departed_scope_not_selected_outer_owner() {
    let p = selected_owner_abort();
    let mut completed = false;
    for fuel in 0..220 {
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                completed = true;
                assert_eq!(heap.string(&value), Ok("outer-owner"));
                assert_eq!(heap.stats().arenas_freed, 2);
                heap.drop_value(value)
                    .expect("valid handler test operation");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            other => {
                let diagnostic = format!("fuel {fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        heap.finish()
            .unwrap_or_else(|e| panic!("fuel {fuel}: {e:?}"));
    }
    assert!(completed, "selected outer-owner storage survives the abort");
}

#[test]
fn singleton_die_from_tail_and_abort_arms_releases_parked_roots_without_typed_recovery() {
    let mut tail = tail_failure_scope();
    tail.functions.insert(
        4,
        function(&[0, 1], vec![intrinsic(2, Intrinsic::MatchFail, &[])], 2),
    );
    let mut abort = nested_tail_abort();
    abort.functions.insert(
        2,
        function(&[0, 1], vec![intrinsic(2, Intrinsic::MatchFail, &[])], 2),
    );
    for p in [tail, abort] {
        let mut completed = false;
        for fuel in 0..300 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("caller".into()))
                .expect("caller root");
            let argument = heap.dup(&external).expect("VM argument alias");
            let report = execute_program_outcome(&mut heap, &p, &entry(), vec![argument], fuel);
            assert_eq!(
                report.stdout, "",
                "no body/outer catch or error arm consumes Die"
            );
            match report.outcome {
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                ProgramOutcome::Failed(cause) => {
                    assert_eq!(causes::defect_error(&mut heap, cause), Error::NoMatch);
                    assert_eq!(heap.stats().arenas_allocated, 1);
                    assert_eq!(heap.stats().arenas_freed, 1);
                    completed = true;
                }
                other => causes::unexpected(&mut heap, other, "tail/abort singleton Die"),
            }
            assert_eq!(heap.string(&external), Ok("caller"));
            assert_eq!(heap.reference_count(&external), Ok(1));
            assert_eq!(heap.stats().live_objects, 1);
            heap.drop_value(external).expect("caller release");
            let stats = heap
                .finish()
                .expect("all active/parked/invocation/cause roots released");
            assert_eq!(stats.allocations, stats.frees);
            assert_eq!(stats.arenas_allocated, stats.arenas_freed);
        }
        assert!(completed);
    }
}
