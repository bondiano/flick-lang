fn assert_arena_protocol_fault(kind: Intrinsic, case: &str) {
    let program = Program {
        functions: BTreeMap::new(),
        globals: vec![],
    };
    let mut heap = Heap::new();
    let foreign = Heap::new();
    let root = match case {
        "exec" | "dataexec" | "slot" => Some(heap.allocate(Data::Arena {
            execution: usize::from(case == "dataexec"),
            contents: if case == "slot" {
                vec![Value::Int(7)]
            } else {
                vec![]
            },
        })),
        "layout" => Some(
            heap.literal(&Lit::String("not arena storage".into()))
                .expect("layout fixture"),
        ),
        "value" | "heap" | "never" => None,
        _ => panic!("known private protocol fixture"),
    };
    let id = root
        .as_ref()
        .and_then(Value::object_id)
        .unwrap_or(ObjectId {
            heap: if case == "heap" {
                foreign.identity
            } else {
                heap.identity
            },
            index: usize::MAX,
        });
    let expected = match case {
        "heap" | "exec" | "dataexec" => Error::ForeignArena,
        "never" => Error::InvalidObject(id),
        "layout" => Error::ExpectedArena,
        "slot" => Error::MissingArenaSlot(7),
        "value" if kind == Intrinsic::ArenaAlloc => Error::ExpectedArena,
        "value" => Error::ExpectedArenaView,
        _ => panic!("known protocol expectation"),
    };
    // Private malformed-capability injection, never a public id constructor.
    let handle = ArenaHandle {
        id,
        execution: usize::from(case == "exec"),
    };
    let receiver = if case == "value" {
        Value::Unit
    } else if kind == Intrinsic::ArenaAlloc {
        Value::Arena(handle)
    } else {
        Value::ArenaView(ArenaView {
            arena: handle,
            index: if case == "slot" { 7 } else { 0 },
        })
    };
    let mut machine = fixture_machine(&program, &mut heap);
    if let Some(root) = root {
        machine
            .globals
            .insert(Name::new("Fixture", "root"), Some(root));
    }
    machine.scratch.push(receiver);
    if kind == Intrinsic::ArenaAlloc {
        let payload = machine
            .heap
            .literal(&Lit::String("accepted payload".into()))
            .expect("payload root");
        machine.scratch.push(payload);
    }
    let result = match machine.scoped_intrinsic(kind, 0) {
        Err(error) => Err(error),
        Ok(()) => Ok(LanguageExit::Failure(
            machine
                .failure
                .take()
                .expect("invalid protocol cannot succeed"),
        )),
    };
    let outcome = machine.finish(result);
    let correct = matches!(&outcome, ProgramOutcome::Fault(actual) if actual == &expected);
    let diagnostic = format!("{kind:?} {case}: {outcome:?}");
    outcome
        .release(machine.heap)
        .expect("release any unexpected owned cause");
    let stats = machine
        .heap
        .finish()
        .expect("accepted operands and fixture roots reclaimed");
    assert_eq!(stats.allocations, stats.frees);
    assert_eq!(stats.arenas_allocated, stats.arenas_freed);
    foreign.finish().expect("foreign heap untouched");
    assert!(correct, "{diagnostic}; expected {expected:?}");
}

#[test]
fn arena_protocol_faults_remain_distinct_from_sealed_retirement() {
    for kind in [Intrinsic::ArenaAlloc, Intrinsic::ArenaGet] {
        for case in ["value", "heap", "exec", "dataexec", "layout", "never"] {
            assert_arena_protocol_fault(kind, case);
        }
    }
    assert_arena_protocol_fault(Intrinsic::ArenaGet, "slot");
}

fn arena_die_unwind_program() -> Program {
    let bind = |binding, value| Instruction::Let { binding, value };
    let intrinsic = |binding, kind, args| bind(binding, Operation::Intrinsic { kind, args });
    let closure = |binding, code, captures| bind(binding, Operation::Closure { code, captures });
    let entry = Name::new("Main", "main");
    Program {
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
                        closure(0, 1, vec![]),
                        intrinsic(1, Intrinsic::ScopeRun, vec![0]),
                    ],
                    1,
                ),
            ),
            (
                1,
                function(
                    vec![0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::Unit)),
                        intrinsic(3, Intrinsic::ArenaMake, vec![2]),
                        closure(4, 2, vec![]),
                        intrinsic(5, Intrinsic::ScopeRun, vec![4]),
                        bind(6, Operation::ServiceTag("Service".into())),
                        bind(7, Operation::Record { fields: vec![] }),
                        intrinsic(8, Intrinsic::LayerSucceed, vec![6, 7]),
                        closure(9, 3, vec![5]),
                        intrinsic(10, Intrinsic::LayerRun, vec![8, 9]),
                    ],
                    10,
                ),
            ),
            (
                2,
                function(
                    vec![0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::Unit)),
                        intrinsic(3, Intrinsic::ArenaMake, vec![2]),
                        bind(4, Operation::Literal(Lit::String("retired slot".into()))),
                        intrinsic(5, Intrinsic::ArenaAlloc, vec![3, 4]),
                    ],
                    5,
                ),
            ),
            (
                3,
                function(
                    vec![0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::Unit)),
                        intrinsic(3, Intrinsic::ArenaMake, vec![2]),
                        bind(
                            4,
                            Operation::Project {
                                object: 0,
                                field: 0,
                            },
                        ),
                        intrinsic(5, Intrinsic::ArenaGet, vec![4]),
                    ],
                    5,
                ),
            ),
        ]),
    }
}

#[test]
fn arena_die_unwinds_live_scope_and_provider_before_final_cleanup() {
    let program = arena_die_unwind_program();
    let entry = Name::new("Main", "main");
    let mut heap = Heap::new();
    let mut machine = fixture_machine(&program, &mut heap);
    machine.global(&entry, None).expect("initializer frame");
    let result = machine.run();
    assert!(machine.frames.is_empty());
    assert!(machine.globals.is_empty());
    assert!(machine.providers.is_empty());
    assert!(machine.owners[1..].iter().all(|owner| owner.closed));
    assert_eq!(machine.current_owner, 0);
    assert!(machine.current_evidence.is_none());
    assert_eq!(machine.heap.stats().arenas_allocated, 3);
    assert_eq!(
        machine.heap.stats().arenas_freed,
        3,
        "unwind, not final cleanup, retires live resources"
    );
    assert_eq!(
        machine.heap.stats().live_objects,
        2,
        "only cause and fresh defect survive"
    );
    let outcome = machine.finish(result);
    let ProgramOutcome::Failed(cause) = &outcome else {
        panic!("sealed arena retirement is language Die")
    };
    let Some(Reason::Die(value)) = machine.heap.cause_reasons(cause).expect("reason").next() else {
        panic!("single Die")
    };
    assert_eq!(
        machine
            .heap
            .defect(value)
            .expect("structured metadata")
            .code(),
        &DefectCode::ArenaClosed
    );
    outcome
        .release(machine.heap)
        .expect("explicit caller release");
    machine.heap.finish().expect("actual heap empty");
}
