use flick_core::ir::memory::ValueRep;
#[test]
fn provider_method_keeps_intervening_caller_handler_active() {
    let p = program(vec![
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
                closure(4, 2),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
        ),
        function(&[0, 1], vec![perform(2, "Context", "read", &[], &[1])], 2),
        function(
            &[0, 1],
            vec![
                closure(2, 3),
                closure(3, 4),
                handle(4, 3, "Context", "read", 2),
            ],
            4,
        ),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(99)))], 2),
        function(&[0, 1], vec![perform(2, "Service", "get", &[], &[1])], 2),
    ]);
    run_int(&p, 99);
}
#[test]
fn provider_remains_active_for_same_service_calls_and_actual_proto_fields() {
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                closure(1, 1),
                bind(2, Operation::Literal(Lit::Int(17))),
                bind(
                    3,
                    Operation::Record {
                        fields: vec![("get".into(), 1), ("__proto__".into(), 2)],
                    },
                ),
                intrinsic(4, Intrinsic::LayerSucceed, &[0, 3]),
                closure(5, 2),
                intrinsic(6, Intrinsic::LayerRun, &[4, 5]),
            ],
            6,
        ),
        function(
            &[0, 1],
            vec![perform(2, "Service", "__proto__", &[], &[])],
            2,
        ),
        function(&[0, 1], vec![perform(2, "Service", "get", &[], &[1])], 2),
    ]);
    run_int(&p, 17);
}
#[test]
fn provider_invocation_stages_dictionaries_before_arguments() {
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                closure(1, 1),
                bind(
                    2,
                    Operation::Record {
                        fields: vec![("pair".into(), 1)],
                    },
                ),
                intrinsic(3, Intrinsic::LayerSucceed, &[0, 2]),
                closure(4, 3),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
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
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                ),
                bind(
                    3,
                    Operation::Construct {
                        tag: "Pair".into(),
                        fields: vec![2, 1],
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                ),
            ],
            3,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::String("dictionary".into()))),
                bind(3, Operation::Literal(Lit::String("argument".into()))),
                perform(4, "Service", "pair", &[2], &[3]),
            ],
            4,
        ),
    ]);
    let mut heap = Heap::new();
    let pair = match execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000).outcome {
        ProgramOutcome::Returned(v) => v,
        other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
    };
    let a = heap
        .project(&pair, 0)
        .expect("valid counted Layer test operation");
    let b = heap
        .project(&pair, 1)
        .expect("valid counted Layer test operation");
    assert_eq!(
        heap.string(&a).expect("valid counted Layer test operation"),
        "dictionary"
    );
    assert_eq!(
        heap.string(&b).expect("valid counted Layer test operation"),
        "argument"
    );
    heap.drop_value(a)
        .expect("valid counted Layer test operation");
    heap.drop_value(b)
        .expect("valid counted Layer test operation");
    heap.drop_value(pair)
        .expect("valid counted Layer test operation");
    heap.finish().expect("valid counted Layer test operation");
}
#[test]
fn nearer_provider_blocks_outer_handler_when_its_field_is_missing() {
    let p = program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 4),
                handle(2, 0, "Service", "missing", 1),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::ServiceTag("Service".into())),
                bind(3, Operation::Record { fields: vec![] }),
                intrinsic(4, Intrinsic::LayerSucceed, &[2, 3]),
                closure(5, 2),
                intrinsic(6, Intrinsic::LayerRun, &[4, 5]),
            ],
            6,
        ),
        function(
            &[0, 1],
            vec![perform(2, "Service", "missing", &[], &[1])],
            2,
        ),
        function(&[0, 1], vec![], 1),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(999)))], 2),
    ]);
    let mut heap = Heap::new();
    {
        let diagnostic = "exact outcome expectation";
        let observed_outcome =
            execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000).outcome;
        let correct = matches!(&observed_outcome, ProgramOutcome::Fault(Error::MissingNamedField(name)) if name=="missing");
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("valid counted Layer test operation");
}
#[test]
fn nearer_partial_handler_skips_missing_operation_but_overrides_matching_one() {
    let mut p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                bind(1, Operation::Literal(Lit::Int(17))),
                bind(
                    2,
                    Operation::Record {
                        fields: vec![("get".into(), 1)],
                    },
                ),
                intrinsic(3, Intrinsic::LayerSucceed, &[0, 2]),
                closure(4, 1),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![
                closure(2, 2),
                closure(3, 3),
                handle(4, 3, "Service", "other", 2),
            ],
            4,
        ),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(33)))], 2),
        function(&[0, 1], vec![perform(2, "Service", "get", &[], &[])], 2),
    ]);
    run_int(&p, 17);
    p.functions.insert(
        1,
        function(
            &[0, 1],
            vec![
                closure(2, 2),
                closure(3, 3),
                handle(4, 3, "Service", "get", 2),
            ],
            4,
        ),
    );
    p.functions.insert(
        3,
        function(&[0, 1], vec![perform(2, "Service", "get", &[], &[1])], 2),
    );
    run_int(&p, 33);
}
