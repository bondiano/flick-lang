#[test]
fn compound_first_fail_reaches_recovery_selection_before_final_reporting() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let selector = function(
        vec![0, 1],
        vec![
            bind(
                2,
                Operation::Intrinsic {
                    kind: Intrinsic::Show,
                    args: vec![1],
                },
            ),
            bind(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![2],
                },
            ),
            bind(
                4,
                Operation::Closure {
                    code: 2,
                    captures: vec![],
                },
            ),
            bind(5, Operation::RecoveryResult { selected: Some(4) }),
        ],
        5,
    );
    let caller = function(vec![], vec![bind(0, Operation::Literal(Lit::Unit))], 0);
    let program = Program {
        functions: BTreeMap::from([
            (0, caller),
            (1, selector),
            (
                2,
                function(
                    vec![0, 1],
                    vec![bind(2, Operation::Literal(Lit::Int(42)))],
                    2,
                ),
            ),
        ]),
        globals: vec![],
        op_spans: BTreeMap::new(),
    };
    let mut heap = Heap::new();
    let original = heap
        .cause(
            &[Reason::Fail(&Value::Int(1)), Reason::Fail(&Value::Int(2))],
            &mut 20,
        )
        .expect("private represented compound injection only");
    let selector = heap.allocate(Data::Closure {
        execution: 0,
        code: 1,
        captures: vec![],
    });
    let mut machine = fixture_machine(&program, &mut heap);
    let block = &program.functions[&0].body;
    let push_frame = |at, destination| Frame {
        recovery_id: None,
        evidence: None,
        block,
        at,
        locals: Locals::new(),
        vars: vec![],
        iterators: vec![],
        destination,
    };
    machine
        .frames
        .push(push_frame(block.instructions.len(), Destination::Top));
    machine.frames.push(push_frame(
        0,
        Destination::Catch {
            handler: selector,
            binding: 0,
        },
    ));
    machine.failure = Some(original);
    let result = machine.run();
    let outcome = machine.finish(result);
    let output = machine.stdout.clone();
    assert!(
        matches!(outcome, ProgramOutcome::Returned(Value::Int(42))),
        "{outcome:?}"
    );
    outcome
        .release(machine.heap)
        .expect("every owned result released");
    machine
        .heap
        .finish()
        .expect("actual heap0 before eligibility assertion");
    assert_eq!(
        output, "1\n",
        "first eligible Fail must reach selector once, despite secondary reasons"
    );
}
fn assert_compound_miss(heap: &Heap, cause: &CauseToken, identity: &CauseToken, interrupt: bool) {
    assert_eq!(cause, identity, "Miss must not rebuild C");
    let actual = heap
        .cause_reasons(cause)
        .expect("diagnostics")
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), if interrupt { 3 } else { 2 });
    let offset = usize::from(interrupt);
    if interrupt {
        assert!(matches!(
            actual[0],
            Reason::Interrupt { fiber_id: Some(37) }
        ));
    }
    for (reason, expected) in actual[offset..].iter().zip(["first", "second"]) {
        let Reason::Fail(value) = reason else {
            panic!("ordered Fail")
        };
        assert_eq!(heap.string(value).expect("payload"), expected);
    }
}

fn release_compound_aliases(heap: &mut Heap, identity: CauseToken, first: Value, second: Value) {
    heap.drop_cause(identity).expect("external cause release");
    assert_eq!(
        heap.string(&first).expect("external first survives"),
        "first"
    );
    assert_eq!(
        heap.string(&second).expect("external second survives"),
        "second"
    );
    heap.drop_values(vec![first, second])
        .expect("external values release");
    let stats = heap.finish().expect("actual empty heap at every prefix");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn compound_miss_keeps_identity_order_interrupt_metadata_and_all_prefix_roots() {
    for interrupt in [false, true] {
        let bind = |binding, value| Instruction::Let { binding, value };
        let selector = function(
            vec![0, 1],
            vec![bind(2, Operation::RecoveryResult { selected: None })],
            2,
        );
        let caller = function(vec![], vec![bind(0, Operation::Literal(Lit::Unit))], 0);
        let program = Program {
            functions: BTreeMap::from([(0, caller), (1, selector)]),
            globals: vec![],
        op_spans: BTreeMap::new(),
        };
        let mut complete = false;
        for fuel in 0..1000 {
            let mut heap = Heap::new();
            let first = heap
                .literal(&Lit::String("first".into()))
                .expect("external first");
            let second = heap
                .literal(&Lit::String("second".into()))
                .expect("external second");
            let mut reasons = vec![Reason::Fail(&first), Reason::Fail(&second)];
            if interrupt {
                reasons.insert(0, Reason::Interrupt { fiber_id: Some(37) });
            }
            let original = heap
                .cause(&reasons, &mut 100)
                .expect("private representation injection");
            let identity = heap.dup_cause(&original).expect("external cause alias");
            let selector = heap.allocate(Data::Closure {
                execution: 0,
                code: 1,
                captures: vec![],
            });
            let mut machine = fixture_machine(&program, &mut heap);
            machine.fuel = fuel;
            let block = &program.functions[&0].body;
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
            machine.failure = Some(original);
            let result = machine.run();
            let outcome = machine.finish(result);
            complete = match &outcome {
                ProgramOutcome::Fault(Error::OutOfFuel) => false,
                ProgramOutcome::Failed(cause) => {
                    assert_compound_miss(machine.heap, cause, &identity, interrupt);
                    true
                }
                other => {
                    let diagnostic = format!("compound Miss must remain Failed: {other:?}");
                    unexpected_outcome(machine.heap, outcome, &diagnostic)
                }
            };
            outcome.release(machine.heap).expect("outcome release");
            release_compound_aliases(machine.heap, identity, first, second);
            if complete {
                break;
            }
        }
        assert!(complete, "actual compound Miss reached");
    }
}

mod recovery_integrity {
    include!("../../recovery_tests.rs");
}

mod recovery_timing {
    use super::*;
    pub(super) mod effects {
        include!("recovery_effects.rs");
    }
    use effects::selection_effects;
    #[test]
    fn original_cause_is_parked_for_tail_but_released_before_abort_callback_and_body() {
        for handler_errors in [false, true] {
            for (abort, arm_fails) in [(false, false), (true, false), (false, true)] {
                let program = selection_effects(handler_errors, abort, arm_fails);
                let mut heap = Heap::new();
                let mut machine = fixture_machine(&program, &mut heap);
                machine
                    .global(&Name::new("Recovery", "main"), None)
                    .expect("start");
                let mut saw_callback = false;
                let mut saw_body = false;
                for _ in 0..1000 {
                    if machine.failure.is_some() && !machine.failure_elected {
                        machine.prepare_failure().expect("elect");
                    }
                    if machine.frames.is_empty()
                        && machine.discard.is_none()
                        && machine.ready.is_none()
                    {
                        break;
                    }
                    if machine.ready.is_some() && !saw_callback {
                        saw_callback = true;
                        let causes = machine
                            .heap
                            .objects
                            .iter()
                            .filter_map(|slot| slot.object.as_ref())
                            .filter(|object| matches!(object.data, Data::Cause(_)))
                            .count();
                        assert_eq!(
                            machine.recoveries.len(),
                            usize::from(!abort),
                            "selection state before callback"
                        );
                        assert_eq!(
                            causes,
                            usize::from(!abort),
                            "C released BEFORE Abort callback, alive through Tail callback"
                        );
                    }
                    if machine.frames.last().is_some_and(|frame| {
                        matches!(frame.destination, Destination::RecoveryBody { .. })
                    }) {
                        saw_body = true;
                        assert!(
                            machine.recoveries.is_empty(),
                            "no obsolete selector state in body"
                        );
                        assert!(
                            !machine
                                .heap
                                .objects
                                .iter()
                                .filter_map(|slot| slot.object.as_ref())
                                .any(|object| matches!(object.data, Data::Cause(_))),
                            "C released BEFORE selected body"
                        );
                    }
                    machine.fuel -= 1;
                    if machine.discard.is_some() {
                        machine.discard_step().expect("discard");
                    } else if let Some(id) = machine.ready.take() {
                        machine.invoke_step(id).expect("callback");
                    } else if machine.failure.is_some() {
                        machine.unwind().expect("unwind");
                    } else {
                        machine.step().expect("ordinary instruction");
                    }
                }
                assert!(saw_callback);
                assert_eq!(saw_body, !abort && !arm_fails);
                let result = machine.run();
                let outcome = machine.finish(result);
                assert!(!matches!(outcome, ProgramOutcome::Fault(_)), "{outcome:?}");
                outcome
                    .release(machine.heap)
                    .expect("caller outcome release");
                let stats = machine.heap.finish().expect("actual no occupied roots");
                assert_eq!(stats.allocations, stats.frees);
            }
        }
    }
}
