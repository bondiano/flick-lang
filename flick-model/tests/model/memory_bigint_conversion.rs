//! ECMA primitive global `BigInt` domains, specific faults and real-root fuel cuts.
use crate::causes;
use flick_core::{
    ir::{
        Lit,
        memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
    },
    opt::perceus,
};
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
use std::collections::BTreeMap;

const FUEL: u64 = 100_000;
const MAX_INTEGER: &str = concat!(
    "179769313486231570814527423731704356798070567525844996598917476803157260780028538760589558632766878171",
    "540458953514382464234321326889464182768467546703537516986049910576551282076245490090389328944075868508",
    "455133942304583236903222948165808559332123348274797826204144723168738177180919299881250404026184124858368",
);

fn function(params: Vec<u32>, operation: Operation) -> Function {
    perceus::insert(&Function {
        params,
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 2,
                value: operation,
            }],
            exit: Exit::Return(2),
        },
    })
    .expect("ownership insertion")
}
fn program() -> Program {
    Program {
        globals: vec![Global {
            name: Name::new("BigIntConversion", "run"),
            initializer: 0,
        }],
        functions: [
            (
                0,
                function(
                    vec![],
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                ),
            ),
            (
                1,
                function(
                    vec![0, 1],
                    Operation::Intrinsic {
                        kind: Intrinsic::ToBigInt,
                        args: vec![1],
                    },
                ),
            ),
        ]
        .into(),
        op_spans: BTreeMap::new(),
    }
}

fn preserves(heap: &Heap, value: &Value, input: &Lit) {
    match input {
        Lit::String(text) => assert_eq!(heap.string(value), Ok(text.as_str())),
        Lit::BigInt(text) => assert_eq!(heap.bigint(value), Ok(text.as_str())),
        Lit::Float(expected) => {
            let Value::Float(actual) = value else {
                panic!("preserved Float representation")
            };
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        Lit::Float32(expected) => {
            let Value::Float32(actual) = value else {
                panic!("preserved Float32 representation")
            };
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        Lit::Int(expected) => assert_eq!(value, &Value::Int(*expected)),
        Lit::Int8(expected) => assert_eq!(value, &Value::Int8(*expected)),
        Lit::Int16(expected) => assert_eq!(value, &Value::Int16(*expected)),
        Lit::Int64(expected) => assert_eq!(value, &Value::Int64(*expected)),
        Lit::UInt8(expected) => assert_eq!(value, &Value::UInt8(*expected)),
        Lit::UInt16(expected) => assert_eq!(value, &Value::UInt16(*expected)),
        Lit::UInt32(expected) => assert_eq!(value, &Value::UInt32(*expected)),
        Lit::UInt64(expected) => assert_eq!(value, &Value::UInt64(*expected)),
        Lit::Bool(expected) => assert_eq!(value, &Value::Bool(*expected)),
        Lit::Unit => assert_eq!(value, &Value::Unit),
    }
    assert_eq!(
        heap.reference_count(value),
        Ok(usize::from(value.object_id().is_some()))
    );
}

fn check(input: &Lit, expected: Result<&str, Error>) {
    let mut heap = Heap::new();
    let external = heap.literal(input).expect("external input");
    let argument = heap.dup(&external).expect("owned argument");
    let report = execute_program_outcome(
        &mut heap,
        &program(),
        &Name::new("BigIntConversion", "run"),
        vec![argument],
        FUEL,
    );
    match (report.outcome, expected) {
        (ProgramOutcome::Returned(value), Ok(expected)) => {
            assert_eq!(heap.bigint(&value), Ok(expected));
            if matches!(input, Lit::BigInt(_)) {
                assert_eq!(
                    value.object_id(),
                    external.object_id(),
                    "BigInt identity retains the original object"
                );
            }
            heap.drop_value(value).expect("caller result release");
        }
        (ProgramOutcome::Failed(cause), Err(expected)) => {
            assert_eq!(causes::defect_error(&mut heap, cause), expected);
        }
        (outcome, expected) => {
            let diagnostic = format!("{input:?}: {outcome:?}; expected {expected:?}");
            causes::unexpected(&mut heap, outcome, &diagnostic)
        }
    }
    assert_eq!(report.stdout, "");
    preserves(&heap, &external, input);
    heap.drop_value(external).expect("caller input release");
    let stats = heap
        .finish()
        .expect("actual zero occupied slots, not a sweep");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn primitive_domains_cover_signed_edges_and_full_binary64_magnitude_boundaries() {
    for (input, expected) in [
        (Lit::Int(i32::MIN), "-2147483648"),
        (Lit::Int(i32::MAX), "2147483647"),
        (Lit::Int(0), "0"),
        (Lit::Bool(false), "0"),
        (Lit::Bool(true), "1"),
        (
            Lit::BigInt("-0X100000000000000000000000000000000".into()),
            "-0X100000000000000000000000000000000",
        ),
        (Lit::Float(0.0), "0"),
        (Lit::Float(-0.0), "0"),
        (Lit::Float(1.0), "1"),
        (Lit::Float(-1.0), "-1"),
        (Lit::Float(9_007_199_254_740_991.0), "9007199254740991"),
        (Lit::Float(9_007_199_254_740_992.0), "9007199254740992"),
        (
            Lit::Float(1e100),
            "10000000000000000159028911097599180468360808563945281389781327557747838772170381060813469985856815104",
        ),
        (Lit::Float(f64::MAX), MAX_INTEGER),
    ] {
        check(&input, Ok(expected));
    }
    check(&Lit::Float(-f64::MAX), Ok(&format!("-{MAX_INTEGER}")));
}

#[test]
fn number_range_errors_and_undefined_type_errors_are_not_unsupported_or_typed_raises() {
    for value in [
        0.5,
        -1.5,
        f64::MIN_POSITIVE,
        f64::from_bits(1),
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        check(&Lit::Float(value), Err(Error::BigIntNonIntegralNumber));
    }
    check(&Lit::Unit, Err(Error::BigIntUndefined));
}

#[test]
fn strings_obey_string_integer_literal_not_the_broader_source_literal_normalizer() {
    for (input, expected) in [
        ("", "0"),
        ("  \r\n", "0"),
        ("+0012", "12"),
        ("-0012", "-12"),
        ("-0", "0"),
        ("+0", "0"),
        ("007", "7"),
        ("0Xff", "255"),
        ("0xFF", "255"),
        ("0O77", "63"),
        ("0o77", "63"),
        ("0B101", "5"),
        ("0b101", "5"),
        (
            "0X100000000000000000000000000000000",
            "340282366920938463463374607431768211456",
        ),
    ] {
        check(&Lit::String(input.into()), Ok(expected));
    }
    for input in [
        "+", "-", "+-1", "-+1", "++1", "--1", "+-0x1", "+0x1", "-0x1", "-0x0", "+0o1", "-0b1",
        "0x", "0o", "0b", "0b2", "0o8", "1_000", "1n", "1e3", "1.0", "+ 1", "1 2", "١",
    ] {
        check(&Lit::String(input.into()), Err(Error::BigIntInvalidString));
    }
}

#[test]
fn ecma_white_space_includes_bom_but_excludes_rust_only_and_zero_width_characters() {
    for code in [
        0x9, 0xa, 0xb, 0xc, 0xd, 0x20, 0xa0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003, 0x2004,
        0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f, 0x205f, 0x3000,
        0xfeff,
    ] {
        let space = char::from_u32(code).expect("ECMA whitespace scalar");
        check(&Lit::String(format!("{space}+012{space}")), Ok("12"));
        check(&Lit::String(space.to_string()), Ok("0"));
    }
    for code in [0x85, 0x180e, 0x200b] {
        let space = char::from_u32(code).expect("non-whitespace scalar");
        check(
            &Lit::String(format!("{space}12{space}")),
            Err(Error::BigIntInvalidString),
        );
    }
}

#[test]
fn every_prefix_through_results_and_each_semantic_fault_preserves_external_aliases() {
    let mut prefixes = 0;
    for (input, expected) in [
        (Lit::Int(i32::MIN), Ok("-2147483648")),
        (Lit::Bool(true), Ok("1")),
        (
            Lit::BigInt("123456789012345678901234567890".into()),
            Ok("123456789012345678901234567890"),
        ),
        (Lit::Float(1e20), Ok("100000000000000000000")),
        (
            Lit::String(" \u{feff}0x123456789abcdef ".into()),
            Ok("81985529216486895"),
        ),
        (Lit::String("+-1".into()), Err(Error::BigIntInvalidString)),
        (Lit::Float(0.5), Err(Error::BigIntNonIntegralNumber)),
        (Lit::Float(f64::NAN), Err(Error::BigIntNonIntegralNumber)),
        (Lit::Unit, Err(Error::BigIntUndefined)),
    ] {
        let program = program();
        let mut completed = false;
        for fuel in 0..=FUEL {
            let mut heap = Heap::new();
            let external = heap.literal(&input).expect("external input");
            let argument = heap.dup(&external).expect("owned argument");
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("BigIntConversion", "run"),
                vec![argument],
                fuel,
            );
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(Ok(heap.bigint(&value).expect("BigInt outcome")), expected);
                    heap.drop_value(value).expect("owned result release");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                ProgramOutcome::Failed(cause) => {
                    assert_eq!(Err(causes::defect_error(&mut heap, cause)), expected);
                    completed = true;
                }
                outcome => {
                    let diagnostic = format!("{input:?} fuel={fuel}: {outcome:?}");
                    causes::unexpected(&mut heap, outcome, &diagnostic)
                }
            }
            assert_eq!(report.stdout, "");
            preserves(&heap, &external, &input);
            heap.drop_value(external).expect("external input release");
            let stats = heap
                .finish()
                .expect("all prefix roots released without collection");
            assert_eq!(stats.allocations, stats.frees);
            prefixes += 1;
            if completed {
                eprintln!("{input:?} first terminal fuel={fuel}");
                break;
            }
        }
        assert!(
            completed,
            "unchanged cap reaches intended conversion outcome"
        );
    }
    eprintln!("checked {prefixes} consecutive global BigInt conversion prefixes");
}

#[test]
fn other_objects_remain_explicit_coercion_obligations_not_assumed_type_errors() {
    let mut heap = Heap::new();
    let field = heap.literal(&Lit::String("held".into())).expect("field");
    let original = heap.construct("Box", vec![field]).expect("object");
    let argument = heap.dup(&original).expect("owned input");
    let report = execute_program_outcome(
        &mut heap,
        &program(),
        &Name::new("BigIntConversion", "run"),
        vec![argument],
        FUEL,
    );
    {
        let diagnostic = "exact outcome expectation";
        let observed_outcome = report.outcome;
        let correct = matches!(&observed_outcome, ProgramOutcome::Fault(Error::UnsupportedPrimitive(message)) if message.contains("ToPrimitive"));
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    assert_eq!(heap.reference_count(&original), Ok(1));
    heap.drop_value(original).expect("external object release");
    let stats = heap
        .finish()
        .expect("unsupported coercion releases input roots");
    assert_eq!(stats.allocations, stats.frees);
}
