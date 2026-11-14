fn binary_program(op: BinOp, left: Lit, right: Lit) -> Program {
    program(vec![function(
        &[],
        vec![
            bind(0, Operation::Literal(left)),
            bind(1, Operation::Literal(right)),
            bind(
                2,
                Operation::Binary {
                    op,
                    left: 0,
                    right: 1,
                },
            ),
        ],
        2,
    )])
}

#[test]
fn primitives_preserve_wrapping_nan_signed_zero_and_utf16_order() {
    use flick_core::ir::BinOp;
    use flick_core::numeric::{Arith, Num};
    let cases = [
        (
            BinOp::Arith(Arith::Add, Num::Int),
            Lit::Int(i32::MAX),
            Lit::Int(1),
            Value::Int(i32::MIN),
        ),
        (
            BinOp::Arith(Arith::Div, Num::Int),
            Lit::Int(5),
            Lit::Int(0),
            Value::Int(0),
        ),
        (
            BinOp::Eq,
            Lit::Float(f64::NAN),
            Lit::Float(f64::NAN),
            Value::Bool(true),
        ),
        (
            BinOp::Same,
            Lit::Float(f64::NAN),
            Lit::Float(f64::NAN),
            Value::Bool(false),
        ),
        (
            BinOp::Eq,
            Lit::Float(-0.0),
            Lit::Float(0.0),
            Value::Bool(true),
        ),
        (
            BinOp::Lt,
            Lit::String("\u{10000}".into()),
            Lit::String("\u{e000}".into()),
            Value::Bool(true),
        ),
    ];
    for (op, left, right, expected) in cases {
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &binary_program(op, left, right),
            &Name::new("Main", "main"),
            vec![],
            100,
        )
        .expect("primitive");
        assert_eq!(ran.value, expected);
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
    for (base, exponent) in [(1.0, f64::NAN), (-1.0, f64::INFINITY)] {
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &binary_program(
                BinOp::Arith(Arith::Pow, Num::Float),
                Lit::Float(base),
                Lit::Float(exponent),
            ),
            &Name::new("Main", "main"),
            vec![],
            100,
        )
        .expect("Float pow");
        assert!(matches!(ran.value,Value::Float(value) if value.is_nan()));
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn bigint_operations_do_not_narrow_arbitrary_sized_literals() {
    use flick_core::ir::BinOp;
    use flick_core::ir::memory::Intrinsic;
    use flick_core::numeric::{Arith, Num};
    let mut heap = Heap::new();
    let comparison = binary_program(
        BinOp::Eq,
        Lit::BigInt("0xffffffffffffffffffffffffffffffff".into()),
        Lit::BigInt("340282366920938463463374607431768211455".into()),
    );
    let ran = execute_program(
        &mut heap,
        &comparison,
        &Name::new("Main", "main"),
        vec![],
        100,
    )
    .expect("unbounded comparison");
    assert_eq!(ran.value, Value::Bool(true));
    assert!(heap.finish().is_ok());
    let show = program(vec![function(
        &[],
        vec![
            bind(
                0,
                Operation::Literal(Lit::BigInt("0x100000000000000000000000000000000".into())),
            ),
            bind(
                1,
                Operation::Intrinsic {
                    kind: Intrinsic::Show,
                    args: vec![0],
                },
            ),
        ],
        1,
    )]);
    let ran = execute_program(&mut heap, &show, &Name::new("Main", "main"), vec![], 100)
        .expect("unbounded show");
    assert_eq!(
        heap.string(&ran.value),
        Ok("340282366920938463463374607431768211456n")
    );
    heap.drop_value(ran.value).expect("show result");
    let addition = binary_program(
        BinOp::Arith(Arith::Add, Num::BigInt),
        Lit::BigInt("1".into()),
        Lit::BigInt("2".into()),
    );
    let ran = execute_program(
        &mut heap,
        &addition,
        &Name::new("Main", "main"),
        vec![],
        100,
    )
    .expect("supported unbounded addition");
    assert_eq!(heap.bigint(&ran.value), Ok("3"));
    heap.drop_value(ran.value).expect("owned addition result");
    let stats = heap.finish().expect("actual zero occupied slots");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn list_concatenation_retains_each_shared_field_before_releasing_inputs() {
    use flick_core::ir::BinOp;
    let p = program(vec![function(
        &[],
        vec![
            bind(0, Operation::Literal(Lit::String("shared".into()))),
            bind(
                1,
                Operation::Construct {
                    tag: "$list".into(),
                    fields: vec![0, 0],
                },
            ),
            bind(
                2,
                Operation::Binary {
                    op: BinOp::ListConcat,
                    left: 1,
                    right: 1,
                },
            ),
        ],
        2,
    )]);
    let mut heap = Heap::new();
    let ran =
        execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 100).expect("concat");
    for index in 0..4 {
        let field = heap.field(&ran.value, index).expect("list item");
        assert_eq!(heap.string(field), Ok("shared"));
        assert_eq!(heap.reference_count(field), Ok(4));
    }
    heap.drop_value(ran.value).expect("list result");
    assert!(heap.finish().is_ok());
}

#[test]
fn structural_record_equality_uses_labels_not_written_field_order() {
    let p = program(vec![function(
        &[],
        vec![
            bind(0, Operation::Literal(Lit::String("same".into()))),
            bind(1, Operation::Literal(Lit::Int(7))),
            bind(
                2,
                Operation::Record {
                    fields: vec![("left".into(), 0), ("right".into(), 1)],
                },
            ),
            bind(
                3,
                Operation::Record {
                    fields: vec![("right".into(), 1), ("left".into(), 0)],
                },
            ),
            bind(
                4,
                Operation::Binary {
                    op: BinOp::Eq,
                    left: 2,
                    right: 3,
                },
            ),
        ],
        4,
    )]);
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 100)
        .expect("record equality");
    assert_eq!(ran.value, Value::Bool(true));
    assert!(heap.finish().is_ok());
}

#[test]
fn tagged_nominal_tag_projection_returns_an_independent_owned_string() {
    let p = program(vec![function(
        &[],
        vec![
            bind(
                0,
                Operation::Construct {
                    tag: "Colors.Color.Red".into(),
                    fields: vec![],
                },
            ),
            bind(
                1,
                Operation::Field {
                    object: 0,
                    field: "_tag".into(),
                },
            ),
        ],
        1,
    )]);
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 100)
        .expect("constructor tag projection");
    assert_eq!(heap.string(&ran.value), Ok("Colors.Color.Red"));
    assert_eq!(
        heap.stats().live_objects,
        1,
        "the constructor is already released"
    );
    assert_eq!(heap.stats().allocations, 2);
    heap.drop_value(ran.value).expect("owned nominal tag");
    assert!(heap.finish().is_ok());
}

#[test]
fn record_tag_projection_preserves_its_actual_field_value() {
    let p = program(vec![function(
        &[],
        vec![
            bind(0, Operation::Literal(Lit::Int(41))),
            bind(
                1,
                Operation::Record {
                    fields: vec![("_tag".into(), 0)],
                },
            ),
            bind(
                2,
                Operation::Field {
                    object: 1,
                    field: "_tag".into(),
                },
            ),
        ],
        2,
    )]);
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 100)
        .expect("record tag projection");
    assert_eq!(
        ran.value,
        Value::Int(41),
        "a record field is not constructor metadata"
    );
    assert_eq!(
        heap.stats().allocations,
        1,
        "no synthetic tag string is allocated"
    );
    heap.drop_value(ran.value).expect("record field result");
    assert!(heap.finish().is_ok());
}

#[test]
fn bigint_show_errors_and_fuel_cuts_release_input_and_output_roots() {
    use flick_core::ir::memory::Intrinsic;
    let show = |literal| {
        program(vec![function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::BigInt(literal))),
                bind(
                    1,
                    Operation::Intrinsic {
                        kind: Intrinsic::Show,
                        args: vec![0],
                    },
                ),
            ],
            1,
        )])
    };
    for spelling in ["", "0x", "0b2", "not an integer"] {
        let mut heap = Heap::new();
        assert_eq!(
            execute_program(
                &mut heap,
                &show(spelling.into()),
                &Name::new("Main", "main"),
                vec![],
                100
            )
            .expect_err("malformed raw memory BigInt"),
            Error::InvalidBigInt(spelling.into())
        );
        heap.finish()
            .expect("failed Show releases the accepted owned BigInt root");
    }
    let p = show("-0xffffffffffffffffffffffffffffffff".into());
    let mut completed = false;
    for fuel in 0..12 {
        let mut heap = Heap::new();
        match execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], fuel) {
            Ok(ran) => {
                completed = true;
                assert_eq!(
                    heap.string(&ran.value),
                    Ok("-340282366920938463463374607431768211455n")
                );
                heap.drop_value(ran.value).expect("display result release");
            }
            Err(error) => assert_eq!(error, Error::OutOfFuel),
        }
        let stats = heap
            .finish()
            .expect("every fuel cut drains only the real owned input/output roots");
        assert_eq!(stats.allocations, stats.frees);
    }
    assert!(
        completed,
        "the fuel sweep includes a successful display result"
    );
}
