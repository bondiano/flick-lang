#[test]
fn ordinary_store_and_entry_reject_markers_without_losing_owned_selected_edges() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let program = Program {
        globals: vec![memory::Global {
            name: Name::new("Root", "main"),
            initializer: 0,
        }],
        functions: BTreeMap::from([(
            0,
            function(vec![], vec![bind(0, Operation::Literal(Lit::Unit))], 0),
        )]),
        op_spans: BTreeMap::new(),
    };
    for entry in [false, true] {
        for fuel in 0..3 {
            let mut heap = Heap::new();
            let selected = heap
                .literal(&Lit::String("external selected edge".into()))
                .expect("original caller edge");
            let external = heap
                .dup(&selected)
                .expect("external alias of selected edge");
            // Private malformed representation injection; no public marker factory.
            let marker = heap.allocate(Data::RecoveryResult {
                execution: 7,
                recovery_id: 8,
                selected: Some(selected),
            });
            if entry {
                let report = execute_program_outcome(
                    &mut heap,
                    &program,
                    &Name::new("Root", "main"),
                    vec![marker],
                    fuel,
                );
                assert!(
                    matches!(report.outcome, ProgramOutcome::Fault(Error::RecoveryEscape)),
                    "{:?}",
                    report.outcome
                );
                report
                    .outcome
                    .release(&mut heap)
                    .expect("explicit outcome release");
            } else {
                let mut machine = fixture_machine(&program, &mut heap);
                machine.fuel = fuel;
                let result = machine.store(0, marker);
                assert_eq!(result, Err(Error::RecoveryEscape));
                assert_eq!(
                    machine.scratch.len(),
                    1,
                    "owned rejected marker stays machine-rooted"
                );
                let outcome = machine.finish(result.map(|()| LanguageExit::Success(Value::Unit)));
                assert!(matches!(
                    outcome,
                    ProgramOutcome::Fault(Error::RecoveryEscape)
                ));
                outcome.release(machine.heap).expect("outcome release");
            }
            assert_eq!(heap.string(&external), Ok("external selected edge"));
            assert_eq!(heap.reference_count(&external), Ok(1));
            heap.drop_value(external).expect("caller release");
            let stats = heap
                .finish()
                .expect("rejected marker releases its selected edge exactly once");
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn recovery_identity_overflow_keeps_original_and_selector_rooted_until_cleanup() {
    let program = Program::default();
    let mut complete = false;
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("external".into()))
            .expect("caller edge");
        let cause = heap
            .cause(&[Reason::Fail(&payload)], &mut 10)
            .expect("original C");
        let selector = heap.allocate(Data::Closure {
            execution: 0,
            code: 0,
            captures: vec![],
        });
        let mut machine = fixture_machine(&program, &mut heap);
        machine.next_recovery = usize::MAX;
        machine.pending_handler = Some(selector);
        machine.failure = Some(cause);
        machine.fuel = fuel;
        let result = machine.begin_recovery(0);
        complete = result == Err(Error::RecoveryIdentityOverflow);
        assert!(complete || result == Err(Error::OutOfFuel), "{result:?}");
        assert_eq!(machine.next_recovery, usize::MAX, "no wrapped identity");
        assert!(machine.failure.is_some());
        assert!(machine.pending_handler.is_some());
        assert!(machine.recoveries.is_empty());
        let outcome = machine.finish(result.map(|()| LanguageExit::Success(Value::Unit)));
        assert!(matches!(
            outcome,
            ProgramOutcome::Fault(Error::RecoveryIdentityOverflow | Error::OutOfFuel)
        ));
        outcome.release(machine.heap).expect("outcome release");
        assert_eq!(machine.heap.string(&payload), Ok("external"));
        assert_eq!(machine.heap.reference_count(&payload), Ok(1));
        machine.heap.drop_value(payload).expect("caller release");
        machine.heap.finish().expect("all original roots released");
        if complete {
            break;
        }
    }
    assert!(complete, "actual identity overflow reached");
}
fn interrupted_recovery_cleanup_keeps_no_accepted_owned_roots(mode: u8) {
    let program = Program {
        globals: vec![],
        functions: BTreeMap::from([(0, function(vec![0, 1], vec![], 1))]),
        op_spans: BTreeMap::new(),
    };
    let mut heap = Heap::new();
    let external = heap
        .literal(&Lit::String("external payload".into()))
        .expect("caller edge");
    let cause = heap
        .cause(&[Reason::Fail(&external)], &mut 10)
        .expect("original C");
    let selector = heap.allocate(Data::Closure {
        execution: 0,
        code: 0,
        captures: vec![],
    });
    let mut machine = fixture_machine(&program, &mut heap);
    machine.failure = Some(cause);
    machine.pending_handler = Some(selector);
    machine
        .begin_recovery(0)
        .expect("executing selection owns whole C");
    assert_eq!(machine.recoveries.len(), 1);
    let invalid = ObjectId {
        heap: machine.heap.identity,
        index: usize::MAX,
        generation: 0,
    };
    let expected = match mode {
        0 => {
            machine.globals.insert(
                Name::new("Broken", "root"),
                Some(Value::Object(ObjectToken { id: invalid })),
            );
            Error::InvalidObject(invalid)
        }
        1 => {
            let wrong = machine
                .heap
                .literal(&Lit::String("wrong arena layout".into()))
                .expect("owned corrupt root");
            machine.owners[0].arenas.push(wrong);
            Error::ExpectedArena
        }
        2 => {
            let wrong = machine
                .heap
                .literal(&Lit::String("wrong cell layout".into()))
                .expect("owned corrupt root");
            machine.cells.push(wrong);
            Error::ExpectedCell
        }
        _ => unreachable!(),
    };
    let abort_fuel = machine.fuel;
    let outcome = machine.finish(Err(Error::OutOfFuel));
    assert_eq!(
        machine.fuel, abort_fuel,
        "integrity cleanup gains no extra abort fuel"
    );
    assert!(
        matches!(&outcome, ProgramOutcome::Fault(error) if error == &expected),
        "{outcome:?}"
    );
    outcome.release(machine.heap).expect("caller release");
    assert!(machine.recoveries.is_empty());
    assert_eq!(machine.heap.string(&external), Ok("external payload"));
    assert_eq!(
        machine.heap.reference_count(&external),
        Ok(1),
        "all accepted roots including original C must drain despite a later integrity error"
    );
    machine
        .heap
        .drop_value(external)
        .expect("external caller release");
    let stats = machine
        .heap
        .finish()
        .expect("actual heap empty despite malformed root");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn malformed_global_cleanup_still_releases_owned_recovery_cause() {
    interrupted_recovery_cleanup_keeps_no_accepted_owned_roots(0);
}
#[test]
fn malformed_arena_layout_cleanup_still_releases_owned_recovery_cause() {
    interrupted_recovery_cleanup_keeps_no_accepted_owned_roots(1);
}
#[test]
fn malformed_cell_layout_cleanup_still_releases_owned_recovery_cause() {
    interrupted_recovery_cleanup_keeps_no_accepted_owned_roots(2);
}
