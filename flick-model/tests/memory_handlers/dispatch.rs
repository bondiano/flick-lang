#[test]
fn nearest_applicable_operation_and_parked_handlers_resume() {
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &partial_handlers(), &entry(), vec![], 1000);
    {
        let diagnostic = format!("{report:?}");
        let observed_outcome = report.outcome;
        let correct = matches!(&observed_outcome, ProgramOutcome::Returned(Value::Int(21)));
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("all execution roots released");
}

fn int(binding: Local, value: i32) -> Instruction {
    bind(binding, Operation::Literal(Lit::Int(value)))
}
fn unit(binding: Local) -> Instruction {
    bind(binding, Operation::Literal(Lit::Unit))
}
fn intrinsic(binding: Local, kind: Intrinsic, args: &[Local]) -> Instruction {
    bind(
        binding,
        Operation::Intrinsic {
            kind,
            args: args.to_vec(),
        },
    )
}
fn add(binding: Local, left: Local, right: Local) -> Instruction {
    bind(
        binding,
        Operation::Binary {
            op: BinOp::Arith(Arith::Add, Num::Int),
            left,
            right,
        },
    )
}
fn run_int(p: &Program, expected: i32) {
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, p, &entry(), vec![], 2000);
    {
        let diagnostic = format!("{report:?}");
        let observed_outcome = report.outcome;
        let correct =
            matches!(&observed_outcome, ProgramOutcome::Returned(Value::Int(n)) if *n == expected);
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("all execution roots released");
}

fn caught_fault_program(mut p: Program) -> Program {
    let n = u32::try_from(p.functions.len()).expect("valid handler test operation");
    p.functions.insert(
        n,
        function(
            &[],
            vec![
                closure(0, n + 1),
                closure(1, n + 2),
                intrinsic(2, Intrinsic::Try, &[0, 1]),
            ],
            2,
        ),
    );
    p.functions.insert(
        n + 1,
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Call {
                    code: 0,
                    args: vec![],
                },
            )],
            2,
        ),
    );
    p.functions.insert(n + 2, constant(99));
    p.globals[0].initializer = n;
    p
}

#[test]
fn tail_arm_runs_outside_itself_and_crossed_inner_handlers() {
    let p = program(vec![
        function(
            &[],
            vec![closure(0, 1), closure(1, 2), handle(2, 0, &[("left", 1)])],
            2,
        ),
        function(
            &[0, 1],
            vec![closure(2, 3), closure(3, 4), handle(4, 2, &[("left", 3)])],
            4,
        ),
        constant(10),
        function(&[0, 1], vec![unit(2), perform(3, "left", &[2])], 3),
        function(
            &[0, 1],
            vec![unit(2), perform(3, "left", &[2]), int(4, 1), add(5, 3, 4)],
            5,
        ),
    ]);
    run_int(&p, 11);
}

#[test]
fn return_and_error_arms_use_outer_evidence_and_transform_once() {
    for failed in [false, true] {
        let body = function(
            &[0, 1],
            if failed {
                vec![int(2, 7), intrinsic(3, Intrinsic::Raise, &[2])]
            } else {
                vec![int(3, 7)]
            },
            3,
        );
        let mut p = program(vec![
            function(
                &[],
                vec![closure(0, 1), closure(1, 2), handle(2, 0, &[("left", 1)])],
                2,
            ),
            function(
                &[0, 1],
                vec![
                    closure(2, 3),
                    closure(3, 4),
                    bind(
                        4,
                        Operation::Handle {
                            body: 2,
                            effects: vec![],
                            ret: (!failed).then_some(3),
                            errors: failed.then_some(3),
                        },
                    ),
                ],
                4,
            ),
            constant(10),
            body,
            function(
                &[0, 1],
                vec![unit(2), perform(3, "left", &[2]), add(4, 1, 3)],
                4,
            ),
        ]);
        if failed {
            recovery::seal_bodies(&mut p, &[4]);
        }
        run_int(&p, 17);
    }
}

fn abort_with_catch() -> Program {
    let mut effects = handled(&[("left", 1)]);
    effects[0].operations[0].class = OpClass::Abort;
    program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 2),
                closure(2, 3),
                bind(
                    3,
                    Operation::Handle {
                        body: 0,
                        effects,
                        ret: Some(2),
                        errors: Some(2),
                    },
                ),
            ],
            3,
        ),
        function(
            &[0, 1],
            vec![
                closure(2, 4),
                closure(3, 3),
                intrinsic(4, Intrinsic::Try, &[2, 3]),
            ],
            4,
        ),
        constant(20),
        constant(99),
        function(
            &[0, 1],
            vec![unit(2), perform(3, "left", &[2]), int(4, 98)],
            4,
        ),
    ])
}
#[test]
fn abort_bypasses_body_catches_and_skips_handler_return_and_errors() {
    run_int(&abort_with_catch(), 20);
}

fn scope_transfer(class: OpClass) -> Program {
    let mut effects = handled(&[("right", 1)]);
    effects[0].operations[0].class = class;
    program(vec![
        function(
            &[],
            vec![
                closure(0, 1),
                closure(1, 2),
                bind(
                    2,
                    Operation::Handle {
                        body: 0,
                        effects,
                        ret: None,
                        errors: None,
                    },
                ),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![closure(2, 3), intrinsic(3, Intrinsic::ScopeRun, &[2])],
            3,
        ),
        function(&[0, 1], vec![intrinsic(2, Intrinsic::ArenaGet, &[1])], 2),
        function(
            &[0, 1],
            vec![
                unit(2),
                intrinsic(3, Intrinsic::ArenaMake, &[2]),
                bind(4, Operation::Literal(Lit::String("scope-payload".into()))),
                intrinsic(5, Intrinsic::ArenaAlloc, &[3, 4]),
                perform(6, "right", &[5]),
            ],
            6,
        ),
    ])
}
#[test]
fn abort_closes_departed_scope_before_arm_but_tail_parks_it_alive() {
    for class in [OpClass::Tail, OpClass::Abort] {
        let mut heap = Heap::new();
        let report =
            execute_program_outcome(&mut heap, &scope_transfer(class), &entry(), vec![], 1000);
        match report.outcome {
            ProgramOutcome::Returned(value) if class == OpClass::Tail => {
                assert_eq!(heap.string(&value), Ok("scope-payload"));
                heap.drop_value(value)
                    .expect("valid handler test operation");
            }
            ProgramOutcome::Failed(cause) if class == OpClass::Abort => {
                assert_eq!(causes::defect_error(&mut heap, cause), Error::ClosedArena);
            }
            other => {
                let diagnostic = format!("{class:?}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        assert_eq!(heap.stats().arenas_freed, 1);
        heap.finish().expect("all execution roots released");
    }
}
