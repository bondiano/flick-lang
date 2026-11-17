fn assert_owned_string(p: &Program, expected: &str, raised: bool) {
    let mut completed = false;
    for fuel in 0..160 {
        let mut heap = Heap::new();
        let alias = heap
            .service_tag("Service")
            .expect("valid counted Layer test operation");
        let report = execute_program_outcome(&mut heap, p, &entry(), vec![], fuel);
        match report.outcome {
            ProgramOutcome::Returned(v) if !raised => {
                assert_eq!(
                    heap.string(&v).expect("valid counted Layer test operation"),
                    expected
                );
                heap.drop_value(v)
                    .expect("valid counted Layer test operation");
                completed = true;
            }
            ProgramOutcome::Raised(v) if raised => {
                assert_eq!(
                    heap.string(&v).expect("valid counted Layer test operation"),
                    expected
                );
                heap.drop_value(v)
                    .expect("valid counted Layer test operation");
                completed = true;
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            other => {
                let diagnostic = format!("fuel {fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        heap.drop_value(alias)
            .expect("valid counted Layer test operation");
        heap.finish().expect("valid counted Layer test operation");
    }
    assert!(completed);
}
#[test]
fn typed_method_failure_is_caught_by_intervening_caller_try() {
    let mut p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                closure(1, 1),
                bind(
                    2,
                    Operation::Record {
                        fields: vec![("fail".into(), 1)],
                    },
                ),
                intrinsic(3, Intrinsic::LayerSucceed, &[0, 2]),
                closure(4, 2),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
        ),
        function(&[0, 1], vec![intrinsic(2, Intrinsic::Raise, &[1])], 2),
        function(
            &[0, 1],
            vec![
                closure(2, 3),
                closure(3, 4),
                intrinsic(4, Intrinsic::Try, &[2, 3]),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Literal(Lit::String("caught at caller".into())),
                ),
                perform(3, "Service", "fail", &[], &[2]),
            ],
            3,
        ),
        function(&[0, 1], vec![], 1),
    ]);
    recovery::seal_bodies(&mut p, &[4]);
    assert_owned_string(&p, "caught at caller", false);
}
#[test]
fn raised_owned_value_survives_provider_scope_close() {
    let p = layer_with_callback(
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::String("raised".into()))),
                intrinsic(3, Intrinsic::Raise, &[2]),
            ],
            3,
        ),
        Operation::Literal(Lit::Unit),
    );
    assert_owned_string(&p, "raised", true);
}
#[test]
fn returned_owned_value_survives_provider_scope_close() {
    let p = layer_with_callback(
        function(
            &[0, 1],
            vec![bind(2, Operation::Literal(Lit::String("returned".into())))],
            2,
        ),
        Operation::Literal(Lit::Unit),
    );
    assert_owned_string(&p, "returned", false);
}
fn parked_provider(raise: bool) -> Program {
    program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 3),
                handle(2, 0, "Outer", "ping", 1),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::ServiceTag("Service".into())),
                bind(3, Operation::Literal(Lit::String("restored".into()))),
                bind(
                    4,
                    Operation::Record {
                        fields: vec![("get".into(), 3)],
                    },
                ),
                intrinsic(5, Intrinsic::LayerSucceed, &[2, 4]),
                closure(6, 2),
                intrinsic(7, Intrinsic::LayerRun, &[5, 6]),
            ],
            7,
        ),
        function(
            &[0, 1],
            vec![
                perform(2, "Outer", "ping", &[], &[1]),
                perform(3, "Service", "get", &[], &[]),
            ],
            3,
        ),
        if raise {
            function(
                &[0, 1],
                vec![
                    bind(2, Operation::Literal(Lit::String("discontinued".into()))),
                    intrinsic(3, Intrinsic::Raise, &[2]),
                ],
                3,
            )
        } else {
            function(&[0, 1], vec![], 1)
        },
    ])
}
#[test]
fn parked_provider_resumes_with_tail_segment_or_closes_on_discontinuation() {
    assert_owned_string(&parked_provider(false), "restored", false);
    assert_owned_string(&parked_provider(true), "discontinued", true);
}
#[test]
fn parked_provider_is_not_visible_to_outer_tail_method() {
    let mut p = parked_provider(false);
    p.functions.insert(
        3,
        function(&[0, 1], vec![perform(2, "Service", "get", &[], &[])], 2),
    );
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000);
    causes::assert_defect(
        &mut heap,
        report.outcome,
        &Error::MissingHandler {
            effect: "Service".into(),
            op: "get".into(),
        },
    );
    heap.finish().expect("valid counted Layer test operation");
}
#[test]
fn provider_owner_closes_its_arena_before_resuming_caller() {
    let mut p = constant_layer();
    p.functions.insert(
        0,
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                bind(1, Operation::Literal(Lit::Unit)),
                intrinsic(2, Intrinsic::LayerSucceed, &[0, 1]),
                closure(3, 1),
                intrinsic(4, Intrinsic::LayerRun, &[2, 3]),
                intrinsic(5, Intrinsic::ArenaGet, &[4]),
            ],
            5,
        ),
    );
    p.functions.insert(
        1,
        function(
            &[0, 1],
            vec![
                intrinsic(2, Intrinsic::ArenaMake, &[1]),
                bind(3, Operation::Literal(Lit::String("provider owned".into()))),
                intrinsic(4, Intrinsic::ArenaAlloc, &[2, 3]),
            ],
            4,
        ),
    );
    let mut success = false;
    for fuel in 0..100 {
        let mut heap = Heap::new();
        match execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel).outcome {
            ProgramOutcome::Failed(cause) => {
                assert_eq!(causes::defect_error(&mut heap, cause), Error::ClosedArena);
                success = true;
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
        }
        heap.finish().expect("valid counted Layer test operation");
    }
    assert!(success);
}
#[test]
fn layer_can_escape_but_its_foreign_execution_method_cannot_be_called() {
    let mut first = constant_layer();
    first.functions.insert(
        0,
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                closure(1, 1),
                bind(
                    2,
                    Operation::Record {
                        fields: vec![("get".into(), 1)],
                    },
                ),
                intrinsic(3, Intrinsic::LayerSucceed, &[0, 2]),
            ],
            3,
        ),
    );
    first.functions.insert(1, function(&[0, 1], vec![], 1));
    let second = program(vec![
        function(&[], vec![closure(0, 1)], 0),
        function(
            &[0, 1],
            vec![closure(2, 2), intrinsic(3, Intrinsic::LayerRun, &[1, 2])],
            3,
        ),
        function(&[0, 1], vec![perform(2, "Service", "get", &[], &[1])], 2),
    ]);
    let mut heap = Heap::new();
    let layer = match execute_program_outcome(&mut heap, &first, &entry(), vec![], 1000).outcome {
        ProgramOutcome::Returned(v) => v,
        other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
    };
    let alias = heap
        .dup(&layer)
        .expect("valid counted Layer test operation");
    {
        let diagnostic = "exact outcome expectation";
        let observed_outcome =
            execute_program_outcome(&mut heap, &second, &entry(), vec![layer], 1000).outcome;
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
        .expect("valid counted Layer test operation");
    heap.finish().expect("valid counted Layer test operation");
}
