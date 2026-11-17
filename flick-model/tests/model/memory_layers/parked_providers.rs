fn captured(binding: Local, code: u32, captures: &[Local]) -> Instruction {
    bind(
        binding,
        Operation::Closure {
            code,
            captures: captures.to_vec(),
        },
    )
}
fn arena_marker(unit: Local) -> Vec<Instruction> {
    vec![
        intrinsic(20, Intrinsic::ArenaMake, &[unit]),
        bind(21, Operation::Literal(Lit::String("scoped storage".into()))),
        intrinsic(22, Intrinsic::ArenaAlloc, &[20, 21]),
    ]
}
fn nested_constant_provider(callback: u32, text: &str) -> Function {
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
            bind(3, Operation::ServiceTag("Service".into())),
            bind(4, Operation::Literal(Lit::String(text.into()))),
            bind(
                5,
                Operation::Record {
                    fields: vec![("value".into(), 4)],
                },
            ),
            intrinsic(6, Intrinsic::LayerSucceed, &[3, 5]),
            captured(7, callback, &[2]),
            intrinsic(8, Intrinsic::LayerRun, &[6, 7]),
        ],
        8,
    )
}
fn nested_outer_functions() -> Vec<Function> {
    vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 12),
                intrinsic(2, Intrinsic::Try, &[0, 1]),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![
                closure(2, 2),
                closure(3, 11),
                handle(4, 2, "Stop", "stop", 3),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![closure(2, 3), intrinsic(3, Intrinsic::ScopeRun, &[2])],
            3,
        ),
        function(
            &[0, 1],
            {
                let mut ops = vec![bind(2, Operation::Literal(Lit::Unit))];
                ops.extend(arena_marker(2));
                ops.extend([
                    captured(3, 4, &[1]),
                    closure(4, 10),
                    handle(5, 3, "Pong", "pong", 4),
                ]);
                ops
            },
            5,
        ),
        nested_constant_provider(5, "outer"),
        function(
            &[0, 1],
            {
                let mut ops = arena_marker(1);
                ops.extend([
                    bind(
                        2,
                        Operation::Project {
                            object: 0,
                            field: 0,
                        },
                    ),
                    captured(3, 6, &[2]),
                    intrinsic(4, Intrinsic::ScopeRun, &[3]),
                ]);
                ops
            },
            4,
        ),
        function(
            &[0, 1],
            {
                let mut ops = vec![bind(2, Operation::Literal(Lit::Unit))];
                ops.extend(arena_marker(2));
                ops.extend([
                    bind(
                        3,
                        Operation::Project {
                            object: 0,
                            field: 0,
                        },
                    ),
                    captured(4, 7, &[3]),
                    closure(5, 9),
                    handle(6, 4, "Ping", "ping", 5),
                ]);
                ops
            },
            6,
        ),
    ]
}
fn nested_parked_providers(mode: &str) -> Program {
    let mut fs = nested_outer_functions();
    fs.extend([
        nested_constant_provider(8, "inner"),
        function(
            &[0, 1],
            {
                let mut ops = arena_marker(1);
                ops.extend([
                    bind(
                        2,
                        Operation::Project {
                            object: 0,
                            field: 0,
                        },
                    ),
                    captured(3, 13, &[2]),
                    intrinsic(4, Intrinsic::ScopeRun, &[3]),
                ]);
                ops
            },
            4,
        ),
        function(&[0, 1], vec![perform(2, "Pong", "pong", &[], &[1])], 2),
        match mode {
            "abort" => function(
                &[0, 1],
                vec![
                    bind(2, Operation::Literal(Lit::String("stopped".into()))),
                    perform(3, "Stop", "stop", &[], &[2]),
                ],
                3,
            ),
            "failure" => function(
                &[0, 1],
                vec![
                    bind(2, Operation::Literal(Lit::String("caught".into()))),
                    intrinsic(3, Intrinsic::Raise, &[2]),
                ],
                3,
            ),
            "resume" => function(&[0, 1], vec![], 1),
            _ => panic!("mode"),
        },
        function(&[0, 1], vec![], 1),
        function(&[0, 1], vec![], 1),
        function(
            &[0, 1],
            {
                let mut ops = vec![bind(2, Operation::Literal(Lit::Unit))];
                ops.extend(arena_marker(2));
                ops.extend([
                    bind(
                        3,
                        Operation::Project {
                            object: 0,
                            field: 0,
                        },
                    ),
                    bind(
                        4,
                        Operation::Within {
                            owner: 3,
                            code: 14,
                            args: vec![2],
                        },
                    ),
                ]);
                ops
            },
            4,
        ),
        function(
            &[0],
            vec![
                perform(1, "Ping", "ping", &[], &[0]),
                perform(2, "Service", "value", &[], &[]),
            ],
            2,
        ),
    ]);
    let mut p = program(fs);
    let handle_instruction = p
        .functions
        .get_mut(&1)
        .expect("valid counted Layer test operation")
        .body
        .instructions
        .iter_mut()
        .find_map(|i| match i {
            Instruction::Let {
                value: Operation::Handle { effects, .. },
                ..
            } => Some(effects),
            _ => None,
        })
        .expect("valid counted Layer test operation");
    handle_instruction[0].operations[0].class = OpClass::Abort;
    recovery::seal_bodies(&mut p, &[12]);
    p
}
#[test]
fn two_parked_provider_segments_resume_or_discontinue_exact_saved_owners() {
    for (mode, expected) in [
        ("resume", "inner"),
        ("abort", "stopped"),
        ("failure", "caught"),
    ] {
        let p = nested_parked_providers(mode);
        let mut completed = 0;
        for fuel in 0..=512 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("external alias".into()))
                .expect("valid counted Layer test operation");
            let tag = heap
                .service_tag("Service")
                .expect("valid counted Layer test operation");
            match execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel).outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(heap.string(&value), Ok(expected), "mode={mode} fuel={fuel}");
                    assert_eq!(
                        heap.stats().arenas_allocated,
                        5,
                        "all five lexical/provider owners allocate"
                    );
                    assert_eq!(
                        heap.stats().arenas_freed,
                        5,
                        "every exact saved owner retires physical storage"
                    );
                    heap.drop_value(value)
                        .expect("valid counted Layer test operation");
                    completed += 1;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => {
                    let diagnostic = format!("mode={mode} fuel={fuel}: {other:?}");
                    causes::unexpected(&mut heap, other, &diagnostic)
                }
            }
            assert_eq!(heap.string(&external), Ok("external alias"));
            assert_eq!(
                heap.stats().live_objects,
                2,
                "mode={mode} fuel={fuel}: only external aliases survive"
            );
            heap.drop_value(external)
                .expect("valid counted Layer test operation");
            heap.drop_value(tag)
                .expect("valid counted Layer test operation");
            heap.finish().expect("valid counted Layer test operation");
        }
        assert!(completed > 0, "{mode}: fuel sweep must reach completion");
    }
}

fn abort_first_handle(p: &mut Program, code: u32) {
    let effects = p
        .functions
        .get_mut(&code)
        .expect("handler function")
        .body
        .instructions
        .iter_mut()
        .find_map(|i| match i {
            Instruction::Let {
                value: Operation::Handle { effects, .. },
                ..
            } => Some(effects),
            _ => None,
        })
        .expect("handler boundary");
    effects[0].operations[0].class = OpClass::Abort;
}
fn assert_int_fuel(p: &Program, expected: i32) {
    let mut completed = false;
    for fuel in 0..=220 {
        let mut heap = Heap::new();
        let alias = heap.service_tag("Service").expect("external tag");
        match execute_program_outcome(&mut heap, p, &entry(), vec![], fuel).outcome {
            ProgramOutcome::Returned(Value::Int(n)) => {
                assert_eq!(n, expected);
                completed = true;
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            other => {
                let diagnostic = format!("fuel={fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        heap.drop_value(alias).expect("external release");
        heap.finish().expect("actual heap empty");
    }
    assert!(completed);
}
#[test]
fn discarded_provider_does_not_leave_abandoned_initializer_marker() {
    let retry = Name::new("Main", "retry");
    let mut p = program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 2),
                handle(2, 0, "Stop", "stop", 1),
                closure(3, 1),
                closure(4, 3),
                handle(5, 3, "Stop", "stop", 4),
            ],
            5,
        ),
        function(&[0, 1], vec![bind(2, Operation::Global(retry.clone()))], 2),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(20)))], 2),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(7)))], 2),
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                bind(1, Operation::Literal(Lit::Unit)),
                intrinsic(2, Intrinsic::LayerSucceed, &[0, 1]),
                closure(3, 5),
                intrinsic(4, Intrinsic::LayerRun, &[2, 3]),
            ],
            4,
        ),
        function(&[0, 1], vec![perform(2, "Stop", "stop", &[], &[1])], 2),
    ]);
    p.globals.push(Global {
        name: retry,
        initializer: 4,
    });
    abort_first_handle(&mut p, 0);
    assert_int_fuel(&p, 7);
}
#[test]
fn discarded_provider_in_modify_callback_never_commits() {
    let mut p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Int(5))),
                intrinsic(1, Intrinsic::RefMake, &[0]),
                captured(2, 1, &[1]),
                closure(3, 2),
                handle(4, 2, "Stop", "stop", 3),
                intrinsic(5, Intrinsic::RefGet, &[1]),
            ],
            5,
        ),
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
                closure(3, 3),
                intrinsic(4, Intrinsic::RefModify, &[2, 3]),
            ],
            4,
        ),
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(20)))], 2),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::ServiceTag("Service".into())),
                intrinsic(3, Intrinsic::LayerSucceed, &[2, 1]),
                closure(4, 4),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
        ),
        function(&[0, 1], vec![perform(2, "Stop", "stop", &[], &[1])], 2),
    ]);
    abort_first_handle(&mut p, 0);
    assert_int_fuel(&p, 5);
}

fn provider_language_die() -> Program {
    program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                closure(1, 4),
                bind(
                    2,
                    Operation::Record {
                        fields: vec![("bad".into(), 1)],
                    },
                ),
                intrinsic(3, Intrinsic::LayerSucceed, &[0, 2]),
                closure(4, 1),
                intrinsic(5, Intrinsic::LayerRun, &[3, 4]),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![
                closure(2, 2),
                closure(3, 3),
                intrinsic(4, Intrinsic::Try, &[2, 3]),
            ],
            4,
        ),
        function(&[0, 1], vec![perform(2, "Service", "bad", &[], &[1])], 2),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::String("caught".into()))),
                intrinsic(3, Intrinsic::PrintLine, &[2]),
                bind(4, Operation::Literal(Lit::Int(999))),
            ],
            4,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::Unit)),
                intrinsic(3, Intrinsic::ArenaMake, &[2]),
                intrinsic(4, Intrinsic::MatchFail, &[]),
            ],
            4,
        ),
    ])
}
#[test]
fn singleton_die_leaves_plain_provider_and_never_enters_ordinary_caller_try() {
    let p = provider_language_die();
    let mut completed = false;
    for fuel in 0..300 {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("caller".into()))
            .expect("caller root");
        let argument = heap.dup(&external).expect("VM argument alias");
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![argument], fuel);
        assert_eq!(report.stdout, "", "ordinary recovery stays cold");
        match report.outcome {
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            ProgramOutcome::Failed(cause) => {
                assert_eq!(causes::defect_error(&mut heap, cause), Error::NoMatch);
                assert_eq!(heap.stats().arenas_allocated, 1);
                assert_eq!(heap.stats().arenas_freed, 1);
                completed = true;
            }
            other => causes::unexpected(&mut heap, other, "plain provider singleton Die"),
        }
        assert_eq!(heap.string(&external), Ok("caller"));
        assert_eq!(heap.reference_count(&external), Ok(1));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external).expect("caller release");
        let stats = heap
            .finish()
            .expect("provider, invocation and cause roots release");
        assert_eq!(stats.allocations, stats.frees);
        assert_eq!(stats.arenas_allocated, stats.arenas_freed);
    }
    assert!(completed);
}
