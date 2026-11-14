#[test]
fn sealed_marker_cannot_cross_any_ordinary_operand_or_owned_edge() {
    for operation in forbidden_marker_operations() {
        let description = format!("{operation:?}");
        let mut program = sealed_selector(false);
        // Adjust helper arity for owner operations; their marker is rejected
        // before even attempting to select D or install lexical C.
        let helper_params: &[u32] = if matches!(
            operation,
            Operation::Within { .. } | Operation::WithOwnerEvidence { .. }
        ) {
            &[]
        } else {
            &[0]
        };
        program.functions.insert(
            4,
            function(
                helper_params,
                vec![bind(5, Operation::Literal(Lit::Unit))],
                5,
            ),
        );
        let annotated = matches!(
            operation,
            Operation::Reuse { .. } | Operation::ReuseWithDicts { .. }
        );
        let input = if annotated {
            Operation::Construct {
                tag: "placeholder".into(),
                fields: vec![2],
            }
        } else {
            operation.clone()
        };
        let mut selector = function(
            &[0, 1],
            vec![
                bind(2, Operation::RecoveryResult { selected: None }),
                bind(3, input),
            ],
            3,
        );
        if annotated {
            let instruction = selector
                .body
                .instructions
                .iter_mut()
                .find(|instruction| matches!(instruction, Instruction::Let { binding: 3, .. }))
                .expect("annotated operation site");
            *instruction = bind(3, operation);
        }
        program.functions.insert(2, selector);
        validate_program(&program).expect("actual annotated MIR is valid before execution");
        let mut done = false;
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
            let reached = fault_or_fuel(&mut heap, report.outcome, &Error::RecoveryEscape);
            assert_eq!(
                heap.string(&external).expect("caller alias survives"),
                "external"
            );
            heap.drop_value(external).expect("caller releases");
            let stats = heap
                .finish()
                .expect("all internal marker/closure/cause roots released");
            assert_eq!(stats.allocations, stats.frees, "{description}, fuel={fuel}");
            if reached {
                done = true;
                break;
            }
        }
        assert!(done, "actual rejection reached for {description}");
    }
}

#[test]
fn only_explicit_direct_suffix_helper_transports_matching_result_authority() {
    let mut program = sealed_selector(true);
    program.functions.insert(
        2,
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Call {
                    code: 4,
                    args: vec![],
                },
            )],
            2,
        ),
    );
    program.functions.insert(
        4,
        function(
            &[],
            vec![
                bind(
                    0,
                    Operation::Closure {
                        code: 3,
                        captures: vec![],
                    },
                ),
                bind(1, Operation::RecoveryResult { selected: Some(0) }),
            ],
            1,
        ),
    );
    let terminal = program
        .functions
        .remove(&4)
        .expect("existing explicit helper");
    program.functions.insert(5, terminal);
    program.functions.insert(
        4,
        function(
            &[],
            vec![bind(
                0,
                Operation::Call {
                    code: 5,
                    args: vec![],
                },
            )],
            0,
        ),
    );
    let mut heap = Heap::new();
    let report = execute_program_outcome(
        &mut heap,
        &program,
        &Name::new("Recovery", "main"),
        vec![],
        1000,
    );
    assert!(
        matches!(report.outcome, ProgramOutcome::Returned(Value::Int(42))),
        "{:?}",
        report.outcome
    );
    report.outcome.release(&mut heap).expect("outcome release");
    heap.finish().expect("actual empty heap");
}

#[test]
fn ordinary_apply_and_selected_body_do_not_inherit_selector_authority() {
    for selected_body in [false, true] {
        let mut program = sealed_selector(true);
        if selected_body {
            program.functions.insert(
                3,
                function(
                    &[0, 1],
                    vec![bind(2, Operation::RecoveryResult { selected: None })],
                    2,
                ),
            );
        } else {
            program.functions.insert(
                2,
                function(
                    &[0, 1],
                    vec![
                        bind(
                            2,
                            Operation::Closure {
                                code: 4,
                                captures: vec![],
                            },
                        ),
                        bind(3, Operation::Literal(Lit::Unit)),
                        bind(
                            4,
                            Operation::Apply {
                                function: 2,
                                argument: 3,
                            },
                        ),
                    ],
                    4,
                ),
            );
            program.functions.insert(
                4,
                function(
                    &[0, 1],
                    vec![bind(2, Operation::RecoveryResult { selected: None })],
                    2,
                ),
            );
        }
        let mut completed = false;
        for fuel in 0..1000 {
            let mut heap = Heap::new();
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Recovery", "main"),
                vec![],
                fuel,
            );
            completed = fault_or_fuel(&mut heap, report.outcome, &Error::RecoveryOutsideSelection);
            let stats = heap.finish().expect("actual empty heap at every prefix");
            assert_eq!(stats.allocations, stats.frees);
            if completed {
                break;
            }
        }
        assert!(completed, "authority rejection reached");
    }
}
