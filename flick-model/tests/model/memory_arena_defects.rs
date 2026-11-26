//! Retired VM-minted arena capabilities fail in the language, not evaluator protocol.
use flick_core::ir::{
    Lit,
    memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
};
use flick_core::opt::perceus;
use flick_vm::tier0::{
    DefectClass, DefectCode, Error, Heap, ObjectId, ProgramOutcome, Reason, execute_program,
    execute_program_outcome,
};
fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn intrinsic(binding: u32, kind: Intrinsic, args: &[u32]) -> Instruction {
    bind(
        binding,
        Operation::Intrinsic {
            kind,
            args: args.to_vec(),
        },
    )
}
fn function(params: &[u32], instructions: Vec<Instruction>, result: u32) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("valid owned boundary MIR")
}
fn closure(binding: u32, code: u32) -> Instruction {
    bind(
        binding,
        Operation::Closure {
            code,
            captures: vec![],
        },
    )
}
fn boundary(case: &str) -> (Vec<Instruction>, u32, Function) {
    let mut main = vec![closure(10, 1), intrinsic(11, Intrinsic::ScopeRun, &[10])];
    match case {
        "make" => {
            main.push(bind(
                12,
                Operation::Within {
                    owner: 11,
                    code: 2,
                    args: vec![],
                },
            ));
            (main, 12, function(&[0, 1], vec![], 1))
        }
        "alloc" => {
            main.extend([
                bind(12, Operation::Literal(Lit::String("late".into()))),
                bind(
                    13,
                    Operation::Construct {
                        tag: "Payload".into(),
                        fields: vec![12],
                    },
                ),
                intrinsic(14, Intrinsic::ArenaAlloc, &[11, 13]),
            ]);
            (
                main,
                14,
                function(
                    &[0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::Unit)),
                        intrinsic(3, Intrinsic::ArenaMake, &[2]),
                    ],
                    3,
                ),
            )
        }
        "get" => {
            main.push(intrinsic(12, Intrinsic::ArenaGet, &[11]));
            (
                main,
                12,
                function(
                    &[0, 1],
                    vec![
                        bind(2, Operation::Literal(Lit::Unit)),
                        intrinsic(3, Intrinsic::ArenaMake, &[2]),
                        bind(4, Operation::Literal(Lit::String("held".into()))),
                        intrinsic(5, Intrinsic::ArenaAlloc, &[3, 4]),
                    ],
                    5,
                ),
            )
        }
        _ => panic!("known arena boundary vector"),
    }
}
fn program(case: &str) -> Program {
    let (mut main, result, scope) = boundary(case);
    main.splice(
        0..0,
        [
            bind(100, Operation::Literal(Lit::String("before".into()))),
            intrinsic(101, Intrinsic::PrintLine, &[100]),
        ],
    );
    Program {
        functions: [
            (0, function(&[], main, result)),
            (1, scope),
            (
                2,
                function(
                    &[],
                    vec![
                        bind(0, Operation::Literal(Lit::Unit)),
                        intrinsic(1, Intrinsic::ArenaMake, &[0]),
                    ],
                    1,
                ),
            ),
        ]
        .into(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    }
}
fn entry() -> Name {
    Name::new("ArenaBoundary", "main")
}
fn expected(case: &str) -> DefectCode {
    if case == "make" {
        DefectCode::ArenaOwnerClosed
    } else {
        DefectCode::ArenaClosed
    }
}
fn inspect(heap: &Heap, outcome: &ProgramOutcome, case: &str) -> Option<ObjectId> {
    let ProgramOutcome::Failed(stored) = outcome else {
        return None;
    };
    let mut reasons = heap.cause_reasons(stored).expect("stored owned reasons");
    let Some(Reason::Die(value)) = reasons.next() else {
        return None;
    };
    if reasons.next().is_some() {
        return None;
    }
    let spec = heap.defect(value).expect("structured arena defect");
    assert_eq!(spec.code(), &expected(case));
    assert_eq!(spec.class(), DefectClass::Error);
    value.object_id()
}
fn assert_language_boundary(case: &str) {
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &program(case), &entry(), vec![], 200);
    assert_eq!(report.stdout, "before\n");
    let language = inspect(&heap, &report.outcome, case).is_some();
    let actual = format!("{:?}", report.outcome);
    report
        .outcome
        .release(&mut heap)
        .expect("release every owned result before assertion");
    finish(&heap);
    assert!(
        language,
        "{case}: expected exact language Die(Error), got {actual}"
    );
}
fn finish(heap: &Heap) {
    let stats = heap.finish().expect("actual zero occupied slots");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
    assert_eq!(stats.arenas_allocated, stats.arenas_freed);
}
#[test]
fn retired_owner_rejects_arena_make_as_language_defect() {
    assert_language_boundary("make");
}
#[test]
fn retired_arena_rejects_late_allocation_as_language_defect() {
    assert_language_boundary("alloc");
}
#[test]
fn retired_view_rejects_read_as_language_defect() {
    assert_language_boundary("get");
}
#[test]
fn every_fuel_prefix_releases_late_payloads_not_external_aliases() {
    for case in ["make", "alloc", "get"] {
        let p = program(case);
        let mut complete = false;
        for fuel in 0..220 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("caller".into()))
                .expect("caller root");
            let argument = heap.dup(&external).expect("VM accepted alias");
            let report = execute_program_outcome(&mut heap, &p, &entry(), vec![argument], fuel);
            assert!("before\n".starts_with(&report.stdout));
            if inspect(&heap, &report.outcome, case).is_some() {
                complete = true;
                assert_eq!(report.stdout, "before\n");
            } else {
                assert!(
                    matches!(&report.outcome, ProgramOutcome::Fault(Error::OutOfFuel)),
                    "{case} fuel={fuel}: {report:?}"
                );
            }
            report
                .outcome
                .release(&mut heap)
                .expect("all owned outcomes release");
            assert_eq!(heap.string(&external), Ok("caller"));
            assert_eq!(heap.reference_count(&external), Ok(1));
            assert_eq!(heap.stats().live_objects, 1);
            heap.drop_value(external)
                .expect("caller releases external root");
            finish(&heap);
        }
        assert!(complete);
    }
}
#[test]
fn fresh_throws_and_owned_cause_duplicates_preserve_correct_identities() {
    for case in ["make", "alloc", "get"] {
        let mut heap = Heap::new();
        let p = program(case);
        let first = execute_program_outcome(&mut heap, &p, &entry(), vec![], 200);
        let identity = inspect(&heap, &first.outcome, case).expect("first defect identity");
        let ProgramOutcome::Failed(stored) = &first.outcome else {
            unreachable!("inspected cause")
        };
        let duplicate = heap.dup_cause(stored).expect("owned cause duplicate");
        first
            .outcome
            .release(&mut heap)
            .expect("original outcome release");
        let duplicate = ProgramOutcome::Failed(duplicate);
        assert_eq!(inspect(&heap, &duplicate, case), Some(identity));
        let second = execute_program_outcome(&mut heap, &p, &entry(), vec![], 200);
        let fresh =
            inspect(&heap, &second.outcome, case).expect("new throw has exact structured identity");
        assert_ne!(fresh, identity);
        assert_eq!(inspect(&heap, &duplicate, case), Some(identity));
        duplicate
            .release(&mut heap)
            .expect("duplicate retained root release");
        second
            .outcome
            .release(&mut heap)
            .expect("new throw release");
        finish(&heap);
    }
}
#[test]
fn convenience_api_keeps_legacy_errors_after_releasing_structured_causes() {
    for case in ["make", "alloc", "get"] {
        let mut heap = Heap::new();
        assert_eq!(
            execute_program(&mut heap, &program(case), &entry(), vec![], 200)
                .expect_err("semantic arena defect"),
            if case == "make" {
                Error::ClosedOwner
            } else {
                Error::ClosedArena
            }
        );
        finish(&heap);
    }
}
fn wrapped(case: &str) -> Program {
    let mut p = program(case);
    let (body, result, _) = boundary(case);
    p.functions.extend([
        (
            0,
            function(
                &[],
                vec![
                    bind(100, Operation::Literal(Lit::String("before".into()))),
                    intrinsic(101, Intrinsic::PrintLine, &[100]),
                    closure(10, 3),
                    closure(11, 4),
                    intrinsic(12, Intrinsic::Try, &[10, 11]),
                ],
                12,
            ),
        ),
        (
            3,
            function(
                &[0, 1],
                vec![
                    closure(10, 5),
                    closure(11, 4),
                    bind(
                        12,
                        Operation::Handle {
                            body: 10,
                            effects: vec![],
                            ret: None,
                            errors: Some(11),
                        },
                    ),
                ],
                12,
            ),
        ),
        (
            4,
            function(
                &[0, 1],
                vec![
                    bind(2, Operation::Literal(Lit::String("caught".into()))),
                    intrinsic(3, Intrinsic::PrintLine, &[2]),
                    bind(4, Operation::Literal(Lit::Unit)),
                ],
                4,
            ),
        ),
        (
            5,
            function(
                &[0, 1],
                vec![
                    bind(10, Operation::ServiceTag("Service".into())),
                    bind(11, Operation::Record { fields: vec![] }),
                    intrinsic(12, Intrinsic::LayerSucceed, &[10, 11]),
                    closure(13, 6),
                    intrinsic(14, Intrinsic::LayerRun, &[12, 13]),
                ],
                14,
            ),
        ),
        (6, function(&[0, 1], body, result)),
    ]);
    p
}
#[test]
fn every_wrapped_prefix_keeps_typed_recovery_cold_and_unwinds_provider() {
    for case in ["make", "alloc", "get"] {
        let p = wrapped(case);
        let mut complete = false;
        for fuel in 0..300 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("caller".into()))
                .expect("caller root");
            let argument = heap.dup(&external).expect("VM accepted alias");
            let report = execute_program_outcome(&mut heap, &p, &entry(), vec![argument], fuel);
            assert!(
                "before\n".starts_with(&report.stdout),
                "typed recovery stays cold"
            );
            if inspect(&heap, &report.outcome, case).is_some() {
                complete = true;
                assert_eq!(report.stdout, "before\n");
            } else {
                assert!(
                    matches!(&report.outcome, ProgramOutcome::Fault(Error::OutOfFuel)),
                    "{case} fuel={fuel}: {report:?}"
                );
            }
            report
                .outcome
                .release(&mut heap)
                .expect("owned result release");
            assert_eq!(heap.string(&external), Ok("caller"));
            assert_eq!(heap.reference_count(&external), Ok(1));
            assert_eq!(heap.stats().live_objects, 1);
            heap.drop_value(external).expect("caller release");
            finish(&heap);
        }
        assert!(complete);
    }
}
