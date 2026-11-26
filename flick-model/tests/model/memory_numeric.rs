//! Raw primitive semantics and explicit ownership on faults and fuel cuts.
use crate::causes;
use flick_core::ir::{
    Lit,
    memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
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
    let unary = function(
        &[0, 1],
        vec![bind(
            2,
            Operation::Intrinsic {
                kind,
                args: vec![1],
            },
        )],
        2,
    );
    let stages = if kind == Intrinsic::PrimitiveHash {
        vec![init, unary]
    } else {
        vec![
            init,
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
                    bind(
                        3,
                        Operation::Intrinsic {
                            kind,
                            args: vec![2, 1],
                        },
                    ),
                ],
                3,
            ),
        ]
    };
    Program {
        globals: vec![Global {
            name: Name::new("Numeric", "test"),
            initializer: 0,
        }],
        functions: stages
            .into_iter()
            .enumerate()
            .map(|(i, f)| (u32::try_from(i).expect("small"), f))
            .collect(),
    }
}

fn hash_result(heap: &mut Heap, value: Value) -> i32 {
    let report = execute_program_outcome(
        heap,
        &program(Intrinsic::PrimitiveHash),
        &Name::new("Numeric", "test"),
        vec![value],
        100_000,
    );
    let ProgramOutcome::Returned(Value::Int(value)) = report.outcome else {
        causes::unexpected(heap, report.outcome, "unexpected owned outcome")
    };
    value
}

#[test]
fn primitive_hash_vectors_match_rt_go_without_integer_or_nan_narrowing() {
    for (literal, expected) in [
        (Lit::String(String::new()), 5381),
        (Lit::String("abc".into()), 193_415_941),
        (Lit::String("😀".into()), 5_191_800),
        (Lit::String("e\u{301}".into()), 5_868_353),
        (Lit::String("é".into()), 177_484),
        (Lit::Int(i32::MAX), 1_073_741_823),
        (Lit::Int(i32::MIN), -1_073_741_824),
        (Lit::Int(-1), -1),
        (Lit::Float(0.0), 0),
        (Lit::Float(-0.0), 0),
        (Lit::Float(1.5), -182_423_092),
        (Lit::Float(-1.5), 932_903_442),
        (Lit::Float(2_147_483_648.0), -942_848_677),
        (Lit::Float(f64::NAN), 389_652_371),
        (
            Lit::Float(f64::from_bits(0xfff0_0000_0000_0001)),
            389_652_371,
        ),
        (Lit::Float(f64::INFINITY), -364_780_647),
        (Lit::Float(f64::NEG_INFINITY), -671_614_997),
        (Lit::Bool(true), 438_880_922),
        (Lit::Bool(false), 671_588_899),
        (Lit::Unit, 219_449_338),
        (
            Lit::BigInt("123456789012345678901234567890".into()),
            -235_053_180,
        ),
        (
            Lit::BigInt("0x18ee90ff6c373e0ee4e3f0ad2".into()),
            -235_053_180,
        ),
    ] {
        let mut heap = Heap::new();
        let value = heap.literal(&literal).expect("input");
        assert_eq!(hash_result(&mut heap, value), expected, "{literal:?}");
        let memory = heap.finish().expect("hash released every input root");
        assert_eq!(memory.allocations, memory.frees);
    }
    for (name, expected) in [
        ("LT", 571_429_162),
        ("EQ", 743_422_407),
        ("GT", -679_922_752),
    ] {
        let mut heap = Heap::new();
        let value = heap
            .construct(&format!("Prelude.Ordering.{name}"), vec![])
            .expect("ordering");
        assert_eq!(hash_result(&mut heap, value), expected);
        heap.finish().expect("Ordering released");
    }
}

#[test]
fn primitive_total_order_handles_nan_utf16_signed_zero_and_unbounded_bigints() {
    for (left, right, expected) in [
        (Lit::Float(f64::NAN), Lit::Float(f64::INFINITY), "GT"),
        (Lit::Float(f64::NAN), Lit::Float(f64::NAN), "EQ"),
        (Lit::Float(f64::NEG_INFINITY), Lit::Float(f64::NAN), "LT"),
        (Lit::Float(-0.0), Lit::Float(0.0), "EQ"),
        (
            Lit::String("😀".into()),
            Lit::String("\u{e000}".into()),
            "LT",
        ),
        (Lit::Bool(false), Lit::Bool(true), "LT"),
        (Lit::Int(i32::MIN), Lit::Int(i32::MAX), "LT"),
        (
            Lit::BigInt("999999999999999999999999999999999999999".into()),
            Lit::BigInt("1000000000000000000000000000000000000000".into()),
            "LT",
        ),
        (
            Lit::BigInt("-999999999999999999999999999999999999999".into()),
            Lit::BigInt("-1000000000000000000000000000000000000000".into()),
            "GT",
        ),
        (
            Lit::BigInt("0x10000000000000000".into()),
            Lit::BigInt("18446744073709551616".into()),
            "EQ",
        ),
    ] {
        let mut heap = Heap::new();
        let left = heap.literal(&left).expect("left");
        let right = heap.literal(&right).expect("right");
        let report = execute_program_outcome(
            &mut heap,
            &program(Intrinsic::PrimitiveCompare),
            &Name::new("Numeric", "test"),
            vec![left, right],
            100_000,
        );
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(
            heap.tag(&value),
            Ok(format!("Prelude.Ordering.{expected}").as_str())
        );
        heap.drop_value(value).expect("result release");
        let memory = heap.finish().expect("compare released inputs and Ordering");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn primitive_numeric_fuel_prefixes_keep_external_aliases_and_release_real_roots() {
    for kind in [Intrinsic::PrimitiveCompare, Intrinsic::PrimitiveHash] {
        let program = program(kind);
        let mut completed = false;
        for fuel in 0..=256 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("😀".into()))
                .expect("external root");
            let mut arguments = vec![heap.dup(&external).expect("input alias")];
            if kind == Intrinsic::PrimitiveCompare {
                arguments.push(
                    heap.literal(&Lit::String("\u{e000}".into()))
                        .expect("right"),
                );
            }
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Numeric", "test"),
                arguments,
                fuel,
            );
            let correct = match report.outcome {
                ProgramOutcome::Returned(value) => {
                    completed = true;
                    let correct = if kind == Intrinsic::PrimitiveHash {
                        value == Value::Int(5_191_800)
                    } else {
                        heap.tag(&value) == Ok("Prelude.Ordering.LT")
                    };
                    heap.drop_value(value).expect("release outcome");
                    correct
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => true,
                ProgramOutcome::Raised(value) => {
                    heap.drop_value(value).expect("unexpected failure");
                    false
                }
                ProgramOutcome::Failed(cause) => {
                    heap.drop_cause(cause).expect("unexpected cause release");
                    false
                }
                ProgramOutcome::Fault(_) => false,
            };
            assert_eq!(heap.string(&external), Ok("😀"));
            assert_eq!(heap.stats().live_objects, 1, "{kind:?} fuel={fuel}");
            heap.drop_value(external).expect("external release");
            let memory = heap.finish().expect("all actual internal roots released");
            assert_eq!(memory.allocations, memory.frees);
            assert!(correct, "{kind:?} fuel={fuel}");
        }
        assert!(completed, "sweep includes successful completion");
    }
}

#[test]
fn malformed_primitive_arguments_fault_without_losing_allocated_operands() {
    for (kind, position) in [
        (Intrinsic::PrimitiveHash, 0),
        (Intrinsic::PrimitiveCompare, 0),
        (Intrinsic::PrimitiveCompare, 1),
    ] {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let child = heap
            .literal(&Lit::BigInt("12345678901234567890".into()))
            .expect("owned child");
        let bad = heap
            .record(vec![("value".into(), child)])
            .expect("not primitive");
        let mut arguments = vec![bad];
        if kind == Intrinsic::PrimitiveCompare {
            let other = heap
                .literal(&Lit::String("value".into()))
                .expect("allocated other");
            if position == 0 {
                arguments.push(other);
            } else {
                arguments.insert(0, other);
            }
        }
        let report = execute_program_outcome(
            &mut heap,
            &program(kind),
            &Name::new("Numeric", "test"),
            arguments,
            1000,
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
        assert_eq!(heap.stats().live_objects, 1);
        assert_eq!(heap.string(&external), Ok("outside"));
        heap.drop_value(external).expect("external release");
        heap.finish()
            .expect("fault released record children and other operands");
    }
}

#[test]
fn bigint_radix_conversion_is_metered_and_unwinds_every_owned_prefix() {
    for kind in [Intrinsic::PrimitiveCompare, Intrinsic::PrimitiveHash] {
        let program = program(kind);
        let mut completed = false;
        // Includes the complete two-operand normalization as well as every
        // smaller interrupted prefix; this does not alter execution budgets.
        for fuel in 0..=2048 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::BigInt("0x18ee90ff6c373e0ee4e3f0ad2".into()))
                .expect("caller bigint");
            let mut arguments = vec![heap.dup(&external).expect("argument alias")];
            if kind == Intrinsic::PrimitiveCompare {
                arguments.push(
                    heap.literal(&Lit::BigInt("123456789012345678901234567890".into()))
                        .expect("decimal operand"),
                );
            }
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Numeric", "test"),
                arguments,
                fuel,
            );
            let correct = match report.outcome {
                ProgramOutcome::Returned(value) => {
                    completed = true;
                    let correct = if kind == Intrinsic::PrimitiveHash {
                        value == Value::Int(-235_053_180)
                    } else {
                        heap.tag(&value) == Ok("Prelude.Ordering.EQ")
                    };
                    heap.drop_value(value).expect("outcome release");
                    correct
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => true,
                ProgramOutcome::Raised(value) => {
                    heap.drop_value(value).expect("unexpected payload");
                    false
                }
                ProgramOutcome::Failed(cause) => {
                    heap.drop_cause(cause).expect("unexpected cause release");
                    false
                }
                ProgramOutcome::Fault(_) => false,
            };
            assert_eq!(heap.bigint(&external), Ok("0x18ee90ff6c373e0ee4e3f0ad2"));
            assert_eq!(heap.stats().live_objects, 1, "{kind:?} fuel={fuel}");
            heap.drop_value(external).expect("caller releases root");
            heap.finish()
                .expect("fuel failure drops every actual internal root");
            assert!(correct, "{kind:?} fuel={fuel}");
        }
        assert!(completed, "fuel sweep includes completed radix conversion");
    }
}
