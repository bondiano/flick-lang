fn fixture_machine<'p, 'h>(program: &'p Program, heap: &'h mut Heap) -> Machine<'p, 'h> {
    Machine {
        heap,
        program,
        execution: 0,
        globals: BTreeMap::new(),
        cells: vec![],
        owners: vec![OwnerState::default()],
        current_owner: 0,
        current_evidence: None,
        frames: vec![],
        activations: BTreeMap::new(),
        providers: BTreeMap::new(),
        segments: BTreeMap::new(),
        invocations: BTreeMap::new(),
        discard: None,
        ready: None,
        next_handler: 0,
        arguments: VecDeque::new(),
        scratch: vec![],
        pending: None,
        failure: None,
        failure_elected: false,
        recoveries: BTreeMap::new(),
        next_recovery: 0,
        pending_handler: None,
        top: None,
        stdout: String::new(),
        fuel: 1000,
        main_stage: MainStage::Done,
    }
}

#[test]
fn language_die_does_not_commit_a_pending_modify_result() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let intrinsic = |binding, kind, args| bind(binding, Operation::Intrinsic { kind, args });
    let entry = Name::new("Main", "main");
    let program = Program {
        globals: vec![memory::Global {
            name: entry.clone(),
            initializer: 0,
        }],
        functions: BTreeMap::from([
            (
                0,
                function(
                    vec![],
                    vec![
                        bind(0, Operation::Literal(Lit::Int(7))),
                        intrinsic(1, Intrinsic::RefMake, vec![0]),
                        bind(
                            2,
                            Operation::Closure {
                                code: 1,
                                captures: vec![],
                            },
                        ),
                        intrinsic(3, Intrinsic::RefModify, vec![1, 2]),
                    ],
                    3,
                ),
            ),
            (
                1,
                function(
                    vec![0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::Int(99))),
                        bind(
                            3,
                            Operation::Construct {
                                tag: "$tuple".into(),
                                fields: vec![1, 2],
                            },
                        ),
                        intrinsic(4, Intrinsic::MatchFail, vec![]),
                    ],
                    4,
                ),
            ),
        ]),
    };
    let mut heap = Heap::new();
    let mut machine = fixture_machine(&program, &mut heap);
    machine
        .global(&entry, None)
        .expect("global initializer frame");
    for _ in 0..64 {
        charge(&mut machine.fuel, 1).expect("ordinary VM step fuel");
        machine
            .step()
            .expect("live semantic exit, not evaluator fault");
        if machine.failure.is_some() {
            break;
        }
    }
    assert!(machine.failure.is_some());
    let Value::Object(cell) = &machine.cells[0] else {
        panic!("actual cell root")
    };
    let Data::Cell { contents, .. } = &machine.heap.object(cell.id).expect("live cell").data else {
        panic!("cell storage")
    };
    assert_eq!(
        contents.as_ref(),
        Some(&Value::Int(7)),
        "abandoned pair must not commit99"
    );
    let result = machine.run();
    let outcome = machine.finish(result);
    let ProgramOutcome::Failed(cause) = &outcome else {
        panic!("owned language Die")
    };
    let Some(Reason::Die(value)) = machine.heap.cause_reasons(cause).expect("reason").next() else {
        panic!("Die")
    };
    assert_eq!(
        machine
            .heap
            .defect(value)
            .expect("structured defect")
            .code(),
        &DefectCode::NoMatch
    );
    outcome.release(machine.heap).expect("caller cause release");
    machine.heap.finish().expect("modify/cause roots drained");
}

#[test]
fn cleanup_error_replacement_releases_the_owned_failed_outcome() {
    let program = Program {
        functions: BTreeMap::new(),
        globals: vec![],
    };
    let mut heap = Heap::new();
    let external = heap
        .literal(&Lit::String("external".into()))
        .expect("caller owner");
    let cause = heap
        .cause(&[Reason::Die(&external)], &mut 10)
        .expect("owned representation");
    let mut machine = fixture_machine(&program, &mut heap);
    // Private evaluator-integrity injection; cannot be produced by valid public MIR.
    let invalid = ObjectId {
        heap: machine.heap.identity,
        index: usize::MAX,
    };
    machine.globals.insert(
        Name::new("Broken", "root"),
        Some(Value::Object(ObjectToken { id: invalid })),
    );
    let outcome = machine.finish(Ok(LanguageExit::Failure(cause)));
    assert!(matches!(outcome, ProgramOutcome::Fault(Error::InvalidObject(id)) if id == invalid));
    assert_eq!(machine.heap.string(&external), Ok("external"));
    assert_eq!(machine.heap.reference_count(&external), Ok(1));
    assert_eq!(
        machine.fuel, 1000,
        "abort cleanup receives no extra fuel and runs no callback"
    );
    machine
        .heap
        .drop_value(external)
        .expect("explicit caller release");
    let stats = machine
        .heap
        .finish()
        .expect("failed outcome edges actually released");
    assert_eq!(stats.allocations, stats.frees);
}
#[test]
fn die_retires_scope_and_provider_during_unwind_before_final_cleanup() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let intrinsic = |binding, kind, args| bind(binding, Operation::Intrinsic { kind, args });
    let closure = |binding, code| {
        bind(
            binding,
            Operation::Closure {
                code,
                captures: vec![],
            },
        )
    };
    let entry = Name::new("Main", "main");
    let program = Program {
        globals: vec![memory::Global {
            name: entry.clone(),
            initializer: 0,
        }],
        functions: BTreeMap::from([
            (
                0,
                function(
                    vec![],
                    vec![closure(0, 1), intrinsic(1, Intrinsic::ScopeRun, vec![0])],
                    1,
                ),
            ),
            (
                1,
                function(
                    vec![0, 1],
                    vec![
                        bind(2, Operation::ServiceTag("Service".into())),
                        bind(3, Operation::Literal(Lit::Int(42))),
                        bind(
                            4,
                            Operation::Record {
                                fields: vec![("value".into(), 3)],
                            },
                        ),
                        intrinsic(5, Intrinsic::LayerSucceed, vec![2, 4]),
                        closure(6, 2),
                        intrinsic(7, Intrinsic::LayerRun, vec![5, 6]),
                    ],
                    7,
                ),
            ),
            (
                2,
                function(
                    vec![0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::Unit)),
                        intrinsic(3, Intrinsic::ArenaMake, vec![2]),
                        intrinsic(4, Intrinsic::MatchFail, vec![]),
                    ],
                    4,
                ),
            ),
        ]),
    };
    let mut heap = Heap::new();
    let mut machine = fixture_machine(&program, &mut heap);
    machine
        .global(&entry, None)
        .expect("global initializer frame");
    let result = machine.run();
    assert!(machine.frames.is_empty());
    assert!(
        machine.globals.is_empty(),
        "failed initializer abandoned during unwind"
    );
    assert!(
        machine.providers.is_empty(),
        "departed provider released during unwind"
    );
    assert!(machine.owners[1..].iter().all(|owner| owner.closed));
    assert_eq!(machine.current_owner, 0);
    assert!(machine.current_evidence.is_none());
    assert_eq!(machine.heap.stats().arenas_allocated, 1);
    assert_eq!(
        machine.heap.stats().arenas_freed,
        1,
        "not deferred until final cleanup"
    );
    assert_eq!(
        machine.heap.stats().live_objects,
        2,
        "only returned cause and fresh defect survive"
    );
    let outcome = machine.finish(result);
    assert!(matches!(&outcome, ProgramOutcome::Failed(_)));
    outcome.release(machine.heap).expect("caller cause release");
    machine.heap.finish().expect("actual heap empty");
}
