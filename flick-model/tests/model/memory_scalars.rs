//! Independent binary64 vectors and ownership for exact scalar primitives.
use crate::causes;
use flick_core::ir::{
    Lit,
    memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};

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
    if kind == Intrinsic::NumberToFloat {
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
            name: Name::new("Scalars", "test"),
            initializer: 0,
        }],
        functions: stages
            .into_iter()
            .enumerate()
            .map(|(i, function)| (u32::try_from(i).expect("small code ID"), function))
            .collect(),
    }
}

fn number_result(literal: &Lit, expected: u64) {
    let mut heap = Heap::new();
    let outside = heap.literal(literal).expect("scalar input");
    let argument = heap.dup(&outside).expect("external alias");
    let report = execute_program_outcome(
        &mut heap,
        &program(Intrinsic::NumberToFloat),
        &Name::new("Scalars", "test"),
        vec![argument],
        1_000_000,
    );
    let ProgramOutcome::Returned(Value::Float(value)) = report.outcome else {
        causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
    };
    assert_eq!(value.to_bits(), expected, "{literal:?}");
    if let Lit::BigInt(text) = literal {
        assert_eq!(heap.bigint(&outside), Ok(text.as_str()));
    }
    heap.drop_value(outside).expect("external input owner");
    let stats = heap
        .finish()
        .expect("numeric input and execution roots released");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn integer_and_bigint_conversion_round_nearest_with_ties_to_even() {
    for (value, bits) in [
        (0, 0),
        (1, 0x3ff0_0000_0000_0000),
        (-1, 0xbff0_0000_0000_0000),
        (i32::MAX, 0x41df_ffff_ffc0_0000),
        (i32::MIN, 0xc1e0_0000_0000_0000),
    ] {
        number_result(&Lit::Int(value), bits);
    }
    for (text, bits) in [
        ("0", 0),
        ("-0", 0),
        ("-0b0", 0),
        ("1", 0x3ff0_0000_0000_0000),
        ("9007199254740991", 0x433f_ffff_ffff_ffff),
        ("9007199254740992", 0x4340_0000_0000_0000),
        ("9007199254740993", 0x4340_0000_0000_0000),
        ("9007199254740994", 0x4340_0000_0000_0001),
        ("9007199254740995", 0x4340_0000_0000_0002),
        ("9007199254740997", 0x4340_0000_0000_0002),
        ("9007199254740999", 0x4340_0000_0000_0004),
        ("-9007199254740993", 0xc340_0000_0000_0000),
        ("-9007199254740995", 0xc340_0000_0000_0002),
        ("18014398509481983", 0x4350_0000_0000_0000),
        ("18014398509481985", 0x4350_0000_0000_0000),
        ("18446744073709551615", 0x43f0_0000_0000_0000),
        ("0x20000000000001", 0x4340_0000_0000_0000),
        ("0x20000000000003", 0x4340_0000_0000_0002),
        ("0xff", 0x406f_e000_0000_0000),
        ("0o377", 0x406f_e000_0000_0000),
        ("0b11111111", 0x406f_e000_0000_0000),
        ("0XFF", 0x406f_e000_0000_0000),
        ("0O377", 0x406f_e000_0000_0000),
        ("0B11111111", 0x406f_e000_0000_0000),
        ("-0XFF", 0xc06f_e000_0000_0000),
    ] {
        number_result(&Lit::BigInt(text.into()), bits);
    }
}

#[test]
fn three_hundred_nine_digit_boundary_rounds_exactly_before_overflow() {
    // max finite = 2^1024 - 2^971; overflow midpoint = 2^1024 - 2^970.
    // At the midpoint the all-ones finite significand is odd: ties select infinity.
    for (digits, bits) in [
        (
            format!("0x{}8{}", "f".repeat(13), "0".repeat(242)),
            0x7fef_ffff_ffff_ffff,
        ),
        (
            format!("0x{}b{}", "f".repeat(13), "f".repeat(242)),
            0x7fef_ffff_ffff_ffff,
        ),
        (
            format!("0x{}c{}", "f".repeat(13), "0".repeat(242)),
            0x7ff0_0000_0000_0000,
        ),
        (format!("0x1{}", "0".repeat(256)), 0x7ff0_0000_0000_0000),
        ("9".repeat(400), 0x7ff0_0000_0000_0000),
        (format!("{}1", "0".repeat(400)), 0x3ff0_0000_0000_0000),
    ] {
        number_result(&Lit::BigInt(digits.clone()), bits);
        number_result(&Lit::BigInt(format!("-{digits}")), bits | 1_u64 << 63);
    }
}

#[test]
fn self_last_selection_preserves_nan_payloads_and_directional_zero_bits() {
    const P: u64 = 0;
    const N: u64 = 0x8000_0000_0000_0000;
    const A: u64 = 0x7ff8_0000_0000_0001;
    const B: u64 = 0xfff8_0000_0000_0002;
    const ONE: u64 = 0x3ff0_0000_0000_0000;
    const TWO: u64 = 0x4000_0000_0000_0000;
    const POS_INF: u64 = 0x7ff0_0000_0000_0000;
    const NEG_INF: u64 = 0xfff0_0000_0000_0000;
    for (that, this, minimum, maximum) in [
        (P, N, N, N),
        (N, P, P, P),
        (A, ONE, A, ONE),
        (ONE, B, B, ONE),
        (A, B, B, B),
        (B, A, A, A),
        (TWO, ONE, ONE, TWO),
        (ONE, ONE, ONE, ONE),
        (NEG_INF, POS_INF, NEG_INF, POS_INF),
        (1, P, P, 1),
        (0x8000_0000_0000_0001, 1, 0x8000_0000_0000_0001, 1),
    ] {
        for (kind, expected) in [
            (Intrinsic::FloatMin, minimum),
            (Intrinsic::FloatMax, maximum),
        ] {
            let mut heap = Heap::new();
            let report = execute_program_outcome(
                &mut heap,
                &program(kind),
                &Name::new("Scalars", "test"),
                vec![
                    Value::Float(f64::from_bits(that)),
                    Value::Float(f64::from_bits(this)),
                ],
                1_000,
            );
            let ProgramOutcome::Returned(Value::Float(result)) = report.outcome else {
                causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
            };
            assert_eq!(result.to_bits(), expected, "{kind:?}({that:x})({this:x})");
            assert_eq!(
                heap.finish()
                    .expect("all selection roots released")
                    .live_objects,
                0
            );
        }
    }
}

#[test]
fn malformed_and_wrong_operands_release_staged_aliases_without_touching_external_roots() {
    for (kind, invalid) in [
        (
            Intrinsic::NumberToFloat,
            Lit::String("not a numeric input".into()),
        ),
        (Intrinsic::NumberToFloat, Lit::BigInt("0xg".into())),
        (Intrinsic::FloatMin, Lit::BigInt("123".into())),
        (Intrinsic::FloatMax, Lit::String("not a Float".into())),
    ] {
        let mut heap = Heap::new();
        let outside = heap.literal(&invalid).expect("external input root");
        let argument = heap.dup(&outside).expect("owned input alias");
        let args = if kind == Intrinsic::NumberToFloat {
            vec![argument]
        } else {
            vec![argument, Value::Float(1.0)]
        };
        let report = execute_program_outcome(
            &mut heap,
            &program(kind),
            &Name::new("Scalars", "test"),
            args,
            1_000,
        );
        match report.outcome {
            ProgramOutcome::Fault(Error::InvalidBigInt(text)) => assert_eq!(text, "0xg"),
            ProgramOutcome::Fault(Error::ExpectedBigInt) if kind == Intrinsic::NumberToFloat => {}
            ProgramOutcome::Fault(Error::UnsupportedPrimitive(_))
                if kind != Intrinsic::NumberToFloat => {}
            other => {
                let diagnostic = format!("specific invalid operand fault: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        match invalid {
            Lit::String(text) => assert_eq!(heap.string(&outside), Ok(text.as_str())),
            Lit::BigInt(text) => assert_eq!(heap.bigint(&outside), Ok(text.as_str())),
            _ => panic!("owned input"),
        }
        heap.drop_value(outside).expect("external owner");
        assert_eq!(
            heap.finish()
                .expect("fault unwinds staged aliases")
                .live_objects,
            0
        );
    }
}

#[test]
fn every_fuel_prefix_reaches_conversion_or_selection_and_releases_real_roots() {
    for kind in [
        Intrinsic::NumberToFloat,
        Intrinsic::FloatMin,
        Intrinsic::FloatMax,
    ] {
        let mut completed = false;
        let mut exhausted = false;
        for fuel in 0..=100_000 {
            let mut heap = Heap::new();
            let literal = if kind == Intrinsic::NumberToFloat {
                Lit::BigInt("9007199254740993".into())
            } else {
                Lit::String("outside".into())
            };
            let outside = heap.literal(&literal).expect("external root");
            let args = if kind == Intrinsic::NumberToFloat {
                vec![heap.dup(&outside).expect("input alias")]
            } else {
                vec![Value::Float(0.0), Value::Float(-0.0)]
            };
            let report = execute_program_outcome(
                &mut heap,
                &program(kind),
                &Name::new("Scalars", "test"),
                args,
                fuel,
            );
            match report.outcome {
                ProgramOutcome::Returned(Value::Float(value)) => {
                    let expected = if kind == Intrinsic::NumberToFloat {
                        0x4340_0000_0000_0000
                    } else {
                        0x8000_0000_0000_0000
                    };
                    assert_eq!(value.to_bits(), expected);
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => exhausted = true,
                other => {
                    let diagnostic = format!("fuel={fuel}: {other:?}");
                    causes::unexpected(&mut heap, other, &diagnostic)
                }
            }
            match &literal {
                Lit::BigInt(text) => assert_eq!(heap.bigint(&outside), Ok(text.as_str())),
                Lit::String(text) => assert_eq!(heap.string(&outside), Ok(text.as_str())),
                _ => panic!("owned external root"),
            }
            heap.drop_value(outside).expect("external owner");
            let stats = heap.finish().expect("no real roots leaked on fuel cut");
            assert_eq!(stats.live_objects, 0);
            assert_eq!(stats.allocations, stats.frees);
            if completed {
                break;
            }
        }
        assert!(
            completed && exhausted,
            "all prefixes and semantic completion"
        );
    }
}

#[test]
fn float_number_identity_preserves_every_tested_binary64_bit_pattern() {
    for bits in [
        0,
        0x8000_0000_0000_0000,
        0x3ff0_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0001,
        0xfff8_0000_0000_0002,
    ] {
        number_result(&Lit::Float(f64::from_bits(bits)), bits);
    }
}
