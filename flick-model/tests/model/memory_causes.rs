//! Semantic exits must not be reported as evaluator integrity faults.
//! Baseline uses only the existing executable API; no proposed Cause API is assumed.
use crate::causes;
use flick_core::{
    ir::{
        BinOp, Lit,
        memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
    },
    numeric::{Arith, Num},
    opt::perceus,
};
use flick_vm::tier0::{
    DefectClass, DefectCode, Error, Heap, LanguageExit, ProgramOutcome, Reason, Value,
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
fn program(mut body: Vec<Instruction>, result: u32) -> Program {
    let mut instructions = vec![
        bind(0, Operation::Literal(Lit::String("before".into()))),
        intrinsic(1, Intrinsic::PrintLine, &[0]),
    ];
    instructions.append(&mut body);
    let function = perceus::insert(&Function {
        params: vec![],
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("well-formed owned MIR, not a missing API compile failure");
    Program {
        functions: [(0, function)].into(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    }
}
fn entry() -> Name {
    Name::new("Causes", "main")
}

fn assert_language_defect(p: &Program, expected_code: &DefectCode, class: DefectClass) {
    let mut heap = Heap::new();
    let external = heap
        .literal(&Lit::String("caller".into()))
        .expect("external owner");
    let report = execute_program_outcome(&mut heap, p, &entry(), vec![], 10_000);
    assert_eq!(report.stdout, "before\n");
    let ProgramOutcome::Failed(cause) = &report.outcome else {
        let diagnostic = format!("{:?}", report.outcome);
        report
            .outcome
            .release(&mut heap)
            .expect("unexpected outcome release");
        panic!("expected owned language Die, not evaluator Fault or typed Fail: {diagnostic}");
    };
    let mut reasons = heap.cause_reasons(cause).expect("borrowed cause");
    let Some(Reason::Die(defect)) = reasons.next() else {
        panic!("expected single Die")
    };
    assert!(reasons.next().is_none());
    let spec = heap.defect(defect).expect("structured semantic defect");
    assert_eq!(spec.code(), expected_code);
    assert_eq!(spec.class(), class);
    assert_eq!(heap.reference_count(defect), Ok(1));
    drop(reasons);
    report
        .outcome
        .release(&mut heap)
        .expect("owned cause release after VM teardown");
    assert_eq!(heap.string(&external), Ok("caller"));
    assert_eq!(heap.reference_count(&external), Ok(1));
    assert_eq!(heap.stats().live_objects, 1);
    heap.drop_value(external).expect("explicit caller release");
    let stats = heap.finish().expect("actual zero occupied heap");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}

fn integer_zero(kind: Intrinsic) -> Program {
    program(
        vec![
            bind(2, Operation::Literal(Lit::Int(7))),
            bind(3, Operation::Literal(Lit::Int(0))),
            intrinsic(4, kind, &[2, 3]),
        ],
        4,
    )
}
#[test]
fn int_division_zero_is_a_language_defect_not_evaluator_fault() {
    // runtime/src/core/text.ts::intUnsafeDiv throws RangeError, not typed Fail.
    assert_language_defect(
        &integer_zero(Intrinsic::IntUnsafeDiv),
        &DefectCode::IntDivisionByZero { remainder: false },
        DefectClass::RangeError,
    );
}
#[test]
fn int_remainder_zero_is_a_language_defect_not_evaluator_fault() {
    assert_language_defect(
        &integer_zero(Intrinsic::IntUnsafeRem),
        &DefectCode::IntDivisionByZero { remainder: true },
        DefectClass::RangeError,
    );
}
#[test]
fn no_match_is_a_language_defect_not_evaluator_fault() {
    // runtime-go/flickrt/value.go::NoMatch panics Defect, not a VM protocol error.
    assert_language_defect(
        &program(vec![intrinsic(2, Intrinsic::MatchFail, &[])], 2),
        &DefectCode::NoMatch,
        DefectClass::Error,
    );
}
#[test]
fn missing_handler_is_a_language_defect_not_evaluator_fault() {
    // runtime/src/rt/handle.ts::callOp throws Error for absent effect lookup.
    let p = program(
        vec![
            bind(2, Operation::Literal(Lit::Unit)),
            bind(
                3,
                Operation::Perform {
                    effect: "Absent".into(),
                    op: "read".into(),
                    dicts: vec![],
                    args: vec![2],
                },
            ),
        ],
        3,
    );
    assert_language_defect(
        &p,
        &DefectCode::MissingHandler {
            effect: "Absent".into(),
            op: "read".into(),
        },
        DefectClass::Error,
    );
}
#[test]
fn negative_bigint_exponent_is_a_language_defect_not_evaluator_fault() {
    let p = program(
        vec![
            bind(2, Operation::Literal(Lit::BigInt("2".into()))),
            bind(3, Operation::Literal(Lit::BigInt("-1".into()))),
            bind(
                4,
                Operation::Binary {
                    op: BinOp::Arith(Arith::Pow, Num::BigInt),
                    left: 2,
                    right: 3,
                },
            ),
        ],
        4,
    );
    assert_language_defect(
        &p,
        &DefectCode::BigIntNegativeExponent,
        DefectClass::RangeError,
    );
}
fn conversion(input: Lit) -> Program {
    program(
        vec![
            bind(2, Operation::Literal(input)),
            intrinsic(3, Intrinsic::ToBigInt, &[2]),
        ],
        3,
    )
}
#[test]
fn nonintegral_bigint_input_is_a_language_range_defect() {
    assert_language_defect(
        &conversion(Lit::Float(0.5)),
        &DefectCode::BigIntNonIntegralNumber,
        DefectClass::RangeError,
    );
}
#[test]
fn invalid_bigint_string_is_a_language_syntax_defect() {
    assert_language_defect(
        &conversion(Lit::String("not an integer".into())),
        &DefectCode::BigIntInvalidString,
        DefectClass::SyntaxError,
    );
}
#[test]
fn undefined_bigint_input_is_a_language_type_defect() {
    assert_language_defect(
        &conversion(Lit::Unit),
        &DefectCode::BigIntUndefined,
        DefectClass::TypeError,
    );
}
#[test]
fn administrative_fuel_and_wrong_kind_stay_evaluator_faults() {
    let p = program(
        vec![
            bind(2, Operation::Literal(Lit::Int(7))),
            bind(3, Operation::Not(2)),
        ],
        3,
    );
    for (fuel, expected) in [(0, Error::OutOfFuel), (100, Error::ExpectedBoolean)] {
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], fuel);
        let ProgramOutcome::Fault(actual) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(actual, expected);
        let stats = heap.finish().expect("no hidden cleanup collector");
        assert_eq!(stats.allocations, stats.frees);
    }
}
#[test]
fn ordinary_raise_still_transfers_only_its_owned_typed_payload() {
    let p = program(
        vec![
            bind(2, Operation::Literal(Lit::String("typed".into()))),
            intrinsic(3, Intrinsic::Raise, &[2]),
        ],
        3,
    );
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &p, &entry(), vec![], 100);
    let ProgramOutcome::Raised(value) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    assert_eq!(heap.string(&value), Ok("typed"));
    assert_ne!(value, Value::Unit);
    heap.drop_value(value).expect("caller owns typed payload");
    let stats = heap.finish().expect("typed failure release");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn ordered_representation_preserves_empty_failure_and_borrowed_diagnostics() {
    let mut heap = Heap::new();
    let payload = heap
        .literal(&Lit::String("payload".into()))
        .expect("payload root");
    let cause = heap
        .cause(
            &[
                Reason::Fail(&payload),
                Reason::Die(&payload),
                Reason::Interrupt { fiber_id: None },
                Reason::Interrupt { fiber_id: Some(42) },
                Reason::Fail(&payload),
            ],
            &mut 100,
        )
        .expect("ordered representation only, not a VM producer");
    let count = heap.reference_count(&payload).expect("payload references");
    let reasons: Vec<_> = heap
        .cause_reasons(&cause)
        .expect("borrowed reasons")
        .collect();
    assert!(matches!(
        reasons.as_slice(),
        [
            Reason::Fail(_),
            Reason::Die(_),
            Reason::Interrupt { fiber_id: None },
            Reason::Interrupt { fiber_id: Some(42) },
            Reason::Fail(_)
        ]
    ));
    assert_eq!(heap.reference_count(&payload), Ok(count));
    let duplicate = heap.dup_cause(&cause).expect("owned duplicate cause");
    heap.drop_cause(cause).expect("release first cause root");
    heap.drop_value(payload).expect("payload original release");
    ProgramOutcome::Failed(duplicate)
        .release(&mut heap)
        .expect("all ordered edges release");
    let empty = heap
        .cause(&[], &mut 1)
        .expect("empty failure representation");
    assert_eq!(heap.cause_reasons(&empty).expect("empty reasons").len(), 0);
    let exit = LanguageExit::Failure(empty);
    let LanguageExit::Failure(empty) = exit else {
        panic!("empty Failure is not Success")
    };
    ProgramOutcome::Failed(empty)
        .release(&mut heap)
        .expect("empty cause allocation releases");
    assert_eq!(
        heap.finish()
            .expect("no payload or cause roots left")
            .live_objects,
        0
    );
}

#[test]
fn cause_duplication_preserves_defect_identity_but_separate_throws_are_fresh() {
    let mut heap = Heap::new();
    let p = integer_zero(Intrinsic::IntUnsafeDiv);
    let first = execute_program_outcome(&mut heap, &p, &entry(), vec![], 100);
    let ProgramOutcome::Failed(cause) = first.outcome else {
        causes::unexpected(&mut heap, first.outcome, "unexpected owned outcome")
    };
    let duplicate = heap.dup_cause(&cause).expect("duplicate cause");
    let identity = match heap.cause_reasons(&cause).expect("first reason").next() {
        Some(Reason::Die(value)) => value.object_id().expect("fresh defect identity"),
        other => panic!("{other:?}"),
    };
    heap.drop_cause(cause)
        .expect("first root released, duplicate survives VM tables");
    let second = execute_program_outcome(&mut heap, &p, &entry(), vec![], 100);
    let ProgramOutcome::Failed(other) = second.outcome else {
        causes::unexpected(&mut heap, second.outcome, "unexpected owned outcome")
    };
    let Some(Reason::Die(second)) = heap.cause_reasons(&other).expect("second reason").next()
    else {
        panic!("Die")
    };
    assert_ne!(second.object_id(), Some(identity));
    let Some(Reason::Die(first)) = heap
        .cause_reasons(&duplicate)
        .expect("retained reason")
        .next()
    else {
        panic!("Die")
    };
    assert_eq!(first.object_id(), Some(identity));
    assert_eq!(
        heap.defect(first).expect("retained metadata").code(),
        &DefectCode::IntDivisionByZero { remainder: false }
    );
    heap.drop_cause(duplicate).expect("retained cause release");
    heap.drop_cause(other).expect("second cause release");
    let stats = heap
        .finish()
        .expect("VM teardown never owns returned causes");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn every_partial_cause_build_prefix_releases_duplicates_not_borrowed_originals() {
    for fuel in 0..12 {
        let mut heap = Heap::new();
        let first = heap
            .literal(&Lit::String("first".into()))
            .expect("first original");
        let second = heap
            .literal(&Lit::String("second".into()))
            .expect("second original");
        match heap.cause(
            &[
                Reason::Fail(&first),
                Reason::Die(&second),
                Reason::Fail(&first),
            ],
            &mut { fuel },
        ) {
            Ok(cause) => heap.drop_cause(cause).expect("release completed cause"),
            Err(error) => assert_eq!(error, Error::OutOfFuel),
        }
        assert_eq!(heap.reference_count(&first), Ok(1), "fuel={fuel}");
        assert_eq!(heap.reference_count(&second), Ok(1), "fuel={fuel}");
        heap.drop_value(first).expect("first original release");
        heap.drop_value(second).expect("second original release");
        let stats = heap.finish().expect("failed accepted prefixes drained");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn foreign_payload_after_a_valid_prefix_never_leaks_accepted_duplicates() {
    let mut heap = Heap::new();
    let mut foreign_heap = Heap::new();
    let first = heap
        .literal(&Lit::String("first".into()))
        .expect("first root");
    let foreign = foreign_heap
        .literal(&Lit::String("foreign".into()))
        .expect("foreign root");
    assert!(matches!(
        heap.cause(&[Reason::Fail(&first), Reason::Die(&foreign)], &mut 100),
        Err(Error::InvalidObject(_))
    ));
    assert_eq!(heap.reference_count(&first), Ok(1));
    assert_eq!(foreign_heap.reference_count(&foreign), Ok(1));
    heap.drop_value(first).expect("local owner release");
    foreign_heap
        .drop_value(foreign)
        .expect("foreign owner release");
    heap.finish().expect("local actual zero");
    foreign_heap.finish().expect("foreign actual zero");
}

#[test]
fn deep_payload_reclamation_uses_the_existing_iterative_destructor() {
    let mut heap = Heap::new();
    let external = heap
        .literal(&Lit::String("outside".into()))
        .expect("outside root");
    let mut payload = heap.dup(&external).expect("retained nested leaf");
    for _ in 0..20_000 {
        payload = heap.construct("Node", vec![payload]).expect("owned edge");
    }
    let cause = heap
        .cause(&[Reason::Die(&payload)], &mut 10)
        .expect("representation owns nested payload");
    heap.drop_value(payload)
        .expect("release original nested root");
    // Another real execution is torn down while only the cause retains the graph.
    let p = program(vec![bind(2, Operation::Literal(Lit::Unit))], 2);
    execute_program_outcome(&mut heap, &p, &entry(), vec![], 100)
        .outcome
        .release(&mut heap)
        .expect("unrelated execution");
    heap.drop_cause(cause).expect("iterative 20k edge release");
    assert_eq!(heap.string(&external), Ok("outside"));
    assert_eq!(heap.reference_count(&external), Ok(1));
    heap.drop_value(external).expect("outside explicit release");
    let stats = heap.finish().expect("actual heap empty");
    assert_eq!(stats.allocations, stats.frees);
}

fn function(params: &[u32], instructions: Vec<Instruction>, result: u32) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("valid owned unwind function")
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
fn scoped_die() -> Program {
    let mut p = program(
        vec![
            closure(2, 1),
            closure(3, 2),
            intrinsic(4, Intrinsic::Try, &[2, 3]),
        ],
        4,
    );
    p.functions.extend([
        (
            1,
            function(
                &[0, 1],
                vec![
                    closure(2, 3),
                    closure(3, 2),
                    bind(
                        4,
                        Operation::Handle {
                            body: 2,
                            effects: vec![],
                            ret: None,
                            errors: Some(3),
                        },
                    ),
                ],
                4,
            ),
        ),
        (
            2,
            function(
                &[0, 1],
                vec![
                    bind(2, Operation::Literal(Lit::String("caught".into()))),
                    intrinsic(3, Intrinsic::PrintLine, &[2]),
                    bind(4, Operation::Literal(Lit::Int(99))),
                ],
                4,
            ),
        ),
        (
            3,
            function(
                &[0, 1],
                vec![closure(2, 4), intrinsic(3, Intrinsic::ScopeRun, &[2])],
                3,
            ),
        ),
        (
            4,
            function(
                &[0, 1],
                vec![
                    bind(2, Operation::Literal(Lit::Unit)),
                    intrinsic(3, Intrinsic::ArenaMake, &[2]),
                    bind(4, Operation::Literal(Lit::String("scoped payload".into()))),
                    intrinsic(5, Intrinsic::ArenaAlloc, &[3, 4]),
                    bind(6, Operation::Literal(Lit::Int(7))),
                    bind(7, Operation::Literal(Lit::Int(0))),
                    intrinsic(8, Intrinsic::IntUnsafeDiv, &[6, 7]),
                ],
                8,
            ),
        ),
    ]);
    p
}
#[test]
fn every_live_defect_prefix_bypasses_typed_recovery_and_closes_departed_scope() {
    let p = scoped_die();
    let mut completed = false;
    for fuel in 0..200 {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("caller".into()))
            .expect("caller owner");
        let argument = heap.dup(&external).expect("VM argument alias");
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![argument], fuel);
        assert!(
            "before\n".starts_with(&report.stdout),
            "no recovery callback executes"
        );
        match report.outcome {
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            ProgramOutcome::Failed(cause) => {
                let mut reasons = heap
                    .cause_reasons(&cause)
                    .expect("owned Die after full unwind");
                let Some(Reason::Die(value)) = reasons.next() else {
                    panic!("single Die")
                };
                assert!(reasons.next().is_none());
                assert_eq!(
                    heap.defect(value).expect("structured defect").code(),
                    &DefectCode::IntDivisionByZero { remainder: false }
                );
                drop(reasons);
                assert_eq!(report.stdout, "before\n");
                assert_eq!(heap.stats().arenas_allocated, 1);
                assert_eq!(heap.stats().arenas_freed, 1);
                heap.drop_cause(cause).expect("owned cause release");
                completed = true;
            }
            other => causes::unexpected(&mut heap, other, "only exact Die or fuel fault"),
        }
        assert_eq!(heap.string(&external), Ok("caller"));
        assert_eq!(heap.reference_count(&external), Ok(1));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external).expect("explicit caller release");
        let stats = heap
            .finish()
            .expect("all active, pending and cause roots drained");
        assert_eq!(stats.allocations, stats.frees);
        assert_eq!(stats.arenas_allocated, stats.arenas_freed);
    }
    assert!(completed);
}
