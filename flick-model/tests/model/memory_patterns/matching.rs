use flick_core::ir::memory::ValueRep;
#[test]
fn whole_value_binding_retains_before_last_receiver_drop() {
    let p = program(function(
        &[0, 1],
        vec![bind(
            2,
            Operation::Match {
                object: 1,
                pattern: Box::new(Pat::Bind("whole".into())),
                rep: ValueRep::Cell,
            },
        )],
        2,
    ));
    let mut heap = Heap::new();
    let input = heap.literal(&Lit::String("owned".into())).expect("input");
    let id = input.object_id().expect("identity");
    let result = run(&mut heap, &p, input, 100).expect("match");
    assert_eq!(heap.tag(&result), Ok("$match"));
    assert_eq!(heap.field(&result, 0), Ok(&Value::Bool(true)));
    assert_eq!(
        heap.field(&result, 1).expect("bound value").object_id(),
        Some(id)
    );
    heap.drop_value(result).expect("result release");
    heap.finish().expect("no leaks");
}
#[test]
fn no_match_intrinsic_is_a_real_error_with_no_operands() {
    let p = program(function(
        &[0, 1],
        vec![bind(
            2,
            Operation::Intrinsic {
                kind: Intrinsic::MatchFail,
                args: vec![],
            },
        )],
        2,
    ));
    let mut heap = Heap::new();
    let input = heap
        .literal(&Lit::String("discarded".into()))
        .expect("input");
    assert_eq!(
        run(&mut heap, &p, input, 100).expect_err("no matching arm"),
        Error::NoMatch
    );
    heap.finish().expect("error released real roots");
}
#[test]
fn updates_reuse_unique_records_allocate_when_shared_or_disabled_and_last_duplicate_wins() {
    let p = program(function(
        &[0, 1],
        vec![
            bind(2, Operation::Literal(Lit::Int(7))),
            bind(3, Operation::Literal(Lit::Int(9))),
            bind(
                4,
                Operation::Update {
                    object: 1,
                    fields: vec![("a".into(), 2), ("a".into(), 3)],
                },
            ),
        ],
        4,
    ));

    // Unique receiver with reuse enabled updates the record slot in place.
    let mut heap = Heap::new();
    let input = heap
        .record(vec![
            ("a".into(), Value::Int(1)),
            ("b".into(), Value::Int(2)),
        ])
        .expect("record");
    let id = input.object_id();
    let result = run(&mut heap, &p, input, 100).expect("unique record update");
    assert_eq!(result.object_id(), id);
    assert_eq!(heap.stats().reuses, 1);
    assert_eq!(heap.stats().reuse_fallbacks, 0);
    assert_eq!(heap.named_field(&result, "a"), Ok(&Value::Int(9)));
    assert_eq!(heap.named_field(&result, "b"), Ok(&Value::Int(2)));
    heap.drop_value(result).expect("record release");
    heap.finish().expect("no leaks");

    // Shared receiver falls back to a fresh record and preserves the external alias.
    let mut heap = Heap::new();
    let input = heap
        .record(vec![
            ("a".into(), Value::Int(1)),
            ("b".into(), Value::Int(2)),
        ])
        .expect("shared record");
    let alias = heap.dup(&input).expect("external alias");
    let id = input.object_id();
    let result = run(&mut heap, &p, input, 100).expect("shared record update");
    assert_ne!(result.object_id(), id);
    assert_eq!(heap.stats().reuses, 0);
    assert_eq!(heap.stats().reuse_fallbacks, 1);
    assert_eq!(heap.named_field(&result, "a"), Ok(&Value::Int(9)));
    assert_eq!(heap.named_field(&result, "b"), Ok(&Value::Int(2)));
    assert_eq!(heap.named_field(&alias, "a"), Ok(&Value::Int(1)));
    assert_eq!(heap.named_field(&alias, "b"), Ok(&Value::Int(2)));
    heap.drop_value(result).expect("updated release");
    heap.drop_value(alias).expect("alias release");
    heap.finish().expect("no leaks");

    // Baseline execution with reuse disabled allocates a fresh record even when unique.
    let mut heap = Heap::without_reuse();
    let input = heap
        .record(vec![
            ("a".into(), Value::Int(1)),
            ("b".into(), Value::Int(2)),
        ])
        .expect("baseline record");
    let id = input.object_id();
    let result = run(&mut heap, &p, input, 100).expect("baseline record update");
    assert_ne!(result.object_id(), id);
    assert_eq!(heap.stats().reuses, 0);
    assert_eq!(heap.named_field(&result, "a"), Ok(&Value::Int(9)));
    assert_eq!(heap.named_field(&result, "b"), Ok(&Value::Int(2)));
    heap.drop_value(result).expect("baseline release");
    heap.finish().expect("no leaks");
}

fn matcher(pattern: Pat) -> Program {
    program(function(
        &[0, 1],
        vec![bind(
            2,
            Operation::Match {
                object: 1,
                pattern: Box::new(pattern),
                rep: ValueRep::Cell,
            },
        )],
        2,
    ))
}
fn flag(heap: &Heap, result: &Value) -> bool {
    match heap.field(result, 0).expect("match flag") {
        Value::Bool(value) => *value,
        _ => panic!("match flag must be Bool"),
    }
}
fn constructor(arity: usize, dicts: usize) -> Constructor {
    Constructor {
        module: "M".into(),
        ty: "Box".into(),
        name: "Box".into(),
        arity,
        dicts,
        rep: Rep::Tagged,
        fields: Vec::new(),
        type_args: Vec::new(),
    }
}
fn bound(name: &str) -> Pat {
    Pat::Bind(name.into())
}

#[test]
fn or_rollback_discards_failed_bindings_and_uses_selected_positions() {
    let pattern = Pat::Or(vec![
        Pat::Tuple(vec![bound("x"), Pat::Lit(Lit::String("missing".into()))]),
        Pat::Tuple(vec![Pat::Lit(Lit::String("left".into())), bound("x")]),
    ]);
    let mut heap = Heap::new();
    let left = heap.literal(&Lit::String("left".into())).expect("left");
    let left_id = left.object_id().expect("left identity");
    let right = heap.literal(&Lit::String("right".into())).expect("right");
    let right_id = right.object_id();
    let input = heap.construct("$tuple", vec![left, right]).expect("pair");
    let result = run(&mut heap, &matcher(pattern), input, 1000).expect("second Or alternative");
    assert!(flag(&heap, &result));
    assert_eq!(
        heap.field(&result, 1).expect("selected binder").object_id(),
        right_id
    );
    assert!(
        !heap.is_live(left_id),
        "failed alternative never retains its tentative binding"
    );
    heap.drop_value(result).expect("result release");
    heap.finish().expect("no leaked trial bindings");
}
#[test]
fn nested_or_rollback_keeps_outer_bindings_and_lexical_order() {
    let pattern = Pat::Tuple(vec![
        bound("z"),
        Pat::Or(vec![
            Pat::Tuple(vec![bound("a"), Pat::Lit(Lit::Bool(false))]),
            Pat::Tuple(vec![Pat::Lit(Lit::String("inside".into())), bound("a")]),
        ]),
    ]);
    let mut heap = Heap::new();
    let outside = heap
        .literal(&Lit::String("outside".into()))
        .expect("outside");
    let inside = heap.literal(&Lit::String("inside".into())).expect("inside");
    let pair = heap
        .construct("$tuple", vec![inside, Value::Bool(true)])
        .expect("nested pair");
    let input = heap
        .construct("$tuple", vec![outside, pair])
        .expect("outer pair");
    let result = run(&mut heap, &matcher(pattern), input, 1000).expect("nested alternatives");
    assert_eq!(
        heap.field(&result, 1),
        Ok(&Value::Bool(true)),
        "a comes before z"
    );
    assert_eq!(
        heap.string(heap.field(&result, 2).expect("z")),
        Ok("outside")
    );
    heap.drop_value(result).expect("result release");
    heap.finish()
        .expect("rollback preserved exactly the selected bindings");
}
#[test]
fn virtual_nested_list_tails_materialize_only_selected_owned_values() {
    let pattern = Pat::List(
        vec![bound("z")],
        Some(Box::new(Pat::List(
            vec![bound("b")],
            Some(Box::new(bound("a"))),
        ))),
    );
    let mut heap = Heap::new();
    let values = ["first", "second", "third"]
        .into_iter()
        .map(|text| {
            heap.literal(&Lit::String(text.into()))
                .expect("list string")
        })
        .collect();
    let input = heap.construct("$list", values).expect("list");
    let input_id = input.object_id();
    let result = run(&mut heap, &matcher(pattern), input, 1000).expect("nested tail");
    let tail = heap.field(&result, 1).expect("a tail");
    assert_eq!(heap.tag(tail), Ok("$list"));
    assert_ne!(
        tail.object_id(),
        input_id,
        "bound tail has fresh counted storage"
    );
    assert_eq!(
        heap.string(heap.field(tail, 0).expect("tail head")),
        Ok("third")
    );
    assert_eq!(
        heap.string(heap.field(&result, 2).expect("b")),
        Ok("second")
    );
    assert_eq!(heap.string(heap.field(&result, 3).expect("z")), Ok("first"));
    heap.drop_value(result).expect("result release");
    heap.finish().expect("all selected elements released");
}
#[test]
fn tuple_unit_list_lengths_and_nested_empty_tails_follow_value_semantics() {
    for (pattern, values, expected) in [
        (Pat::List(vec![], None), vec![], true),
        (Pat::List(vec![], None), vec![Value::Int(1)], false),
        (
            Pat::List(vec![Pat::Wild, Pat::Wild], None),
            vec![Value::Int(1)],
            false,
        ),
        (
            Pat::List(vec![Pat::Wild], Some(Box::new(Pat::List(vec![], None)))),
            vec![Value::Int(1)],
            true,
        ),
    ] {
        let mut heap = Heap::new();
        let input = heap.construct("$list", values).expect("list");
        let result = run(&mut heap, &matcher(pattern), input, 1000).expect("list pattern");
        assert_eq!(flag(&heap, &result), expected);
        heap.drop_value(result).expect("result release");
        heap.finish().expect("list released");
    }
    let mut heap = Heap::new();
    let result = run(&mut heap, &matcher(Pat::Tuple(vec![])), Value::Unit, 100)
        .expect("empty tuple is Unit");
    assert!(flag(&heap, &result));
    heap.drop_value(result).expect("result release");
    heap.finish().expect("Unit match released");
}
#[test]
fn record_patterns_read_actual_labels_including_tag() {
    let mut heap = Heap::new();
    let tag = heap
        .literal(&Lit::String("actual".into()))
        .expect("tag field");
    let input = heap
        .record(vec![("_tag".into(), tag), ("value".into(), Value::Int(42))])
        .expect("record");
    let pattern = Pat::Record(vec![
        ("_tag".into(), bound("z")),
        ("value".into(), bound("a")),
    ]);
    let result = run(&mut heap, &matcher(pattern), input, 1000).expect("record pattern");
    assert_eq!(heap.field(&result, 1), Ok(&Value::Int(42)));
    assert_eq!(
        heap.string(heap.field(&result, 2).expect("actual _tag")),
        Ok("actual")
    );
    heap.drop_value(result).expect("result release");
    heap.finish().expect("record released");
}
#[test]
fn constructor_matching_uses_full_nominal_tags_and_dictionary_suffixes() {
    let pattern = Pat::Con(
        constructor(1, 1),
        vec![bound("z"), Pat::Record(vec![("name".into(), bound("a"))])],
    );
    let mut heap = Heap::new();
    let payload = heap
        .literal(&Lit::String("payload".into()))
        .expect("payload");
    let name = heap
        .literal(&Lit::String("dictionary".into()))
        .expect("dictionary name");
    let dictionary = heap
        .record(vec![("name".into(), name)])
        .expect("dictionary record");
    let input = heap
        .construct_with_dicts("M.Box.Box", vec![payload], vec![dictionary])
        .expect("constructor with a distinct owned evidence suffix");
    let result = run(&mut heap, &matcher(pattern), input, 1000).expect("dictionary suffix pattern");
    assert_eq!(
        heap.string(heap.field(&result, 1).expect("a")),
        Ok("dictionary")
    );
    assert_eq!(
        heap.string(heap.field(&result, 2).expect("z")),
        Ok("payload")
    );
    heap.drop_value(result).expect("result release");
    heap.finish().expect("constructor contents released");
    let input = heap
        .construct("Other.Box.Box", vec![])
        .expect("different nominal constructor");
    let result = run(
        &mut heap,
        &matcher(Pat::Con(constructor(1, 0), vec![Pat::Wild])),
        input,
        1000,
    )
    .expect("tag differs before layout is inspected");
    assert!(!flag(&heap, &result));
    heap.drop_value(result).expect("false result release");
    heap.finish().expect("different tag released");
}
#[test]
fn literal_float_matching_is_numeric_and_bigints_are_unbounded_numbers() {
    for (pattern, value, expected) in [
        (0.0, -0.0, true),
        (-0.0, 0.0, true),
        (f64::NAN, f64::NAN, false),
        (1.5, 1.5, true),
        (1.0, 2.0, false),
    ] {
        let mut heap = Heap::new();
        let result = run(
            &mut heap,
            &matcher(Pat::Lit(Lit::Float(pattern))),
            Value::Float(value),
            100,
        )
        .expect("Float literal");
        assert_eq!(flag(&heap, &result), expected);
        heap.drop_value(result).expect("result release");
        heap.finish().expect("Float match released");
    }
    for (pattern, value, expected) in [
        ("0x10000000000000000", "18446744073709551616", true),
        ("-0b0", "000", true),
        ("-0o1777777777777777777777", "-18446744073709551615", true),
        ("0x10000000000000001", "18446744073709551616", false),
    ] {
        let mut heap = Heap::new();
        let input = heap
            .literal(&Lit::BigInt(value.into()))
            .expect("BigInt input");
        let result = run(
            &mut heap,
            &matcher(Pat::Lit(Lit::BigInt(pattern.into()))),
            input,
            100_000,
        )
        .expect("numerical BigInt literal");
        assert_eq!(
            flag(&heap, &result),
            expected,
            "{pattern} compared to {value}"
        );
        heap.drop_value(result).expect("result release");
        heap.finish().expect("BigInt input and result released");
    }
}
#[test]
fn array_observer_normalization_does_not_merge_tuple_and_list_pattern_kinds() {
    for (pattern, tag, expected) in [
        (
            Pat::List(vec![Pat::Wild, Pat::Wild], None),
            "$tuple",
            "list",
        ),
        (Pat::Tuple(vec![Pat::Wild, Pat::Wild]), "$list", "tuple"),
    ] {
        let mut heap = Heap::new();
        let input = heap
            .construct(tag, vec![Value::Int(1), Value::Int(2)])
            .expect("two-element array");
        assert_eq!(
            run(&mut heap, &matcher(pattern), input, 1000),
            Err(Error::PatternKind(expected))
        );
        let stats = heap.finish().expect("wrong-kind input still released");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn bad_value_kinds_missing_record_fields_and_matching_tag_layouts_are_errors() {
    let mut heap = Heap::new();
    assert_eq!(
        run(
            &mut heap,
            &matcher(Pat::Lit(Lit::Int(1))),
            Value::Bool(true),
            100
        )
        .expect_err("wrong literal kind"),
        Error::PatternKind("matching literal type")
    );
    let input = heap.record(vec![]).expect("record");
    assert_eq!(
        run(
            &mut heap,
            &matcher(Pat::Record(vec![("missing".into(), Pat::Wild)])),
            input,
            1000
        )
        .expect_err("missing field"),
        Error::MissingNamedField("missing".into())
    );
    let input = heap
        .construct("M.Box.Box", vec![])
        .expect("malformed constructor payload");
    assert_eq!(
        run(
            &mut heap,
            &matcher(Pat::Con(constructor(1, 0), vec![Pat::Wild])),
            input,
            1000
        )
        .expect_err("matching tag with invalid layout"),
        Error::PatternLayout {
            expected: 1,
            actual: 0
        }
    );
    let input = heap.construct("M.Box.Box", vec![]).expect("not a record");
    assert_eq!(
        run(&mut heap, &matcher(Pat::Record(vec![])), input, 100)
            .expect_err("constructor is not a record"),
        Error::ExpectedRecord
    );
    heap.finish().expect("each error cleaned its arguments");
}
#[test]
fn wildcard_and_binding_copy_closed_capability_metadata_without_using_storage() {
    let bodies = [
        vec![bind(
            2,
            Operation::Intrinsic {
                kind: Intrinsic::RefMake,
                args: vec![1],
            },
        )],
        vec![bind(
            2,
            Operation::Intrinsic {
                kind: Intrinsic::ScopeCurrent,
                args: vec![1],
            },
        )],
        vec![bind(
            2,
            Operation::Intrinsic {
                kind: Intrinsic::ArenaMake,
                args: vec![1],
            },
        )],
        vec![
            bind(
                2,
                Operation::Intrinsic {
                    kind: Intrinsic::ArenaMake,
                    args: vec![1],
                },
            ),
            bind(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::ArenaAlloc,
                    args: vec![2, 1],
                },
            ),
        ],
    ];
    for (index, instructions) in bodies.into_iter().enumerate() {
        for pattern in [Pat::Wild, bound("held")] {
            let mut heap = Heap::new();
            let last = if index == 3 { 3 } else { 2 };
            let handle = run(
                &mut heap,
                &program(function(&[0, 1], instructions.clone(), last)),
                Value::Unit,
                1000,
            )
            .expect("nonowning capability");
            assert_eq!(
                heap.stats().live_objects,
                0,
                "capability storage already closed"
            );
            let result = run(&mut heap, &matcher(pattern), handle, 1000)
                .expect("no dereference of closed/foreign capability");
            assert!(flag(&heap, &result));
            heap.drop_value(result).expect("metadata safely released");
            heap.finish().expect("no capability resurrected storage");
        }
    }
}
