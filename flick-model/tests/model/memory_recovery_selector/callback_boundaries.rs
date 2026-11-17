#[test]
fn both_consumers_park_resume_or_discard_current_selector_before_callback() {
    for handler_errors in [false, true] {
        for (abort, arm_fails) in [(false, false), (true, false), (false, true)] {
            let program = selection_effects(handler_errors, abort, arm_fails);
            let expected_stdout = if !abort && !arm_fails {
                "guard\narm\nbody\n"
            } else {
                "guard\narm\n"
            };
            let mut complete = false;
            for fuel in 0..1000 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("external".into()))
                    .expect("external root");
                let report = execute_program_outcome(
                    &mut heap,
                    &program,
                    &Name::new("Recovery", "main"),
                    vec![],
                    fuel,
                );
                assert!(
                    expected_stdout.starts_with(&report.stdout),
                    "ordered side effects: {report:?}"
                );
                match &report.outcome {
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    ProgramOutcome::Returned(Value::Int(value)) if !arm_fails => {
                        assert_eq!(*value, if abort { 7 } else { 42 });
                        complete = true;
                    }
                    ProgramOutcome::Raised(value) if arm_fails => {
                        assert_eq!(
                            heap.string(value).expect("replacement D only"),
                            "replacement"
                        );
                        complete = true;
                    }
                    other => {
                        let diagnostic = format!(
                            "handler_errors={handler_errors} abort={abort} arm_fails={arm_fails} fuel={fuel}: {other:?}"
                        );
                        causes::unexpected(&mut heap, report.outcome, &diagnostic)
                    }
                }
                if complete {
                    assert_eq!(report.stdout, expected_stdout);
                }
                assert_eq!(
                    heap.string(&external).expect("external root survives"),
                    "external"
                );
                report.outcome.release(&mut heap).expect("outcome release");
                heap.drop_value(external).expect("caller release");
                let stats = heap
                    .finish()
                    .expect("current selection and parked/tailfail roots all reclaimed");
                assert_eq!(stats.allocations, stats.frees);
                if complete {
                    break;
                }
            }
            assert!(complete, "actual terminal outcome reached");
        }
    }
}

#[test]
fn ordinary_tasks_and_getters_cannot_find_an_ambient_executing_selection() {
    use flick_core::ir::memory::DictionaryEntry;
    for getter in [false, true] {
        let mut program = sealed_selector(true);
        let instructions = if getter {
            vec![
                bind(
                    2,
                    Operation::Closure {
                        code: 4,
                        captures: vec![],
                    },
                ),
                bind(
                    3,
                    Operation::Dictionary {
                        entries: vec![("get".into(), DictionaryEntry::Getter(2))],
                    },
                ),
                bind(
                    4,
                    Operation::Method {
                        dictionary: 3,
                        field: "get".into(),
                    },
                ),
            ]
        } else {
            vec![
                bind(
                    2,
                    Operation::Task {
                        code: 4,
                        captures: vec![],
                    },
                ),
                bind(4, Operation::Await(2)),
            ]
        };
        program
            .functions
            .insert(2, function(&[0, 1], instructions, 4));
        program.functions.insert(
            4,
            function(
                &[0, 1],
                vec![bind(2, Operation::RecoveryResult { selected: None })],
                2,
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
                .expect("original C and opaque/ordinary callback roots reclaimed");
            assert_eq!(stats.allocations, stats.frees);
            if complete {
                break;
            }
        }
        assert!(complete, "actual authority error reached");
    }
}

fn direct_defect() -> Function {
    function(
        &[0, 1],
        vec![
            bind(2, Operation::Literal(Lit::String("fault".into()))),
            intrinsic(3, Intrinsic::PrintLine, &[2]),
            bind(4, Operation::Literal(Lit::Int(1))),
            bind(5, Operation::Literal(Lit::Int(0))),
            intrinsic(6, Intrinsic::IntUnsafeDiv, &[4, 5]),
        ],
        6,
    )
}

#[test]
fn guard_selected_body_and_tail_arm_die_replace_original_without_concatenation() {
    for handler_errors in [false, true] {
        for (code, expected_stdout) in [
            (4, "fault\n"),
            (5, "guard\narm\nfault\n"),
            (2, "guard\nfault\n"),
        ] {
            let mut program = selection_effects(handler_errors, false, false);
            program.functions.insert(code, direct_defect());
            let mut complete = false;
            for fuel in 0..1000 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("caller".into()))
                    .expect("external root");
                let report = execute_program_outcome(
                    &mut heap,
                    &program,
                    &Name::new("Recovery", "main"),
                    vec![],
                    fuel,
                );
                assert!(expected_stdout.starts_with(&report.stdout), "{report:?}");
                match report.outcome {
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    outcome => {
                        causes::assert_defect(
                            &mut heap,
                            outcome,
                            &Error::IntDivisionByZero { remainder: false },
                        );
                        assert_eq!(report.stdout, expected_stdout);
                        complete = true;
                    }
                }
                assert_eq!(heap.string(&external), Ok("caller"));
                heap.drop_value(external).expect("caller release");
                let stats = heap
                    .finish()
                    .expect("actual empty heap for every replaced-C prefix");
                assert_eq!(stats.allocations, stats.frees);
                if complete {
                    break;
                }
            }
            assert!(complete);
        }
    }
}

#[test]
fn selected_operand_validation_and_outside_creation_release_staged_owned_captures() {
    for task in [false, true] {
        let mut program = sealed_selector(true);
        let selected = if task {
            Operation::Task {
                code: 3,
                captures: vec![1],
            }
        } else {
            Operation::Literal(Lit::Int(42))
        };
        program.functions.insert(
            2,
            function(
                &[0, 1],
                vec![
                    bind(2, selected),
                    bind(3, Operation::RecoveryResult { selected: Some(2) }),
                ],
                3,
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
            complete = fault_or_fuel(&mut heap, report.outcome, &Error::ExpectedClosure);
            heap.finish()
                .expect("C and staged invalid selected edges actually released");
            if complete {
                break;
            }
        }
        assert!(complete);
    }
}

#[test]
fn outside_creation_releases_staged_selected_closure_with_owned_input_capture() {
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
                        code: 3,
                        captures: vec![1],
                    },
                ),
                bind(3, Operation::RecoveryResult { selected: Some(2) }),
            ],
            3,
        ),
    );
    let mut complete = false;
    for fuel in 0..1000 {
        let mut heap = Heap::new();
        let input = heap
            .literal(&Lit::String("external input".into()))
            .expect("owned argument");
        let external = heap.dup(&input).expect("caller alias");
        let report = execute_program_outcome(
            &mut heap,
            &program,
            &Name::new("Recovery", "main"),
            vec![input],
            fuel,
        );
        complete = fault_or_fuel(&mut heap, report.outcome, &Error::RecoveryOutsideSelection);
        assert_eq!(heap.reference_count(&external), Ok(1));
        heap.drop_value(external).expect("caller alias release");
        heap.finish()
            .expect("staged closure's argument capture actually released");
        if complete {
            break;
        }
    }
    assert!(complete);
}
