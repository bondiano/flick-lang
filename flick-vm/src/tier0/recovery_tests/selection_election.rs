fn program() -> Program {
    Program {
        globals: vec![],
        functions: BTreeMap::from([(0, function(vec![0, 1], vec![], 1))]),
        op_spans: BTreeMap::new(),
    }
}

fn start(machine: &mut Machine<'_, '_>, payload: &Value) -> Result<(), Error> {
    machine.failure = Some(machine.heap.cause(&[Reason::Fail(payload)], &mut 10)?);
    machine.pending_handler = Some(machine.heap.allocate(Data::Closure {
        execution: machine.execution,
        code: 0,
        captures: vec![],
    }));
    machine.begin_recovery(0)
}

fn finish_fault(machine: &mut Machine<'_, '_>, payload: Value, error: Error) {
    let fuel = machine.fuel;
    let expected = format!("{error:?}");
    let outcome = machine.finish(Err(error));
    let ProgramOutcome::Fault(actual) = &outcome else {
        unexpected_outcome(
            machine.heap,
            outcome,
            "expected exact Fault after integrity rejection",
        )
    };
    assert_eq!(
        format!("{actual:?}"),
        expected,
        "preserve the exact already-asserted evaluator error"
    );
    outcome.release(machine.heap).expect("outcome release");
    assert_eq!(machine.fuel, fuel, "abort has no extra fuel grant");
    assert_eq!(machine.heap.reference_count(&payload), Ok(1));
    machine
        .heap
        .drop_value(payload)
        .expect("external caller release");
    let stats = machine
        .heap
        .finish()
        .expect("every accepted owned edge drained");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn mismatching_execution_or_selection_results_stay_rooted_on_every_rejection() {
    for foreign_execution in [false, true] {
        for transport in [false, true] {
            let program = program();
            let mut heap = Heap::new();
            let payload = heap
                .literal(&Lit::String("external".into()))
                .expect("external root");
            let mut machine = fixture_machine(&program, &mut heap);
            start(&mut machine, &payload).expect("executing selector");
            let marker = machine.heap.allocate(Data::RecoveryResult {
                execution: usize::from(foreign_execution),
                recovery_id: usize::from(!foreign_execution),
                selected: None,
            });
            let error = if transport {
                let error = machine
                    .store_transport(2, marker, 0)
                    .expect_err("foreign marker cannot transport");
                assert_eq!(machine.scratch.len(), 1, "failed transport remains rooted");
                error
            } else {
                machine.pending = Some(marker);
                let error = machine
                    .complete_recovery(0)
                    .expect_err("foreign marker cannot select");
                assert!(machine.pending.is_some(), "rejected result remains rooted");
                error
            };
            assert_eq!(error, Error::ForeignRecoveryResult);
            assert_eq!(machine.recoveries.len(), 1, "original C remains owned");
            finish_fault(&mut machine, payload, error);
        }
    }
}

#[test]
fn selector_argument_duplicate_overflow_has_safe_roots_at_every_fuel_prefix() {
    let program = program();
    let mut complete = false;
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("external".into()))
            .expect("external root");
        let id = payload.object_id().expect("diagnostic identity");
        let mut machine = fixture_machine(&program, &mut heap);
        machine.failure = Some(
            machine
                .heap
                .cause(&[Reason::Fail(&payload)], &mut 10)
                .expect("C"),
        );
        machine.pending_handler = Some(machine.heap.allocate(Data::Closure {
            execution: 0,
            code: 0,
            captures: vec![],
        }));
        machine
            .heap
            .object_mut(id)
            .expect("private overflow fixture")
            .references = usize::MAX;
        machine.fuel = fuel;
        let error = machine
            .begin_recovery(0)
            .expect_err("forced overflow or earlier fuel exhaustion");
        complete = error == Error::ReferenceOverflow(id);
        assert!(complete || error == Error::OutOfFuel, "{error:?}");
        // Restore the real two owned references before testing actual teardown.
        machine
            .heap
            .object_mut(id)
            .expect("restore counted fixture")
            .references = 2;
        finish_fault(&mut machine, payload, error);
        if complete {
            break;
        }
    }
    assert!(complete, "actual duplicate overflow reached");
}

#[test]
fn selected_body_duplicate_overflow_does_not_release_original_or_marker_early() {
    let program = program();
    let mut complete = false;
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("external".into()))
            .expect("external root");
        let mut machine = fixture_machine(&program, &mut heap);
        start(&mut machine, &payload).expect("executing selector");
        let selected = machine.heap.allocate(Data::Closure {
            execution: 0,
            code: 0,
            captures: vec![],
        });
        let id = selected.object_id().expect("selected closure identity");
        machine.pending = Some(machine.heap.allocate(Data::RecoveryResult {
            execution: 0,
            recovery_id: 0,
            selected: Some(selected),
        }));
        machine
            .heap
            .object_mut(id)
            .expect("overflow fixture")
            .references = usize::MAX;
        machine.fuel = fuel;
        let error = machine
            .complete_recovery(0)
            .expect_err("forced selected-body duplicate overflow");
        complete = error == Error::ReferenceOverflow(id);
        assert!(complete || error == Error::OutOfFuel, "{error:?}");
        assert_eq!(machine.recoveries.len(), 1);
        assert!(machine.pending.is_some());
        machine
            .heap
            .object_mut(id)
            .expect("restore one real marker-owned edge")
            .references = 1;
        finish_fault(&mut machine, payload, error);
        if complete {
            break;
        }
    }
    assert!(complete, "actual selected-body duplicate overflow reached");
}
#[test]
fn partial_cause_builder_rejects_marker_after_prefix_without_retaining_originals() {
    let mut complete = false;
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("external".into()))
            .expect("external root");
        let marker = heap.allocate(Data::RecoveryResult {
            execution: 0,
            recovery_id: 0,
            selected: None,
        });
        let error = heap
            .cause(&[Reason::Fail(&payload), Reason::Fail(&marker)], &mut {
                fuel
            })
            .expect_err("opaque edge is not a typed failure payload");
        complete = error == Error::RecoveryEscape;
        assert!(complete || error == Error::OutOfFuel, "{error:?}");
        assert_eq!(
            heap.reference_count(&payload),
            Ok(1),
            "partial builder duplicates rolled back"
        );
        assert_eq!(
            heap.reference_count(&marker),
            Ok(1),
            "borrowed opaque input remains caller-owned"
        );
        heap.drop_values(vec![payload, marker])
            .expect("explicit caller releases");
        let stats = heap.finish().expect("actual empty heap at every prefix");
        assert_eq!(stats.allocations, stats.frees);
        if complete {
            break;
        }
    }
    assert!(complete);
}

fn install_injected_catch(machine: &mut Machine<'_, '_>, cause: CauseToken) {
    let selector = machine.heap.allocate(Data::Closure {
        execution: 0,
        code: 0,
        captures: vec![],
    });
    let block = &machine.program.functions[&0].body;
    machine.frames.push(Frame {
        recovery_id: None,
        evidence: None,
        block,
        at: block.instructions.len(),
        locals: Locals::new(),
        vars: vec![],
        iterators: vec![],
        destination: Destination::Top,
    });
    machine.frames.push(Frame {
        recovery_id: None,
        evidence: None,
        block,
        at: 0,
        locals: Locals::new(),
        vars: vec![],
        iterators: vec![],
        destination: Destination::Catch {
            handler: selector,
            binding: 0,
        },
    });
    machine.failure = Some(cause);
}

fn injected_reasons<'a>(case: u8, first: &'a Value, second: &'a Value) -> Vec<Reason<'a>> {
    match case {
        0 => vec![
            Reason::Fail(first),
            Reason::Interrupt { fiber_id: None },
            Reason::Die(second),
            Reason::Fail(first),
        ],
        1 => vec![Reason::Die(first), Reason::Interrupt { fiber_id: Some(37) }],
        2 => vec![],
        3 => vec![Reason::Interrupt { fiber_id: None }],
        _ => unreachable!(),
    }
}

fn assert_election(
    heap: &Heap,
    cause: &CauseToken,
    original: &CauseToken,
    scenario: u8,
    first: &Value,
    second: &Value,
) {
    let actual = heap
        .cause_reasons(cause)
        .expect("structured ordered reasons")
        .collect::<Vec<_>>();
    assert_eq!(
        cause == original,
        scenario != 0,
        "only mixed Fail/Die election allocates a replacement C"
    );
    match scenario {
        0 => {
            assert_eq!(actual.len(), 4);
            assert!(
                matches!(actual[0], Reason::Die(value) if value.object_id() == first.object_id())
            );
            assert!(matches!(actual[1], Reason::Interrupt { fiber_id: None }));
            assert!(
                matches!(actual[2], Reason::Die(value) if value.object_id() == second.object_id())
            );
            assert!(
                matches!(actual[3], Reason::Die(value) if value.object_id() == first.object_id())
            );
            let old = heap
                .cause_reasons(original)
                .expect("borrowed original")
                .collect::<Vec<_>>();
            assert!(
                matches!(old[0], Reason::Fail(_)) && matches!(old[3], Reason::Fail(_)),
                "immutable original not mutated"
            );
        }
        1 => {
            assert_eq!(actual.len(), 2);
            assert!(
                matches!(actual[0], Reason::Die(value) if value.object_id() == first.object_id())
            );
            assert!(matches!(
                actual[1],
                Reason::Interrupt { fiber_id: Some(37) }
            ));
        }
        2 => assert!(actual.is_empty()),
        3 => assert!(matches!(
            actual.as_slice(),
            [Reason::Interrupt { fiber_id: None }]
        )),
        _ => unreachable!(),
    }
}

#[test]
fn private_represented_die_election_and_noeligible_causes_are_not_singleton_raises() {
    let program = program();
    for case in 0..4 {
        let mut complete = false;
        for fuel in 0..1000 {
            let mut heap = Heap::new();
            let first = heap
                .literal(&Lit::String("first".into()))
                .expect("external first");
            let second = heap
                .literal(&Lit::String("second".into()))
                .expect("external second");
            // This is private represented-cause kernel injection, NOT a new VM/source producer.
            let cause = heap
                .cause(&injected_reasons(case, &first, &second), &mut 100)
                .expect("represented C");
            let identity = heap.dup_cause(&cause).expect("external owned C alias");
            let mut machine = fixture_machine(&program, &mut heap);
            install_injected_catch(&mut machine, cause);
            machine.fuel = fuel;
            let result = machine.run();
            let outcome = machine.finish(result);
            match &outcome {
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                ProgramOutcome::Failed(cause) => {
                    complete = true;
                    assert_election(machine.heap, cause, &identity, case, &first, &second);
                }
                other => {
                    let diagnostic = format!(
                        "case={case}, fuel={fuel}: Die/noeligible must not select or canonicalize as Raised: {other:?}"
                    );
                    unexpected_outcome(machine.heap, outcome, &diagnostic)
                }
            }
            assert!(machine.stdout.is_empty(), "selection is cold");
            outcome.release(machine.heap).expect("outcome release");
            machine
                .heap
                .drop_cause(identity)
                .expect("external C release");
            assert_eq!(machine.heap.reference_count(&first), Ok(1));
            assert_eq!(machine.heap.reference_count(&second), Ok(1));
            machine
                .heap
                .drop_values(vec![first, second])
                .expect("caller releases payload aliases");
            let stats = machine
                .heap
                .finish()
                .expect("actual empty heap at every election prefix");
            assert_eq!(stats.allocations, stats.frees);
            if complete {
                break;
            }
        }
        assert!(complete, "actual outward Failed reached");
    }
}
