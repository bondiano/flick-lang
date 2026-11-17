fn assert_fault_and_finish(p: &Program, expected: &Error) {
    let mut reached = false;
    for fuel in 0..140 {
        let mut heap = Heap::new();
        let alias = heap
            .service_tag("Service")
            .expect("valid counted Layer test operation");
        match execute_program_outcome(&mut heap, p, &entry(), vec![], fuel).outcome {
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            ProgramOutcome::Fault(error) => {
                assert_ne!(
                    expected,
                    &Error::ClosedArena,
                    "sealed arena retirement is not an evaluator Fault"
                );
                assert_eq!(&error, expected);
                reached = true;
            }
            ProgramOutcome::Failed(cause) => {
                let actual = causes::defect_error(&mut heap, cause);
                assert_eq!(expected, &Error::ClosedArena);
                assert_eq!(&actual, expected);
                reached = true;
            }
            other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
        }
        heap.drop_value(alias)
            .expect("valid counted Layer test operation");
        heap.finish().expect("valid counted Layer test operation");
    }
    assert!(reached);
}
#[test]
fn invalid_nominal_inputs_and_callback_entry_faults_release_both_operands() {
    let bad_tag = program(vec![function(
        &[],
        vec![
            bind(0, Operation::Literal(Lit::String("Service".into()))),
            bind(
                1,
                Operation::Literal(Lit::String("owned implementation".into())),
            ),
            intrinsic(2, Intrinsic::LayerSucceed, &[0, 1]),
        ],
        2,
    )]);
    assert_fault_and_finish(&bad_tag, &Error::ExpectedServiceTag);
    let bad_layer = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::String("not a layer".into()))),
                closure(1, 1),
                intrinsic(2, Intrinsic::LayerRun, &[0, 1]),
            ],
            2,
        ),
        function(&[0, 1], vec![], 1),
    ]);
    assert_fault_and_finish(&bad_layer, &Error::ExpectedLayer);
    let bad_callback = program(vec![function(
        &[],
        vec![
            bind(0, Operation::ServiceTag("Service".into())),
            bind(
                1,
                Operation::Literal(Lit::String("owned implementation".into())),
            ),
            intrinsic(2, Intrinsic::LayerSucceed, &[0, 1]),
            bind(3, Operation::Literal(Lit::String("not callable".into()))),
            intrinsic(4, Intrinsic::LayerRun, &[2, 3]),
        ],
        4,
    )]);
    assert_fault_and_finish(&bad_callback, &Error::ExpectedClosure);
}
#[test]
fn method_allocates_under_caller_scope_owner_not_provider_owner() {
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                closure(1, 1),
                bind(
                    2,
                    Operation::Record {
                        fields: vec![("allocate".into(), 1)],
                    },
                ),
                intrinsic(3, Intrinsic::LayerSucceed, &[0, 2]),
                closure(4, 2),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![
                intrinsic(2, Intrinsic::ArenaMake, &[1]),
                bind(
                    3,
                    Operation::Literal(Lit::String("caller allocation".into())),
                ),
                intrinsic(4, Intrinsic::ArenaAlloc, &[2, 3]),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                closure(2, 3),
                intrinsic(3, Intrinsic::ScopeRun, &[2]),
                intrinsic(4, Intrinsic::ArenaGet, &[3]),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::Unit)),
                perform(3, "Service", "allocate", &[], &[2]),
            ],
            3,
        ),
    ]);
    assert_fault_and_finish(&p, &Error::ClosedArena);
}
#[test]
fn within_restores_selection_without_closing_selected_outer_storage() {
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Unit)),
                intrinsic(1, Intrinsic::ScopeCurrent, &[0]),
                bind(2, Operation::ServiceTag("Service".into())),
                intrinsic(3, Intrinsic::LayerSucceed, &[2, 0]),
                bind(
                    4,
                    Operation::Closure {
                        code: 1,
                        captures: vec![1],
                    },
                ),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
                intrinsic(6, Intrinsic::ArenaGet, &[5]),
            ],
            6,
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
                    Operation::Within {
                        owner: 2,
                        code: 2,
                        args: vec![1],
                    },
                ),
            ],
            3,
        ),
        function(
            &[0],
            vec![
                intrinsic(1, Intrinsic::ArenaMake, &[0]),
                bind(
                    2,
                    Operation::Literal(Lit::String("outer owner remains open".into())),
                ),
                intrinsic(3, Intrinsic::ArenaAlloc, &[1, 2]),
            ],
            3,
        ),
    ]);
    assert_owned_string(&p, "outer owner remains open", false);
}
#[test]
fn outer_abort_closes_departed_provider_before_invoking_abort_arm() {
    let mut p = program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 3),
                handle(2, 0, "Outer", "abort", 1),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::ServiceTag("Service".into())),
                intrinsic(3, Intrinsic::LayerSucceed, &[2, 1]),
                closure(4, 2),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![
                intrinsic(2, Intrinsic::ArenaMake, &[1]),
                bind(3, Operation::Literal(Lit::String("departed".into()))),
                intrinsic(4, Intrinsic::ArenaAlloc, &[2, 3]),
                perform(5, "Outer", "abort", &[], &[4]),
            ],
            5,
        ),
        function(&[0, 1], vec![intrinsic(2, Intrinsic::ArenaGet, &[1])], 2),
    ]);
    if let Instruction::Let {
        value: Operation::Handle { effects, .. },
        ..
    } = &mut p
        .functions
        .get_mut(&0)
        .expect("valid counted Layer test operation")
        .body
        .instructions[2]
    {
        effects[0].operations[0].class = OpClass::Abort;
    } else {
        panic!("handler instruction");
    }
    assert_fault_and_finish(&p, &Error::ClosedArena);
}
#[test]
fn method_defects_are_not_intercepted_by_caller_try() {
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                bind(1, Operation::Literal(Lit::String("not callable".into()))),
                bind(
                    2,
                    Operation::Record {
                        fields: vec![("bad".into(), 1)],
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
                intrinsic(4, Intrinsic::Try, &[2, 3]),
            ],
            4,
        ),
        function(&[0, 1], vec![perform(2, "Service", "bad", &[], &[1])], 2),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(999)))], 2),
    ]);
    assert_fault_and_finish(&p, &Error::ExpectedClosure);
}
