#[test]
fn malformed_selected_values_reject_before_body_or_original_release() {
    for case in 0..4 {
        let program = program();
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("external".into()))
            .expect("caller root");
        let mut machine = fixture_machine(&program, &mut heap);
        start(&mut machine, &payload).expect("executing selection");
        let (selected, expected) = match case {
            0 => (Value::Int(42), Error::ExpectedClosure),
            1 => (
                machine.heap.allocate(Data::Task {
                    execution: 0,
                    code: 0,
                    captures: vec![],
                }),
                Error::ExpectedClosure,
            ),
            2 => (
                machine.heap.allocate(Data::Closure {
                    execution: 1,
                    code: 0,
                    captures: vec![],
                }),
                Error::ForeignClosure,
            ),
            3 => (
                machine.heap.allocate(Data::Closure {
                    execution: 0,
                    code: u32::MAX,
                    captures: vec![],
                }),
                Error::InvalidProgram(memory::Error::UnknownCode(u32::MAX)),
            ),
            _ => unreachable!(),
        };
        machine.pending = Some(machine.heap.allocate(Data::RecoveryResult {
            execution: 0,
            recovery_id: 0,
            selected: Some(selected),
        }));
        let error = machine
            .complete_recovery(0)
            .expect_err("selected edge is not a callable Unit body");
        assert_eq!(error, expected);
        assert_eq!(
            machine.recoveries.len(),
            1,
            "original C not released on validation failure"
        );
        assert!(
            machine.pending.is_some(),
            "owned selected edge remains rooted"
        );
        finish_fault(&mut machine, payload, error);
    }
}

#[test]
fn malformed_middle_arena_root_does_not_skip_valid_later_owned_arenas() {
    let program = program();
    let mut heap = Heap::new();
    let payload = heap
        .literal(&Lit::String("external".into()))
        .expect("caller root");
    let mut machine = fixture_machine(&program, &mut heap);
    start(&mut machine, &payload).expect("executing selection");
    for wrong in [false, true, false] {
        let root = if wrong {
            machine
                .heap
                .literal(&Lit::String("wrong arena".into()))
                .expect("owned malformed layout")
        } else {
            let retained = machine
                .heap
                .dup(&payload)
                .expect("valid accepted owned arena slot");
            machine.heap.allocate(Data::Arena {
                execution: 0,
                contents: vec![retained],
            })
        };
        machine.owners[0].arenas.push(root);
    }
    finish_fault(&mut machine, payload, Error::ExpectedArena);
    assert_eq!(machine.heap.stats().arenas_allocated, 2);
    assert_eq!(machine.heap.stats().arenas_freed, 2);
    assert!(machine.owners[0].closed);
    machine
        .close_owner(0)
        .expect("retirement is still idempotent");
}

#[test]
fn none_marker_creation_never_steals_an_unrelated_existing_scratch_root() {
    let program = program();
    let mut heap = Heap::new();
    let payload = heap
        .literal(&Lit::String("external".into()))
        .expect("caller root");
    let extra = heap
        .literal(&Lit::String("unrelated".into()))
        .expect("existing root");
    let external_extra = heap.dup(&extra).expect("caller alias of unrelated root");
    let mut machine = fixture_machine(&program, &mut heap);
    start(&mut machine, &payload).expect("executing selection");
    machine.scratch.push(extra);
    machine
        .recovery_result(2, None)
        .expect("None result under actual executing authority");
    let marker =
        peek(&machine.frames.last().expect("selector frame").locals, 2).expect("owned marker");
    let Value::Object(token) = marker else {
        panic!("marker object")
    };
    let Data::RecoveryResult { selected, .. } =
        &machine.heap.object(token.id).expect("marker data").data
    else {
        panic!("sealed marker")
    };
    let is_miss = selected.is_none();
    let scratch_roots = machine.scratch.len();
    let outcome = machine.finish(Err(Error::OutOfFuel));
    assert!(matches!(outcome, ProgramOutcome::Fault(Error::OutOfFuel)));
    outcome.release(machine.heap).expect("outcome release");
    assert_eq!(machine.heap.reference_count(&payload), Ok(1));
    assert_eq!(machine.heap.reference_count(&external_extra), Ok(1));
    machine
        .heap
        .drop_values(vec![payload, external_extra])
        .expect("caller releases external aliases");
    machine
        .heap
        .finish()
        .expect("actual empty heap before semantic assertions");
    assert!(is_miss, "None must not become Some(unrelated scratch root)");
    assert_eq!(
        scratch_roots, 1,
        "builder preserves previously owned scratch roots"
    );
}

#[test]
fn borrowed_foreign_payload_rolls_back_only_accepted_local_prefix_edges() {
    let mut complete = false;
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let mut foreign_heap = Heap::new();
        let local = heap
            .literal(&Lit::String("local".into()))
            .expect("caller local root");
        let foreign = foreign_heap
            .literal(&Lit::String("foreign".into()))
            .expect("caller foreign root");
        let id = foreign.object_id().expect("diagnostic identity only");
        let error = heap
            .cause(&[Reason::Fail(&local), Reason::Die(&foreign)], &mut {
                fuel
            })
            .expect_err("foreign payload is not admitted");
        complete = error == Error::InvalidObject(id);
        assert!(complete || error == Error::OutOfFuel, "{error:?}");
        assert_eq!(heap.reference_count(&local), Ok(1));
        assert_eq!(foreign_heap.reference_count(&foreign), Ok(1));
        heap.drop_value(local).expect("caller local release");
        foreign_heap
            .drop_value(foreign)
            .expect("caller releases foreign root in ITS heap");
        heap.finish().expect("accepted local prefix fully drained");
        foreign_heap
            .finish()
            .expect("no foreign heap collection or lost ownership");
        if complete {
            break;
        }
    }
    assert!(complete);
}

#[test]
fn typed_guard_failure_boundary_restores_saved_default_and_lexical_evidence() {
    let program = program();
    let mut heap = Heap::new();
    let payload = heap
        .literal(&Lit::String("external".into()))
        .expect("caller root");
    let mut machine = fixture_machine(&program, &mut heap);
    machine
        .owners
        .extend([OwnerState::default(), OwnerState::default()]);
    machine.current_owner = 1;
    let saved_evidence = Some(OwnerEvidence::from_owner(&machine.owner_handle(2)));
    machine.current_evidence = saved_evidence;
    start(&mut machine, &payload).expect("executing guard saves distinct D and C");
    machine.current_owner = 2;
    machine.current_evidence = None;
    machine.failure = Some(
        machine
            .heap
            .cause(&[Reason::Fail(&Value::Int(99))], &mut 10)
            .expect("fresh ordinary guard failure D"),
    );
    machine.failure_elected = false;
    machine
        .unwind()
        .expect("cross actual RecoverySelection boundary");
    assert_eq!(machine.current_owner, 1);
    let key = |e: OwnerEvidence| (e.heap, e.execution, e.index);
    assert_eq!(machine.current_evidence.map(key), saved_evidence.map(key));
    assert!(
        machine.recoveries.is_empty(),
        "obsolete original C state removed once"
    );
    assert_eq!(
        machine.heap.reference_count(&payload),
        Ok(1),
        "original C released before further unwinding"
    );
    let result = machine.run();
    let outcome = machine.finish(result);
    assert!(
        matches!(outcome, ProgramOutcome::Raised(Value::Int(99))),
        "D replaces C, not concatenation: {outcome:?}"
    );
    outcome
        .release(machine.heap)
        .expect("owned D outcome release");
    machine
        .heap
        .drop_value(payload)
        .expect("external caller release");
    machine.heap.finish().expect("actual empty heap");
}

fn ordinary_tick(machine: &mut Machine<'_, '_>) {
    assert!(machine.fuel > 0);
    machine.fuel -= 1;
    if machine.discard.is_some() {
        machine.discard_step().expect("discard boundary");
    } else if let Some(id) = machine.ready.take() {
        machine.invoke_step(id).expect("callback step");
    } else if machine.failure.is_some() {
        machine.unwind().expect("unwind boundary");
    } else {
        machine.step().expect("ordinary instruction");
    }
}

#[test]
fn tailfailure_discards_old_selection_before_the_outer_recovery_callback() {
    use super::recovery_timing::effects::tail_failure_outer_recovery;
    for handler_errors in [false, true] {
        let program = tail_failure_outer_recovery(handler_errors);
        let mut heap = Heap::new();
        let mut machine = fixture_machine(&program, &mut heap);
        machine
            .global(&Name::new("Recovery", "main"), None)
            .expect("start valid MIR");
        let mut old_cause = None;
        let mut saw_outer_selection = false;
        for _ in 0..1000 {
            if machine.failure.is_some() && !machine.failure_elected {
                machine.prepare_failure().expect("elect");
            }
            if machine.frames.is_empty() && machine.discard.is_none() && machine.ready.is_none() {
                break;
            }
            if old_cause.is_none() && machine.recoveries.contains_key(&0) {
                old_cause = Some(
                    machine
                        .heap
                        .objects
                        .iter()
                        .enumerate()
                        .find_map(|(index, slot)| {
                            slot.object
                                .as_ref()
                                .filter(|object| matches!(object.data, Data::Cause(_)))
                                .map(|_| ObjectId {
                                    heap: machine.heap.identity,
                                    index,
                                    generation: slot.generation,
                                })
                        })
                        .expect("original C"),
                );
            }
            if machine.frames.last().is_some_and(|frame| {
                matches!(frame.destination, Destination::RecoverySelection { id: 1 })
            }) {
                saw_outer_selection = true;
                assert!(
                    !machine.heap.is_live(old_cause.expect("old C seen")),
                    "old C released BEFORE outer recovery callback"
                );
                assert!(!machine.recoveries.contains_key(&0));
                assert_eq!(
                    machine.recoveries.len(),
                    1,
                    "only fresh D selection is live"
                );
            }
            ordinary_tick(&mut machine);
        }
        assert!(saw_outer_selection);
        let result = machine.run();
        let outcome = machine.finish(result);
        let ProgramOutcome::Returned(value) = &outcome else {
            unexpected_outcome(
                machine.heap,
                outcome,
                "expected returned outer recovery result",
            )
        };
        assert_eq!(machine.heap.string(value), Ok("replacement"));
        assert_eq!(machine.stdout, "guard\narm\nouter\n");
        assert_eq!(machine.current_owner, 0);
        assert!(machine.current_evidence.is_none());
        outcome
            .release(machine.heap)
            .expect("owned outcome release");
        let stats = machine.heap.finish().expect("actual empty heap");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn cached_global_marker_is_rejected_even_with_matching_live_selection_identity() {
    let program = program();
    let mut heap = Heap::new();
    let payload = heap
        .literal(&Lit::String("external".into()))
        .expect("caller root");
    let mut machine = fixture_machine(&program, &mut heap);
    start(&mut machine, &payload).expect("executing selection");
    let retained = machine
        .heap
        .dup(&payload)
        .expect("selected closure owns a real payload alias");
    let selected = machine.heap.allocate(Data::Closure {
        execution: 0,
        code: 0,
        captures: vec![retained],
    });
    let marker = machine.heap.allocate(Data::RecoveryResult {
        execution: 0,
        recovery_id: 0,
        selected: Some(selected),
    });
    let name = Name::new("Malformed", "cached");
    machine.globals.insert(name.clone(), Some(marker));
    let error = machine
        .global(&name, Some(2))
        .expect_err("ordinary global cache cannot transport markers");
    assert_eq!(error, Error::RecoveryEscape);
    finish_fault(&mut machine, payload, error);
}

#[test]
fn cleanup_keeps_first_layout_error_while_draining_a_later_invalid_global() {
    let program = program();
    let mut heap = Heap::new();
    let payload = heap
        .literal(&Lit::String("external".into()))
        .expect("caller root");
    let mut machine = fixture_machine(&program, &mut heap);
    start(&mut machine, &payload).expect("executing selection");
    let wrong = machine
        .heap
        .literal(&Lit::String("wrong arena layout".into()))
        .expect("owned root");
    machine.owners[0].arenas.push(wrong);
    let invalid = ObjectId {
        heap: machine.heap.identity,
        index: usize::MAX,
        generation: 0,
    };
    machine.globals.insert(
        Name::new("Broken", "later"),
        Some(Value::Object(ObjectToken { id: invalid })),
    );
    finish_fault(&mut machine, payload, Error::ExpectedArena);
}

#[test]
fn guard_failure_with_same_payload_is_fresh_d_not_a_miss_on_original_c() {
    let program = Program {
        globals: vec![],
        functions: BTreeMap::from([(
            0,
            function(
                vec![0, 1],
                vec![Instruction::Let {
                    binding: 2,
                    value: Operation::Intrinsic {
                        kind: Intrinsic::Raise,
                        args: vec![1],
                    },
                }],
                2,
            ),
        )]),
    };
    let mut complete = false;
    for fuel in 0..1000 {
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("same payload".into()))
            .expect("external payload");
        let original = heap
            .cause(&[Reason::Fail(&payload), Reason::Fail(&payload)], &mut 20)
            .expect("private compound representation injection");
        let identity = heap
            .dup_cause(&original)
            .expect("external C alias for exact identity observation");
        let mut machine = fixture_machine(&program, &mut heap);
        install_injected_catch(&mut machine, original);
        machine.fuel = fuel;
        let result = machine.run();
        match &result {
            Err(Error::OutOfFuel) => {}
            Ok(LanguageExit::Failure(cause)) => {
                assert_ne!(
                    cause, &identity,
                    "guard D is fresh even when its payload is identical"
                );
                let reasons = machine
                    .heap
                    .cause_reasons(cause)
                    .expect("borrowed D")
                    .collect::<Vec<_>>();
                assert!(
                    matches!(reasons.as_slice(), [Reason::Fail(value)] if value.object_id() == payload.object_id())
                );
            }
            _ => {
                let outcome = machine.finish(result);
                unexpected_outcome(machine.heap, outcome, "ordinary guard failure D expected");
            }
        }
        let outcome = machine.finish(result);
        match &outcome {
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            ProgramOutcome::Raised(value) => {
                complete = true;
                assert_eq!(value.object_id(), payload.object_id());
            }
            other => {
                let diagnostic = format!(
                    "fresh singleton D canonicalizes independently of original compound C: {other:?}"
                );
                unexpected_outcome(machine.heap, outcome, &diagnostic);
            }
        }
        outcome
            .release(machine.heap)
            .expect("owned outcome release");
        machine
            .heap
            .drop_cause(identity)
            .expect("external original C alias release");
        assert_eq!(machine.heap.reference_count(&payload), Ok(1));
        machine
            .heap
            .drop_value(payload)
            .expect("external payload release");
        let stats = machine
            .heap
            .finish()
            .expect("actual empty heap at every prefix");
        assert_eq!(stats.allocations, stats.frees);
        if complete {
            break;
        }
    }
    assert!(complete);
}
