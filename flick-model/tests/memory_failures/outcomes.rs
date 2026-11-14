#[test]
fn outcomes_keep_stdout_and_transfer_only_the_payload() {
    for caught in [false, true] {
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &failure(caught, false), &entry(), vec![], 1000);
        assert_eq!(report.stdout, "before\n");
        let value = match report.outcome {
            ProgramOutcome::Returned(v) if caught => v,
            ProgramOutcome::Raised(v) if !caught => v,
            other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
        };
        assert_eq!(heap.string(&value), Ok("payload"));
        heap.drop_value(value).expect("valid typed-failure test");
        heap.finish().expect("valid typed-failure test");
        assert_eq!(heap.stats().allocations, heap.stats().frees);
    }
}
#[test]
fn faults_bypass_catch_and_keep_stdout() {
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &failure(true, true), &entry(), vec![], 1000);
    assert_eq!(report.stdout, "before\n");
    {
        let diagnostic = "exact outcome expectation";
        let observed_outcome = report.outcome;
        let correct = matches!(
            &observed_outcome,
            ProgramOutcome::Fault(Error::ExpectedResult)
        );
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("valid typed-failure test");
}
#[test]
fn every_fuel_cut_releases_unwind_and_handler_roots() {
    for caught in [false, true] {
        for fuel in 0..100 {
            let mut heap = Heap::new();
            let report =
                execute_program_outcome(&mut heap, &failure(caught, false), &entry(), vec![], fuel);
            match report.outcome {
                ProgramOutcome::Returned(v) | ProgramOutcome::Raised(v) => {
                    heap.drop_value(v).expect("valid typed-failure test");
                }
                ProgramOutcome::Fault(e) => assert_eq!(e, Error::OutOfFuel),
                other @ ProgramOutcome::Failed(_) => causes::unexpected(
                    &mut heap,
                    other,
                    "unexpected failure during ordinary typed unwind",
                ),
            }
            heap.finish().expect("valid typed-failure test");
        }
    }
}
#[test]
fn legacy_wrapper_drops_unhandled_payload() {
    let mut heap = Heap::new();
    assert_eq!(
        execute_program(&mut heap, &failure(false, false), &entry(), vec![], 1000)
            .expect_err("unhandled typed failure"),
        Error::UnhandledFailure
    );
    heap.finish().expect("valid typed-failure test");
}

#[test]
fn try_entry_faults_drop_registered_handler_captures() {
    for invalid_body in [false, true] {
        let setup = if invalid_body {
            bind(1, Operation::Literal(Lit::Int(0)))
        } else {
            bind(
                1,
                Operation::Closure {
                    code: 1,
                    captures: vec![],
                },
            )
        };
        let handler = if invalid_body {
            bind(
                2,
                Operation::Closure {
                    code: 2,
                    captures: vec![0],
                },
            )
        } else {
            bind(2, Operation::Literal(Lit::Int(0)))
        };
        let p = program(vec![
            function(
                &[],
                vec![
                    bind(0, Operation::Literal(Lit::String("capture".into()))),
                    setup,
                    handler,
                    intrinsic(3, Intrinsic::Try, &[1, 2]),
                ],
                3,
            ),
            function(&[0, 1], vec![intrinsic(2, Intrinsic::Raise, &[1])], 2),
            function(&[0, 1], vec![], 1),
        ]);
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000);
        {
            let diagnostic = "exact outcome expectation";
            let observed_outcome = report.outcome;
            let correct = matches!(
                &observed_outcome,
                ProgramOutcome::Fault(Error::ExpectedClosure)
            );
            let detail = format!("{observed_outcome:?}");
            observed_outcome
                .release(&mut heap)
                .expect("every owned outcome released before assertion");
            assert!(correct, "{diagnostic}: {detail}");
        };
        heap.finish().expect("valid typed-failure test");
    }
}

#[test]
fn success_releases_unused_handler_and_its_captures() {
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::String("unused".into()))),
                bind(
                    1,
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                ),
                bind(
                    2,
                    Operation::Closure {
                        code: 2,
                        captures: vec![0],
                    },
                ),
                intrinsic(3, Intrinsic::Try, &[1, 2]),
            ],
            3,
        ),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(42)))], 2),
        function(&[0, 1], vec![], 1),
    ]);
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000);
    {
        let diagnostic = "exact outcome expectation";
        let observed_outcome = report.outcome;
        let correct = matches!(&observed_outcome, ProgramOutcome::Returned(Value::Int(42)));
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("valid typed-failure test");
}

#[test]
fn result_payload_retains_external_alias_on_success_and_failure() {
    for tag in ["Prelude.Result.Ok", "Prelude.Result.Err"] {
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
            function(&[0, 1], vec![intrinsic(2, Intrinsic::FromResult, &[1])], 2),
        ]);
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("shared".into()))
            .expect("valid typed-failure test");
        let alias = heap.dup(&payload).expect("valid typed-failure test");
        let wrapper = heap
            .construct(tag, vec![payload])
            .expect("valid typed-failure test");
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![wrapper], 1000);
        let value = match report.outcome {
            ProgramOutcome::Returned(v) if tag.ends_with("Ok") => v,
            ProgramOutcome::Raised(v) if tag.ends_with("Err") => v,
            other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
        };
        assert_eq!(value.object_id(), alias.object_id());
        heap.drop_value(value).expect("valid typed-failure test");
        assert_eq!(heap.string(&alias), Ok("shared"));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(alias).expect("valid typed-failure test");
        heap.finish().expect("valid typed-failure test");
    }
}

#[test]
fn malformed_results_are_faults_not_typed_failures() {
    for (tag, fields, dicts) in [
        ("Result.Ok", vec![1], vec![]),
        ("Prelude.Result.Ok", vec![], vec![]),
        ("Prelude.Result.Err", vec![1, 1], vec![]),
        ("Prelude.Result.Err", vec![1], vec![1]),
    ] {
        let p = program(vec![
            function(
                &[],
                vec![
                    bind(0, Operation::Literal(Lit::Int(5))),
                    bind(
                        1,
                        Operation::Closure {
                            code: 1,
                            captures: vec![],
                        },
                    ),
                    bind(
                        2,
                        Operation::Closure {
                            code: 2,
                            captures: vec![],
                        },
                    ),
                    intrinsic(3, Intrinsic::Try, &[1, 2]),
                ],
                3,
            ),
            function(
                &[0, 1],
                vec![
                    bind(
                        2,
                        Operation::ConstructWithDicts {
                            tag: tag.into(),
                            fields,
                            dicts,
                        },
                    ),
                    intrinsic(3, Intrinsic::FromResult, &[2]),
                ],
                3,
            ),
            function(&[0, 1], vec![], 1),
        ]);
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000);
        {
            let diagnostic = format!("{report:?}");
            let observed_outcome = report.outcome;
            let correct = matches!(
                &observed_outcome,
                ProgramOutcome::Fault(Error::ExpectedResult)
            );
            let detail = format!("{observed_outcome:?}");
            observed_outcome
                .release(&mut heap)
                .expect("every owned outcome released before assertion");
            assert!(correct, "{diagnostic}: {detail}");
        };
        heap.finish().expect("valid typed-failure test");
    }
}

#[test]
fn raised_outcome_cancels_queued_applications() {
    let mut heap = Heap::new();
    let argument = heap
        .literal(&Lit::String("never applied".into()))
        .expect("valid typed-failure test");
    let report = execute_program_outcome(
        &mut heap,
        &failure(false, false),
        &entry(),
        vec![argument],
        1000,
    );
    let ProgramOutcome::Raised(value) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    heap.drop_value(value).expect("valid typed-failure test");
    heap.finish().expect("valid typed-failure test");
}
