fn same_payload_entry(handler_errors: bool) -> Function {
    function(
        &[0, 1],
        vec![
            bind(
                2,
                Operation::Closure {
                    code: 4,
                    captures: vec![1],
                },
            ),
            bind(
                3,
                Operation::Closure {
                    code: 2,
                    captures: vec![],
                },
            ),
            bind(
                4,
                if handler_errors {
                    Operation::Handle {
                        body: 2,
                        effects: vec![],
                        ret: None,
                        errors: Some(3),
                    }
                } else {
                    Operation::Intrinsic {
                        kind: Intrinsic::Try,
                        args: vec![2, 3],
                    }
                },
            ),
        ],
        4,
    )
}

fn same_payload_program(handler_errors: bool, ordinary_none: bool) -> Program {
    let mut program = sealed_selector(true);
    program.functions.insert(
        0,
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
    );
    program
        .functions
        .insert(1, same_payload_entry(handler_errors));
    program.functions.insert(
        4,
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
                intrinsic(3, Intrinsic::Raise, &[2]),
            ],
            3,
        ),
    );
    program.functions.insert(
        2,
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Closure {
                        code: 3,
                        captures: vec![1],
                    },
                ),
                bind(3, Operation::RecoveryResult { selected: Some(2) }),
            ],
            3,
        ),
    );
    let body = if ordinary_none {
        Operation::Construct {
            tag: "Prelude.Option.None".into(),
            fields: vec![],
        }
    } else {
        Operation::Project {
            object: 0,
            field: 0,
        }
    };
    program
        .functions
        .insert(3, function(&[0, 1], vec![bind(2, body)], 2));
    program
}

#[test]
fn selected_body_returns_same_owned_payload_or_ordinary_none_without_reinterpretation() {
    for handler_errors in [false, true] {
        for ordinary_none in [false, true] {
            let program = same_payload_program(handler_errors, ordinary_none);
            let mut complete = false;
            for fuel in 0..1000 {
                let mut heap = Heap::new();
                let input = heap
                    .literal(&Lit::String("same payload".into()))
                    .expect("owned argument");
                let external = heap
                    .dup(&input)
                    .expect("external alias of actual raised payload");
                let report = execute_program_outcome(
                    &mut heap,
                    &program,
                    &Name::new("Recovery", "main"),
                    vec![input],
                    fuel,
                );
                match report.outcome {
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    ProgramOutcome::Returned(value) => {
                        if ordinary_none {
                            assert_eq!(heap.tag(&value), Ok("Prelude.Option.None"));
                        } else {
                            assert_eq!(
                                value.object_id(),
                                external.object_id(),
                                "SAME payload is a successful body result, not Miss/re-raise"
                            );
                        }
                        heap.drop_value(value)
                            .expect("ordinary selected-body result release");
                        complete = true;
                    }
                    other => causes::unexpected(
                        &mut heap,
                        other,
                        "selected body always returns ordinary result",
                    ),
                }
                assert_eq!(heap.reference_count(&external), Ok(1));
                heap.drop_value(external).expect("caller alias release");
                let stats = heap
                    .finish()
                    .expect("actual no occupied objects at every prefix");
                assert_eq!(stats.allocations, stats.frees);
                if complete {
                    break;
                }
            }
            assert!(complete);
        }
    }
}

fn lookalike_selector(kind: u8) -> Program {
    let mut program = sealed_selector(true);
    let closure = || {
        bind(
            2,
            Operation::Closure {
                code: 3,
                captures: vec![],
            },
        )
    };
    let (instructions, result) = match kind {
        0 => (vec![bind(2, Operation::Literal(Lit::Unit))], 2),
        1 => (vec![closure()], 2),
        2 => (
            vec![bind(
                2,
                Operation::Construct {
                    tag: "Prelude.Option.None".into(),
                    fields: vec![],
                },
            )],
            2,
        ),
        3 => (
            vec![
                closure(),
                bind(
                    3,
                    Operation::Construct {
                        tag: "Prelude.Option.Some".into(),
                        fields: vec![2],
                    },
                ),
            ],
            3,
        ),
        4 => (
            vec![
                closure(),
                bind(
                    3,
                    Operation::Record {
                        fields: vec![("selected".into(), 2)],
                    },
                ),
            ],
            3,
        ),
        5 => (
            vec![
                closure(),
                bind(
                    3,
                    Operation::Construct {
                        tag: "RecoveryResult".into(),
                        fields: vec![2],
                    },
                ),
            ],
            3,
        ),
        _ => unreachable!(),
    };
    program
        .functions
        .insert(2, function(&[0, 1], instructions, result));
    program
}

#[test]
fn ordinary_unit_closure_option_record_and_tag_lookalikes_are_not_sealed_results() {
    for kind in 0..6 {
        let program = lookalike_selector(kind);
        let mut complete = false;
        for fuel in 0..1000 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("external".into()))
                .expect("caller root");
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Recovery", "main"),
                vec![],
                fuel,
            );
            complete = fault_or_fuel(&mut heap, report.outcome, &Error::ExpectedRecoveryResult);
            assert_eq!(heap.string(&external), Ok("external"));
            heap.drop_value(external).expect("caller root release");
            let stats = heap
                .finish()
                .expect("lookalike edges and original C actually released");
            assert_eq!(stats.allocations, stats.frees);
            if complete {
                break;
            }
        }
        assert!(complete, "actual lookalike rejection reached");
    }
}
