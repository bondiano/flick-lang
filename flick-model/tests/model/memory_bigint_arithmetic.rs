//! Independent unbounded-integer vectors and real roots through every fuel cut.
use crate::causes;
use flick_core::{
    ir::{
        BinOp, Lit,
        memory::{Block, Exit, Function, Global, Instruction, Name, Operation, Program, ValueRep},
    },
    numeric::{Arith, Num},
    opt::perceus,
};
use flick_vm::tier0::{Error, Heap, ProgramOutcome, execute_program_outcome};

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

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn program(op: Arith) -> Program {
    Program {
        globals: vec![Global {
            name: Name::new("BigIntArithmetic", "run"),
            initializer: 0,
        }],
        functions: [
            (
                0,
                function(
                    &[],
                    vec![bind(
                        0,
                        Operation::Closure {
                            code: 1,
                            captures: vec![],
                        },
                    )],
                    0,
                ),
            ),
            (
                1,
                function(
                    &[0, 1],
                    vec![bind(
                        2,
                        Operation::Closure {
                            code: 2,
                            captures: vec![1],
                        },
                    )],
                    2,
                ),
            ),
            (
                2,
                function(
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
                            Operation::Binary {
                                op: BinOp::Arith(op, Num::BigInt),
                                left: 2,
                                right: 1,
                            },
                        ),
                    ],
                    3,
                ),
            ),
        ]
        .into(),
    }
}

fn vector(op: Arith, left: &str, right: &str, expected: &str) {
    let mut heap = Heap::new();
    let a = heap
        .literal(&Lit::BigInt(left.into()))
        .expect("external left");
    let b = heap
        .literal(&Lit::BigInt(right.into()))
        .expect("external right");
    let args = vec![
        heap.dup(&a).expect("left input"),
        heap.dup(&b).expect("right input"),
    ];
    let report = execute_program_outcome(
        &mut heap,
        &program(op),
        &Name::new("BigIntArithmetic", "run"),
        args,
        100_000,
    );
    let ProgramOutcome::Returned(value) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    assert_eq!(heap.bigint(&value), Ok(expected), "{op:?} {left} {right}");
    heap.drop_value(value).expect("owned result");
    assert_eq!(heap.bigint(&a), Ok(left));
    assert_eq!(heap.bigint(&b), Ok(right));
    assert_eq!(heap.reference_count(&a), Ok(1));
    assert_eq!(heap.reference_count(&b), Ok(1));
    heap.drop_value(a).expect("left external release");
    heap.drop_value(b).expect("right external release");
    let stats = heap.finish().expect("actual zero occupied slots");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn independent_vectors_exceed_machine_words_and_preserve_signs_and_radices() {
    for (op, a, b, expected) in [
        (
            Arith::Add,
            "340282366920938463463374607431768211455",
            "2",
            "340282366920938463463374607431768211457",
        ),
        (
            Arith::Add,
            "-9999999999999999999999999999999999999999",
            "1",
            "-9999999999999999999999999999999999999998",
        ),
        (
            Arith::Sub,
            "1",
            "340282366920938463463374607431768211456",
            "-340282366920938463463374607431768211455",
        ),
        (
            Arith::Sub,
            "-123456789012345678901234567890",
            "-987654321098765432109876543210",
            "864197532086419753208641975320",
        ),
        (
            Arith::Mul,
            "123456789012345678901234567890",
            "987654321098765432109876543210",
            "121932631137021795226185032733622923332237463801111263526900",
        ),
        (
            Arith::Mul,
            "-0X100000000000000000000000000000000",
            "0B101",
            "-1701411834604692317316873037158841057280",
        ),
        (
            Arith::Div,
            "-1234567890123456789012345678901234567890",
            "98765432109876543210",
            "-12499999886093750001",
        ),
        (
            Arith::Rem,
            "-1234567890123456789012345678901234567890",
            "98765432109876543210",
            "-54205246805420524680",
        ),
        (
            Arith::Div,
            "1234567890123456789012345678901234567890",
            "-98765432109876543210",
            "-12499999886093750001",
        ),
        (
            Arith::Rem,
            "1234567890123456789012345678901234567890",
            "-98765432109876543210",
            "54205246805420524680",
        ),
        (
            Arith::Pow,
            "-123456789",
            "7",
            "-437124189620885610010004822109262358637075660656881926429",
        ),
        (
            Arith::Pow,
            "2",
            "256",
            "115792089237316195423570985008687907853269984665640564039457584007913129639936",
        ),
        (Arith::Pow, "0", "0", "1"),
        (Arith::Add, "-0b0", "0X0", "0"),
        (
            Arith::Mul,
            "0",
            "-1000000000000000000000000000000000000000000000",
            "0",
        ),
    ] {
        vector(op, a, b, expected);
    }
    for (a, b, quotient, remainder) in [
        ("-7", "-3", "2", "-1"),
        ("-7", "3", "-2", "-1"),
        ("7", "-3", "-2", "1"),
        ("7", "3", "2", "1"),
        ("-7", "0", "0", "0"),
        ("7", "0", "0", "0"),
        ("0", "-3", "0", "0"),
        ("0", "0", "0", "0"),
    ] {
        vector(Arith::Div, a, b, quotient);
        vector(Arith::Rem, a, b, remainder);
    }
}

#[test]
fn exponent_magnitudes_are_not_narrowed_to_a_machine_word() {
    for (base, exponent, expected) in [
        ("-1", "18446744073709551616", "1"),
        ("-1", "18446744073709551617", "-1"),
        ("1", "99999999999999999999999999999999999999", "1"),
        ("0", "18446744073709551616", "0"),
        ("-0b0", "-0x0", "1"),
    ] {
        vector(Arith::Pow, base, exponent, expected);
    }
}

#[test]
fn invalid_or_noninteger_inputs_fault_without_losing_owned_arguments() {
    for (literal, expected) in [
        (Lit::BigInt("0x".into()), Error::InvalidBigInt("0x".into())),
        (
            Lit::BigInt("0B102".into()),
            Error::InvalidBigInt("0B102".into()),
        ),
        (Lit::String("not a BigInt".into()), Error::ExpectedBigInt),
    ] {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&literal)
            .expect("external malformed/other input");
        let argument = heap.dup(&outside).expect("owned argument");
        let right = heap.literal(&Lit::BigInt("1".into())).expect("right input");
        let report = execute_program_outcome(
            &mut heap,
            &program(Arith::Add),
            &Name::new("BigIntArithmetic", "run"),
            vec![argument, right],
            100_000,
        );
        {
            let diagnostic = "exact outcome expectation";
            let observed_outcome = report.outcome;
            let correct =
                matches!(&observed_outcome, ProgramOutcome::Fault(error) if *error == expected);
            let detail = format!("{observed_outcome:?}");
            observed_outcome
                .release(&mut heap)
                .expect("every owned outcome released before assertion");
            assert!(correct, "{diagnostic}: {detail}");
        };
        assert_eq!(heap.reference_count(&outside), Ok(1));
        heap.drop_value(outside).expect("caller root release");
        let stats = heap.finish().expect("fault releases both staged inputs");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn dense_small_integer_oracle_covers_carries_borrows_signs_and_zero_divisors() {
    for left in -12_i64..=12 {
        for right in -12_i64..=12 {
            for op in [Arith::Add, Arith::Sub, Arith::Mul, Arith::Div, Arith::Rem] {
                let expected = match op {
                    Arith::Add => left + right,
                    Arith::Sub => left - right,
                    Arith::Mul => left * right,
                    Arith::Div if right != 0 => left / right,
                    Arith::Rem if right != 0 => left % right,
                    Arith::Div | Arith::Rem => 0,
                    Arith::Pow => unreachable!("powers have their own exponent range"),
                };
                vector(
                    op,
                    &left.to_string(),
                    &right.to_string(),
                    &expected.to_string(),
                );
            }
        }
        for exponent in 0..=7 {
            vector(
                Arith::Pow,
                &left.to_string(),
                &exponent.to_string(),
                &left.pow(exponent).to_string(),
            );
        }
    }
}

#[test]
fn a_shared_operand_is_not_mutated_or_consumed_through_two_owned_aliases() {
    let mut heap = Heap::new();
    let outside = heap
        .literal(&Lit::BigInt(
            "10000000000000000000000000000000000000000".into(),
        ))
        .expect("external input");
    let args = vec![
        heap.dup(&outside).expect("first argument"),
        heap.dup(&outside).expect("second argument"),
    ];
    let report = execute_program_outcome(
        &mut heap,
        &program(Arith::Mul),
        &Name::new("BigIntArithmetic", "run"),
        args,
        100_000,
    );
    let ProgramOutcome::Returned(value) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    assert_eq!(
        heap.bigint(&value),
        Ok("100000000000000000000000000000000000000000000000000000000000000000000000000000000")
    );
    heap.drop_value(value).expect("owned result");
    assert_eq!(heap.reference_count(&outside), Ok(1));
    assert_eq!(
        heap.bigint(&outside),
        Ok("10000000000000000000000000000000000000000")
    );
    heap.drop_value(outside).expect("external input release");
    let stats = heap.finish().expect("actual zero shared-operand roots");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn every_fuel_prefix_releases_arithmetic_roots_and_negative_exponents_are_specific_defects() {
    let mut prefixes = 0;
    for (op, a, b, expected) in [
        (Arith::Add, "999999999999", "1", Some("1000000000000")),
        (Arith::Sub, "-123456789", "987654321", Some("-1111111110")),
        (
            Arith::Mul,
            "123456789",
            "987654321",
            Some("121932631112635269"),
        ),
        (Arith::Div, "-123456789012", "1234", Some("-100046020")),
        (Arith::Rem, "-123456789012", "1234", Some("-332")),
        (Arith::Pow, "-5", "17", Some("-762939453125")),
        (Arith::Pow, "2", "-1", None),
    ] {
        let program = program(op);
        let mut completed = false;
        for fuel in 0..=100_000 {
            let mut heap = Heap::new();
            let left = heap
                .literal(&Lit::BigInt(a.into()))
                .expect("left external root");
            let right = heap
                .literal(&Lit::BigInt(b.into()))
                .expect("right external root");
            let args = vec![
                heap.dup(&left).expect("left input"),
                heap.dup(&right).expect("right input"),
            ];
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("BigIntArithmetic", "run"),
                args,
                fuel,
            );
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(Some(heap.bigint(&value).expect("BigInt result")), expected);
                    heap.drop_value(value).expect("owned outcome release");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                ProgramOutcome::Failed(cause) if expected.is_none() => {
                    assert_eq!(
                        causes::defect_error(&mut heap, cause),
                        Error::BigIntNegativeExponent
                    );
                    // Stable repository diagnostic, not a claim about V8's wording.
                    assert_eq!(
                        Error::BigIntNegativeExponent.to_string(),
                        "RangeError: Exponent must be non-negative"
                    );
                    completed = true;
                }
                outcome => {
                    let diagnostic = format!("{op:?} {a} {b} fuel={fuel}: {outcome:?}");
                    causes::unexpected(&mut heap, outcome, &diagnostic)
                }
            }
            assert_eq!(report.stdout, "");
            assert_eq!(heap.bigint(&left), Ok(a));
            assert_eq!(heap.bigint(&right), Ok(b));
            assert_eq!(heap.reference_count(&left), Ok(1));
            assert_eq!(heap.reference_count(&right), Ok(1));
            heap.drop_value(left).expect("left caller release");
            heap.drop_value(right).expect("right caller release");
            let stats = heap
                .finish()
                .expect("all prefix objects released without sweep");
            assert_eq!(stats.allocations, stats.frees);
            prefixes += 1;
            if completed {
                eprintln!("{op:?} {a} {b} first terminal fuel={fuel}");
                break;
            }
        }
        assert!(completed, "ordinary unchanged fuel cap reaches {op:?}");
    }
    eprintln!("checked {prefixes} consecutive BigInt arithmetic prefixes");
}
