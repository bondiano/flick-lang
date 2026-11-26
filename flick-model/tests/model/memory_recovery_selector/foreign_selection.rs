fn foreign_selection_programs() -> (Program, Program) {
    let mut factory = sealed_selector(true);
    factory.functions.insert(
        0,
        function(
            &[],
            vec![bind(
                0,
                Operation::Closure {
                    code: 3,
                    captures: vec![],
                },
            )],
            0,
        ),
    );
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
    program.functions.insert(
        1,
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
                intrinsic(4, Intrinsic::Try, &[2, 3]),
            ],
            4,
        ),
    );
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
                        rep: ValueRep::Cell,
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
            vec![bind(2, Operation::RecoveryResult { selected: Some(1) })],
            2,
        ),
    );
    (factory, program)
}

#[test]
fn foreign_execution_selected_closure_cannot_acquire_new_selection_authority() {
    let (factory, program) = foreign_selection_programs();
    let mut complete = false;
    for fuel in 0..1000 {
        let mut heap = Heap::new();
        let produced = execute_program_outcome(
            &mut heap,
            &factory,
            &Name::new("Recovery", "main"),
            vec![],
            1000,
        );
        let ProgramOutcome::Returned(closure) = produced.outcome else {
            causes::unexpected(
                &mut heap,
                produced.outcome,
                "foreign execution factory must return real owned closure",
            )
        };
        let external = heap
            .dup(&closure)
            .expect("caller alias to previous execution closure");
        let report = execute_program_outcome(
            &mut heap,
            &program,
            &Name::new("Recovery", "main"),
            vec![closure],
            fuel,
        );
        complete = fault_or_fuel(&mut heap, report.outcome, &Error::ForeignClosure);
        assert_eq!(heap.reference_count(&external), Ok(1));
        heap.drop_value(external)
            .expect("caller releases previous execution closure alias");
        let stats = heap
            .finish()
            .expect("both executions' owned roots actually gone");
        assert_eq!(stats.allocations, stats.frees);
        if complete {
            break;
        }
    }
    assert!(complete);
}

#[test]
fn tailfailure_outer_recovery_has_no_old_selection_at_any_fuel_prefix() {
    for handler_errors in [false, true] {
        let program = effects::tail_failure_outer_recovery(handler_errors);
        let mut complete = false;
        for fuel in 0..1000 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("caller".into()))
                .expect("caller root");
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Recovery", "main"),
                vec![],
                fuel,
            );
            assert!(
                "guard\narm\nouter\n".starts_with(&report.stdout),
                "{report:?}"
            );
            match &report.outcome {
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                ProgramOutcome::Returned(value) => {
                    assert_eq!(heap.string(value), Ok("replacement"));
                    assert_eq!(report.stdout, "guard\narm\nouter\n");
                    complete = true;
                }
                other => {
                    let diagnostic =
                        format!("handler_errors={handler_errors}, fuel={fuel}: {other:?}");
                    causes::unexpected(&mut heap, report.outcome, &diagnostic)
                }
            }
            assert_eq!(heap.string(&external), Ok("caller"));
            report
                .outcome
                .release(&mut heap)
                .expect("owned outcome release");
            heap.drop_value(external).expect("caller release");
            let stats = heap
                .finish()
                .expect("old C, D, states, and parked frames drained");
            assert_eq!(stats.allocations, stats.frees);
            if complete {
                break;
            }
        }
        assert!(complete);
    }
}

#[test]
fn global_initializer_and_owner_context_helpers_are_ordinary_authority_boundaries() {
    for case in 0..3 {
        let mut program = sealed_selector(true);
        let (instructions, result) = if case == 0 {
            let name = Name::new("Recovery", "initializer");
            program.globals.push(Global {
                name: name.clone(),
                initializer: 4,
            });
            (vec![bind(2, Operation::Global(name))], 2)
        } else {
            let operation = if case == 1 {
                Operation::Within {
                    owner: 3,
                    code: 4,
                    args: vec![],
                }
            } else {
                Operation::WithOwnerEvidence {
                    owner: 3,
                    code: 4,
                    args: vec![],
                }
            };
            (
                vec![
                    bind(2, Operation::Literal(Lit::Unit)),
                    intrinsic(3, Intrinsic::ScopeCurrent, &[2]),
                    bind(4, operation),
                ],
                4,
            )
        };
        program
            .functions
            .insert(2, function(&[0, 1], instructions, result));
        program.functions.insert(
            4,
            function(
                &[],
                vec![bind(0, Operation::RecoveryResult { selected: None })],
                0,
            ),
        );
        let mut complete = false;
        for fuel in 0..1000 {
            let mut heap = Heap::new();
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Recovery", "main"),
                vec![],
                fuel,
            );
            complete = fault_or_fuel(&mut heap, report.outcome, &Error::RecoveryOutsideSelection);
            let stats = heap
                .finish()
                .expect("initializer marker/current C/context helper roots drained");
            assert_eq!(stats.allocations, stats.frees);
            if complete {
                break;
            }
        }
        assert!(complete);
    }
}
