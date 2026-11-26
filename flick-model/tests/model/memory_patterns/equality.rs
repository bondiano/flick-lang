fn comparison(op: BinOp) -> Program {
    program(function(
        &[0, 1],
        vec![
            bind(
                2,
                Operation::Project {
                    object: 1,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            bind(
                3,
                Operation::Project {
                    object: 1,
                    field: 1,
                    rep: ValueRep::Cell,
                },
            ),
            bind(
                4,
                Operation::Binary {
                    op,
                    left: 2,
                    right: 3,
                },
            ),
        ],
        4,
    ))
}
fn compare_values(heap: &mut Heap, op: BinOp, left: Value, right: Value, expected: bool) {
    let input = heap
        .construct("$tuple", vec![left, right])
        .expect("comparison inputs");
    let result = run(heap, &comparison(op), input, 10_000).expect("value comparison");
    assert_eq!(result, Value::Bool(expected));
    heap.drop_value(result).expect("comparison result release");
}
#[test]
fn equality_ignores_owned_dictionary_closures_and_releases_every_capture() {
    let packed = |binding, field, dict| {
        bind(
            binding,
            Operation::ConstructWithDicts {
                tag: "M.Packed.Packed".into(),
                fields: vec![field],
                dicts: vec![dict],
                rep: ValueRep::Cell,
                layout: None,
            },
        )
    };
    for (op, same, expected) in [
        (BinOp::Eq, true, true),
        (BinOp::Eq, false, false),
        (BinOp::NotEq, true, false),
        (BinOp::NotEq, false, true),
    ] {
        let body = function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::Int(42))),
                bind(
                    3,
                    Operation::Closure {
                        code: 2,
                        captures: vec![1],
                    },
                ),
                bind(
                    4,
                    Operation::Record {
                        fields: vec![("allow".into(), 3)],
                    },
                ),
                packed(5, 2, 4),
                bind(6, Operation::Literal(Lit::Int(if same { 42 } else { 43 }))),
                bind(
                    7,
                    Operation::Closure {
                        code: 3,
                        captures: vec![1],
                    },
                ),
                bind(
                    8,
                    Operation::Record {
                        fields: vec![("allow".into(), 7)],
                    },
                ),
                packed(9, 6, 8),
                bind(
                    10,
                    Operation::Binary {
                        op,
                        left: 5,
                        right: 9,
                    },
                ),
            ],
            10,
        );
        let mut p = program(body);
        p.functions.insert(
            2,
            function(
                &[0, 1],
                vec![bind(2, Operation::Literal(Lit::Bool(true)))],
                2,
            ),
        );
        p.functions.insert(
            3,
            function(
                &[0, 1],
                vec![bind(2, Operation::Literal(Lit::Bool(false)))],
                2,
            ),
        );
        let mut heap = Heap::new();
        let input = heap
            .literal(&Lit::String("external captured value".into()))
            .expect("capture");
        let external = heap.dup(&input).expect("external alias");
        let result = run(&mut heap, &p, input, 10_000).expect("evidence must not be compared");
        assert_eq!(result, Value::Bool(expected));
        assert_eq!(
            heap.stats().live_objects,
            1,
            "all evidence records, closures and constructor storage are released"
        );
        assert_eq!(heap.reference_count(&external), Ok(1));
        heap.drop_value(result).expect("Bool result release");
        heap.drop_value(external).expect("external capture release");
        heap.finish()
            .expect("all physical evidence and ordinary fields released");
    }
}
#[test]
fn equality_ignores_evidence_counts_but_never_guesses_from_record_shape() {
    let mut heap = Heap::new();
    let left = heap
        .construct_with_dicts("T.Box", vec![Value::Int(42)], vec![Value::Int(1)])
        .expect("one dictionary");
    let right = heap
        .construct_with_dicts(
            "T.Box",
            vec![Value::Int(42)],
            vec![Value::Bool(false), Value::Int(99)],
        )
        .expect("two dictionaries");
    compare_values(&mut heap, BinOp::Eq, left, right, true);
    let left = heap
        .record(vec![("allow".into(), Value::Int(1))])
        .expect("ordinary record payload");
    let right = heap
        .record(vec![("allow".into(), Value::Int(2))])
        .expect("different ordinary record payload");
    let left = heap
        .construct_with_dicts("T.Box", vec![left], vec![Value::Int(7)])
        .expect("ordinary record with evidence");
    let right = heap
        .construct_with_dicts("T.Box", vec![right], vec![Value::Int(7)])
        .expect("ordinary record with evidence");
    compare_values(&mut heap, BinOp::Eq, left, right, false);
    heap.finish()
        .expect("all fields, not just semantic prefixes, released");
}
#[test]
fn payload_free_primitive_equality_ignores_evidence_but_payloads_are_not_primitive() {
    let mut heap = Heap::new();
    for (op, tag, expected) in [
        (BinOp::Same, "T.Token", true),
        (BinOp::NotSame, "T.Other", true),
    ] {
        let left = heap
            .construct_with_dicts("T.Token", vec![], vec![Value::Int(1)])
            .expect("left token");
        let right = heap
            .construct_with_dicts(tag, vec![], vec![Value::Int(2), Value::Int(3)])
            .expect("right token");
        compare_values(&mut heap, op, left, right, expected);
    }
    let left = heap
        .construct_with_dicts("T.Box", vec![Value::Int(1)], vec![])
        .expect("payload");
    let right = heap
        .construct_with_dicts("T.Box", vec![Value::Int(1)], vec![])
        .expect("payload");
    let pair = heap
        .construct("$tuple", vec![left, right])
        .expect("comparison pair");
    assert_eq!(
        run(&mut heap, &comparison(BinOp::Same), pair, 1000)
            .expect_err("ordinary payload is not identity equality"),
        Error::UnsupportedPrimitive("primitive equality of non-primitive objects".into())
    );
    heap.finish()
        .expect("rejected primitive comparison cleaned both values");
}
#[test]
fn ordinary_closures_still_reject_equality_even_when_other_fields_are_evidence() {
    let mut p = program(function(
        &[0, 1],
        vec![
            bind(
                2,
                Operation::Closure {
                    code: 2,
                    captures: vec![],
                },
            ),
            bind(3, Operation::Literal(Lit::Int(99))),
            bind(
                4,
                Operation::ConstructWithDicts {
                    tag: "T.Box".into(),
                    fields: vec![2],
                    dicts: vec![3],
                    rep: ValueRep::Cell,
                    layout: None,
                },
            ),
            bind(
                5,
                Operation::ConstructWithDicts {
                    tag: "T.Box".into(),
                    fields: vec![2],
                    dicts: vec![3],
                    rep: ValueRep::Cell,
                    layout: None,
                },
            ),
            bind(
                6,
                Operation::Binary {
                    op: BinOp::Eq,
                    left: 4,
                    right: 5,
                },
            ),
        ],
        6,
    ));
    p.functions.insert(2, function(&[0, 1], vec![], 1));
    let mut heap = Heap::new();
    assert_eq!(
        run(&mut heap, &p, Value::Unit, 1000).expect_err("ordinary closures cannot be ignored"),
        Error::UnsupportedPrimitive("closure equality".into())
    );
    heap.finish()
        .expect("ordinary closure and evidence ownership balanced on error");
}
