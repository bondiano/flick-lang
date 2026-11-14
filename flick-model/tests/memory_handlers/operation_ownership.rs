#[test]
fn dictionaries_then_arguments_are_applied_as_owned_unary_stages() {
    let mut effects = handled(&[("right", 1)]);
    effects[0].operations[0].dicts = 1;
    effects[0].operations[0].arity = 2;
    let p = program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 2),
                bind(
                    2,
                    Operation::Handle {
                        body: 0,
                        effects,
                        ret: None,
                        errors: None,
                    },
                ),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![
                int(2, 1),
                int(3, 2),
                int(4, 3),
                bind(
                    5,
                    Operation::Perform {
                        effect: "Pair".into(),
                        op: "right".into(),
                        dicts: vec![2],
                        args: vec![3, 4],
                    },
                ),
            ],
            5,
        ),
        function(&[0, 1], vec![capture(2, 3, &[1])], 2),
        function(&[0, 1], vec![project(2, 0, 0), capture(3, 4, &[2, 1])], 3),
        function(
            &[0, 1],
            vec![
                project(2, 0, 0),
                project(3, 0, 1),
                bind(
                    4,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![2, 3, 1],
                    },
                ),
            ],
            4,
        ),
    ]);
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000);
    let ProgramOutcome::Returned(value) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    for (field, expected) in [1, 2, 3].into_iter().enumerate() {
        assert_eq!(heap.project(&value, field), Ok(Value::Int(expected)));
    }
    heap.drop_value(value)
        .expect("valid handler test operation");
    heap.finish().expect("all execution roots released");
}

#[test]
fn zero_arity_operation_is_an_owned_value_not_an_invented_unit_application() {
    let mut effects = handled(&[("right", 1)]);
    effects[0].operations[0].arity = 0;
    let p = program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                int(1, 42),
                bind(
                    2,
                    Operation::Handle {
                        body: 0,
                        effects,
                        ret: None,
                        errors: None,
                    },
                ),
            ],
            2,
        ),
        function(&[0, 1], vec![perform(2, "right", &[])], 2),
    ]);
    run_int(&p, 42);
}

#[test]
fn malformed_operation_counts_missing_handlers_and_noncallables_are_faults() {
    for malformed in ["count", "argcount", "missing", "callable", "control"] {
        let mut effects = handled(&[("right", 1)]);
        if malformed == "count" {
            effects[0].operations[0].dicts = 1;
        }
        if malformed == "argcount" {
            effects[0].operations[0].arity = 2;
        }
        if malformed == "missing" {
            effects[0].operations[0].name = "left".into();
        }
        if malformed == "control" {
            effects[0].operations[0].class = OpClass::Control;
        }
        let p = caught_fault_program(program(vec![
            function(
                &[],
                vec![
                    closure(0, 1),
                    if malformed == "callable" {
                        int(1, 42)
                    } else {
                        closure(1, 2)
                    },
                    bind(
                        2,
                        Operation::Handle {
                            body: 0,
                            effects,
                            ret: None,
                            errors: None,
                        },
                    ),
                ],
                2,
            ),
            function(&[0, 1], vec![unit(2), perform(3, "right", &[2])], 3),
            constant(20),
        ]));
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000);
        if malformed == "missing" {
            causes::assert_defect(
                &mut heap,
                report.outcome,
                &Error::MissingHandler {
                    effect: "Pair".into(),
                    op: "right".into(),
                },
            );
            heap.finish().expect("missing-handler cause released");
            continue;
        }
        let correct = matches!(
            (&report.outcome, malformed),
            (
                ProgramOutcome::Fault(Error::OperationArgumentCount { .. }),
                "count"
            ) | (
                ProgramOutcome::Fault(Error::OperationArgumentCount { .. }),
                "argcount"
            ) | (ProgramOutcome::Fault(Error::ExpectedClosure), "callable")
                | (
                    ProgramOutcome::Fault(Error::UnsupportedOperation("control handler")),
                    "control",
                )
        );
        let diagnostic = format!("{malformed}: {report:?}");
        report
            .outcome
            .release(&mut heap)
            .expect("all outcome variants released");
        assert!(correct, "{diagnostic}");
        heap.finish().expect("all execution roots released");
    }
}

#[test]
fn every_fuel_cut_releases_active_parked_and_pending_handler_roots() {
    for p in [
        partial_handlers(),
        abort_with_catch(),
        scope_transfer(OpClass::Tail),
        scope_transfer(OpClass::Abort),
    ] {
        for fuel in 0..180 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("external alias".into()))
                .expect("valid handler test operation");
            let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel);
            match report.outcome {
                ProgramOutcome::Returned(v) | ProgramOutcome::Raised(v) => {
                    heap.drop_value(v).expect("valid handler test operation");
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
            assert_eq!(heap.string(&external), Ok("external alias"));
            heap.drop_value(external)
                .expect("valid handler test operation");
            heap.finish()
                .unwrap_or_else(|e| panic!("fuel {fuel}: {e:?}"));
            assert_eq!(heap.stats().allocations, heap.stats().frees);
        }
    }
}

fn capture(binding: Local, code: CodeId, captures: &[Local]) -> Instruction {
    bind(
        binding,
        Operation::Closure {
            code,
            captures: captures.to_vec(),
        },
    )
}
fn project(binding: Local, object: Local, field: usize) -> Instruction {
    bind(binding, Operation::Project { object, field })
}

#[test]
fn every_fuel_cut_preserves_an_external_alias_of_a_parked_owned_capture() {
    let p = program(vec![
        function(&[], vec![closure(0, 1)], 0),
        function(
            &[0, 1],
            vec![
                capture(2, 2, &[1]),
                closure(3, 3),
                handle(4, 2, &[("right", 3)]),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                project(2, 0, 0),
                unit(3),
                perform(4, "right", &[3]),
                bind(
                    5,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![2, 4],
                    },
                ),
            ],
            5,
        ),
        constant(20),
    ]);
    let mut completed = false;
    for fuel in 0..130 {
        let mut heap = Heap::new();
        let alias = heap
            .literal(&Lit::String("external-owned-capture".into()))
            .expect("valid handler test operation");
        let argument = heap.dup(&alias).expect("valid handler test operation");
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![argument], fuel);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                completed = true;
                let held = heap
                    .project(&value, 0)
                    .expect("valid handler test operation");
                assert_eq!(heap.string(&held), Ok("external-owned-capture"));
                heap.drop_value(held).expect("valid handler test operation");
                heap.drop_value(value)
                    .expect("valid handler test operation");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            other => {
                let diagnostic = format!("fuel {fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        assert_eq!(heap.string(&alias), Ok("external-owned-capture"));
        heap.drop_value(alias)
            .expect("valid handler test operation");
        heap.finish()
            .unwrap_or_else(|e| panic!("fuel {fuel}: {e:?}"));
    }
    assert!(
        completed,
        "the fueled program returns its owned captured value"
    );
}

#[test]
fn foreign_execution_operation_closure_is_a_fault_and_external_alias_survives() {
    let maker = program(vec![function(&[], vec![closure(0, 1)], 0), constant(1)]);
    let p = program(vec![
        function(&[], vec![closure(0, 1)], 0),
        function(
            &[0, 1],
            vec![closure(2, 2), handle(3, 2, &[("right", 1)])],
            3,
        ),
        function(&[0, 1], vec![unit(2), perform(3, "right", &[2])], 3),
    ]);
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &maker, &entry(), vec![], 1000);
    let ProgramOutcome::Returned(foreign) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    let alias = heap.dup(&foreign).expect("valid handler test operation");
    let report = execute_program_outcome(&mut heap, &p, &entry(), vec![foreign], 1000);
    {
        let diagnostic = format!("{report:?}");
        let observed_outcome = report.outcome;
        let correct = matches!(
            &observed_outcome,
            ProgramOutcome::Fault(Error::ForeignClosure)
        );
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.drop_value(alias)
        .expect("valid handler test operation");
    heap.finish().expect("all execution roots released");
}

fn tail_failure_scope() -> Program {
    program(vec![
        function(
            &[],
            vec![
                unit(0),
                intrinsic(1, Intrinsic::RefMake, &[0]),
                capture(2, 1, &[1]),
                capture(3, 2, &[1]),
                intrinsic(4, Intrinsic::Try, &[2, 3]),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                project(2, 0, 0),
                capture(3, 3, &[2]),
                closure(4, 4),
                closure(5, 7),
                bind(
                    6,
                    Operation::Handle {
                        body: 3,
                        effects: handled(&[("right", 4)]),
                        ret: Some(5),
                        errors: Some(5),
                    },
                ),
            ],
            6,
        ),
        function(
            &[0, 1],
            vec![
                intrinsic(2, Intrinsic::PrintLine, &[1]),
                project(3, 0, 0),
                intrinsic(4, Intrinsic::RefGet, &[3]),
                intrinsic(5, Intrinsic::ArenaGet, &[4]),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![
                project(2, 0, 0),
                capture(3, 5, &[2]),
                intrinsic(4, Intrinsic::ScopeRun, &[3]),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::String("armfailed".into()))),
                intrinsic(3, Intrinsic::Raise, &[2]),
            ],
            3,
        ),
        function(
            &[0, 1],
            vec![
                project(2, 0, 0),
                unit(3),
                intrinsic(4, Intrinsic::ArenaMake, &[3]),
                bind(5, Operation::Literal(Lit::String("body-owned".into()))),
                intrinsic(6, Intrinsic::ArenaAlloc, &[4, 5]),
                intrinsic(7, Intrinsic::RefSet, &[2, 6]),
                closure(8, 6),
                closure(9, 7),
                intrinsic(10, Intrinsic::Try, &[8, 9]),
            ],
            10,
        ),
        function(&[0, 1], vec![unit(2), perform(3, "right", &[2])], 3),
        constant(99),
    ])
}
