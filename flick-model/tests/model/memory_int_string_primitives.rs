//! Raw primitive contracts and real-root cleanup, including every fuel prefix.
use crate::causes;
use flick_core::ir::{
    Lit,
    memory::{
        Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program, ValueRep,
    },
};
use flick_core::opt::perceus;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn function(params: &[u32], instructions: Vec<Instruction>, result: u32) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("ownership insertion")
}
fn program(kind: Intrinsic) -> Program {
    let init = function(
        &[],
        vec![bind(
            0,
            Operation::Closure {
                code: 1,
                captures: vec![],
            },
        )],
        0,
    );
    let mut stages = vec![init];
    if kind == Intrinsic::StringLength {
        stages.push(function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Intrinsic {
                    kind,
                    args: vec![1],
                },
            )],
            2,
        ));
    } else {
        stages.push(function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Closure {
                    code: 2,
                    captures: vec![1],
                },
            )],
            2,
        ));
        stages.push(function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                ),
                bind(
                    3,
                    Operation::Intrinsic {
                        kind,
                        args: vec![2, 1],
                    },
                ),
            ],
            3,
        ));
    }
    Program {
        globals: vec![Global {
            name: Name::new("Primitives", "test"),
            initializer: 0,
        }],
        functions: stages
            .into_iter()
            .enumerate()
            .map(|(i, function)| (u32::try_from(i).expect("small code ID"), function))
            .collect(),
    }
}

fn integer_result(kind: Intrinsic, left: i32, right: i32, expected: i32) {
    let mut heap = Heap::new();
    let report = execute_program_outcome(
        &mut heap,
        &program(kind),
        &Name::new("Primitives", "test"),
        vec![Value::Int(left), Value::Int(right)],
        1_000,
    );
    let ProgramOutcome::Returned(value) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    assert_eq!(value, Value::Int(expected), "{kind:?}({left},{right})");
    heap.drop_value(value).expect("caller releases result");
    assert_eq!(
        heap.finish()
            .expect("all execution roots dropped")
            .live_objects,
        0
    );
}

#[test]
fn integer_quotient_and_remainder_have_independent_boundary_vectors() {
    for (left, right, quotient, remainder) in [
        (7, 3, 2, 1),
        (-7, 3, -2, -1),
        (7, -3, -2, 1),
        (-7, -3, 2, -1),
        (i32::MIN, -1, i32::MIN, 0),
        (i32::MIN, 1, i32::MIN, 0),
        (i32::MAX, -1, -i32::MAX, 0),
        (0, -1, 0, 0),
        (-6, 3, -2, 0),
        (1, i32::MIN, 0, 1),
        (i32::MIN, i32::MAX, -1, -1),
    ] {
        integer_result(Intrinsic::IntUnsafeDiv, left, right, quotient);
        integer_result(Intrinsic::IntUnsafeRem, left, right, remainder);
    }
}

#[test]
fn utf16_length_has_independent_unicode_vectors_and_borrows_the_subject() {
    for (subject, expected) in [
        ("", 0),
        ("abc", 3),
        ("😀", 2),
        ("a😀中", 4),
        ("é", 1),
        ("e\u{301}", 2),
        ("\0", 1),
    ] {
        let mut heap = Heap::new();
        let outside = heap.literal(&Lit::String(subject.into())).expect("subject");
        let argument = heap.dup(&outside).expect("external alias");
        let report = execute_program_outcome(
            &mut heap,
            &program(Intrinsic::StringLength),
            &Name::new("Primitives", "test"),
            vec![argument],
            1_000,
        );
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(value, Value::Int(expected));
        assert_eq!(heap.string(&outside), Ok(subject));
        heap.drop_value(value).expect("result");
        heap.drop_value(outside).expect("external owner");
        let stats = heap
            .finish()
            .expect("subject released only after external drop");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn unsafe_zero_divisors_are_specific_dies_not_typed_raises_or_unsupported() {
    for (kind, remainder, text) in [
        (
            Intrinsic::IntUnsafeDiv,
            false,
            "RangeError: Int.unsafe_div: division by zero",
        ),
        (
            Intrinsic::IntUnsafeRem,
            true,
            "RangeError: Int.unsafe_rem: division by zero",
        ),
    ] {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let report = execute_program_outcome(
            &mut heap,
            &program(kind),
            &Name::new("Primitives", "test"),
            vec![Value::Int(7), Value::Int(0)],
            1_000,
        );
        let ProgramOutcome::Failed(cause) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "expected owned zero-divisor Die")
        };
        let error = causes::defect_error(&mut heap, cause);
        assert_eq!(error, Error::IntDivisionByZero { remainder });
        assert_eq!(error.to_string(), text);
        assert_eq!(heap.string(&outside), Ok("outside"));
        heap.drop_value(outside).expect("external owner");
        let stats = heap
            .finish()
            .expect("fault tears down counted execution roots");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn invalid_integer_operands_keep_external_aliases_and_release_staged_roots() {
    for kind in [Intrinsic::IntUnsafeDiv, Intrinsic::IntUnsafeRem] {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("not an Int".into()))
            .expect("external root");
        let left = heap.dup(&outside).expect("external alias");
        let right = heap.dup(&outside).expect("second external alias");
        let report = execute_program_outcome(
            &mut heap,
            &program(kind),
            &Name::new("Primitives", "test"),
            vec![left, right],
            1_000,
        );
        {
            let diagnostic = "exact outcome expectation";
            let observed_outcome = report.outcome;
            let correct = matches!(
                &observed_outcome,
                ProgramOutcome::Fault(Error::UnsupportedPrimitive(_))
            );
            let detail = format!("{observed_outcome:?}");
            observed_outcome
                .release(&mut heap)
                .expect("every owned outcome released before assertion");
            assert!(correct, "{diagnostic}: {detail}");
        };
        assert_eq!(heap.string(&outside), Ok("not an Int"));
        heap.drop_value(outside).expect("external owner");
        assert_eq!(
            heap.finish()
                .expect("both staged operands released")
                .live_objects,
            0
        );
    }
}

#[test]
fn length_rejects_nontext_without_releasing_an_external_container() {
    let mut heap = Heap::new();
    let text = heap.literal(&Lit::String("held".into())).expect("field");
    let outside = heap
        .construct("NotText", vec![text])
        .expect("external container");
    let argument = heap.dup(&outside).expect("external alias");
    let report = execute_program_outcome(
        &mut heap,
        &program(Intrinsic::StringLength),
        &Name::new("Primitives", "test"),
        vec![argument],
        1_000,
    );
    {
        let diagnostic = "exact outcome expectation";
        let observed_outcome = report.outcome;
        let correct = matches!(
            &observed_outcome,
            ProgramOutcome::Fault(Error::ExpectedString)
        );
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    assert_eq!(
        heap.string(heap.field(&outside, 0).expect("retained field")),
        Ok("held")
    );
    heap.drop_value(outside).expect("external owner");
    assert_eq!(
        heap.finish()
            .expect("staged container released exactly once")
            .live_objects,
        0
    );
}

#[test]
fn every_fuel_prefix_releases_actual_roots_and_preserves_external_owners() {
    for kind in [
        Intrinsic::IntUnsafeDiv,
        Intrinsic::IntUnsafeRem,
        Intrinsic::StringLength,
    ] {
        let mut completed = false;
        for fuel in 0..120 {
            let mut heap = Heap::new();
            let outside = heap
                .literal(&Lit::String("a😀中".into()))
                .expect("external root");
            let arguments = if kind == Intrinsic::StringLength {
                vec![heap.dup(&outside).expect("length subject alias")]
            } else {
                vec![Value::Int(i32::MIN), Value::Int(0)]
            };
            let report = execute_program_outcome(
                &mut heap,
                &program(kind),
                &Name::new("Primitives", "test"),
                arguments,
                fuel,
            );
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    completed = true;
                    assert_eq!(kind, Intrinsic::StringLength);
                    assert_eq!(value, Value::Int(4));
                    heap.drop_value(value).expect("successful result");
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                ProgramOutcome::Failed(cause) => {
                    let Error::IntDivisionByZero { remainder } =
                        causes::defect_error(&mut heap, cause)
                    else {
                        panic!("exact zero-divisor defect")
                    };
                    completed = true;
                    assert_ne!(kind, Intrinsic::StringLength);
                    assert_eq!(remainder, kind == Intrinsic::IntUnsafeRem);
                }
                outcome => {
                    let diagnostic = format!("fuel={fuel} kind={kind:?}: {outcome:?}");
                    causes::unexpected(&mut heap, outcome, &diagnostic)
                }
            }
            assert_eq!(heap.string(&outside), Ok("a😀中"));
            heap.drop_value(outside).expect("external owner");
            let stats = heap.finish().expect("fuel cut tears down real roots");
            assert_eq!(stats.live_objects, 0);
            assert_eq!(stats.allocations, stats.frees);
        }
        assert!(completed, "prefix sweep reaches the semantic outcome");
    }
}
