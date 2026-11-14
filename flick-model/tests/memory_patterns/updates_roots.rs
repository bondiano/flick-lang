#[test]
fn every_pattern_fuel_cut_releases_partial_commit_roots_and_preserves_aliases() {
    let p = matcher(Pat::List(
        vec![bound("a"), bound("b")],
        Some(Box::new(bound("z"))),
    ));
    let mut exhausted_after_retaining = false;
    for fuel in 0..160 {
        let mut heap = Heap::new();
        let fields = ["first", "second", "tail"]
            .into_iter()
            .map(|text| heap.literal(&Lit::String(text.into())).expect("element"))
            .collect();
        let input = heap.construct("$list", fields).expect("list");
        let external = heap.dup(&input).expect("external root");
        match run(&mut heap, &p, input, fuel) {
            Ok(result) => heap.drop_value(result).expect("result release"),
            Err(error) => {
                assert_eq!(error, Error::OutOfFuel);
                exhausted_after_retaining |= heap.stats().dups > 2;
            }
        }
        assert_eq!(
            heap.stats().live_objects,
            4,
            "only original external list and elements survive at fuel {fuel}"
        );
        assert_eq!(
            heap.string(heap.field(&external, 2).expect("tail element")),
            Ok("tail")
        );
        heap.drop_value(external).expect("external release");
        heap.finish()
            .expect("partial selections/tails released explicitly");
    }
    assert!(
        exhausted_after_retaining,
        "test must exercise commit exhaustion, not just selection"
    );
}
#[test]
fn retries_and_bigint_normalization_spend_the_machine_fuel() {
    let trial = Pat::Tuple(vec![bound("x"), Pat::Lit(Lit::Bool(false))]);
    let alternatives = matcher(Pat::Or(vec![trial; 80]));
    let mut heap = Heap::new();
    let held = heap
        .literal(&Lit::String("tentative".into()))
        .expect("tentative binding");
    let input = heap
        .construct("$tuple", vec![held, Value::Bool(true)])
        .expect("scrutinee");
    assert_eq!(
        run(&mut heap, &alternatives, input, 100).expect_err("Or retry budget"),
        Error::OutOfFuel
    );
    heap.finish().expect("failed alternatives owned no tokens");
    let digits = "7".repeat(200);
    let input = heap.literal(&Lit::BigInt(digits.clone())).expect("BigInt");
    let p = matcher(Pat::Lit(Lit::BigInt(digits)));
    assert_eq!(
        run(&mut heap, &p, input, 1000).expect_err("quadratic normalization is metered"),
        Error::OutOfFuel
    );
    heap.finish()
        .expect("normalization failure released its counted input");
}
#[test]
fn update_external_aliases_keep_original_records_unchanged() {
    let p = program(function(
        &[0, 1],
        vec![
            bind(
                2,
                Operation::Field {
                    object: 1,
                    field: "a".into(),
                },
            ),
            bind(
                3,
                Operation::Update {
                    object: 1,
                    fields: vec![("b".into(), 2), ("b".into(), 2)],
                },
            ),
        ],
        3,
    ));
    let mut heap = Heap::new();
    let a = heap.literal(&Lit::String("a".into())).expect("a");
    let b = heap.literal(&Lit::String("old b".into())).expect("b");
    let input = heap
        .record(vec![("a".into(), a), ("b".into(), b)])
        .expect("record");
    let external = heap.dup(&input).expect("external base root");
    let result = run(&mut heap, &p, input, 1000).expect("aliased update");
    assert_ne!(result.object_id(), external.object_id());
    assert_eq!(
        heap.string(heap.named_field(&external, "b").expect("original b")),
        Ok("old b")
    );
    assert_eq!(
        heap.named_field(&result, "a").expect("new a").object_id(),
        heap.named_field(&result, "b").expect("new b").object_id()
    );
    heap.drop_value(result).expect("new record release");
    heap.drop_value(external).expect("external record release");
    heap.finish()
        .expect("duplicate replacement aliases balanced");
}

#[test]
fn updates_do_not_mutate_original_records_retained_by_cells_or_arenas() {
    let mut heap = Heap::new();
    for cell in [false, true] {
        let mut instructions = if cell {
            vec![
                bind(
                    2,
                    Operation::Intrinsic {
                        kind: Intrinsic::RefMake,
                        args: vec![1],
                    },
                ),
                bind(
                    3,
                    Operation::Intrinsic {
                        kind: Intrinsic::RefGet,
                        args: vec![2],
                    },
                ),
            ]
        } else {
            vec![
                bind(4, Operation::Literal(Lit::Unit)),
                bind(
                    5,
                    Operation::Intrinsic {
                        kind: Intrinsic::ArenaMake,
                        args: vec![4],
                    },
                ),
                bind(
                    2,
                    Operation::Intrinsic {
                        kind: Intrinsic::ArenaAlloc,
                        args: vec![5, 1],
                    },
                ),
                bind(
                    3,
                    Operation::Intrinsic {
                        kind: Intrinsic::ArenaGet,
                        args: vec![2],
                    },
                ),
            ]
        };
        instructions.extend([
            bind(6, Operation::Literal(Lit::Int(9))),
            bind(
                7,
                Operation::Update {
                    object: 3,
                    fields: vec![("a".into(), 6)],
                },
            ),
            bind(
                8,
                Operation::Intrinsic {
                    kind: if cell {
                        Intrinsic::RefGet
                    } else {
                        Intrinsic::ArenaGet
                    },
                    args: vec![2],
                },
            ),
            bind(
                9,
                Operation::Construct {
                    tag: "$tuple".into(),
                    fields: vec![7, 8],
                },
            ),
        ]);
        let p = program(function(&[0, 1], instructions, 9));
        let input = heap
            .record(vec![("a".into(), Value::Int(1))])
            .expect("stored record");
        let result = run(&mut heap, &p, input, 1000).expect("stored alias update");
        assert_eq!(
            heap.named_field(heap.field(&result, 0).expect("new record"), "a"),
            Ok(&Value::Int(9))
        );
        assert_eq!(
            heap.named_field(heap.field(&result, 1).expect("old record"), "a"),
            Ok(&Value::Int(1))
        );
        heap.drop_value(result)
            .expect("stored alias results release");
        heap.finish()
            .expect("cell/arena and immutable records all released");
    }
}
#[test]
fn update_error_and_all_fuel_cuts_release_every_written_operand() {
    let p = program(function(
        &[0, 1],
        vec![
            bind(2, Operation::Literal(Lit::String("replaced".into()))),
            bind(
                3,
                Operation::Update {
                    object: 1,
                    fields: vec![("a".into(), 2), ("a".into(), 2)],
                },
            ),
        ],
        3,
    ));
    let mut partial_commit = false;
    for fuel in 0..140 {
        let mut heap = Heap::new();
        let a = heap.literal(&Lit::String("original a".into())).expect("a");
        let b = heap.literal(&Lit::String("original b".into())).expect("b");
        let input = heap
            .record(vec![("a".into(), a), ("b".into(), b)])
            .expect("record");
        let external = heap.dup(&input).expect("external record");
        match run(&mut heap, &p, input, fuel) {
            Ok(result) => heap.drop_value(result).expect("new result release"),
            Err(error) => {
                assert_eq!(error, Error::OutOfFuel);
                partial_commit |= heap.stats().dups > 3;
            }
        }
        assert_eq!(
            heap.stats().live_objects,
            3,
            "only external original remains at fuel {fuel}"
        );
        assert_eq!(
            heap.string(heap.named_field(&external, "a").expect("old a")),
            Ok("original a")
        );
        heap.drop_value(external).expect("external release");
        heap.finish()
            .expect("all staged duplicates/partial output retains released");
    }
    assert!(partial_commit);
    let p = program(function(
        &[0, 1],
        vec![
            bind(2, Operation::Literal(Lit::String("discarded".into()))),
            bind(
                3,
                Operation::Update {
                    object: 1,
                    fields: vec![("missing".into(), 2), ("missing".into(), 2)],
                },
            ),
        ],
        3,
    ));
    let mut heap = Heap::new();
    let input = heap
        .record(vec![("a".into(), Value::Int(1))])
        .expect("record");
    assert_eq!(
        run(&mut heap, &p, input, 1000).expect_err("unknown replacement label"),
        Error::MissingNamedField("missing".into())
    );
    heap.finish()
        .expect("every duplicate written value released on failure");
}
#[test]
fn borrowed_match_reuse_respects_whole_aliases_children_and_dead_results() {
    for bind_whole in [false, true] {
        let pattern = if bind_whole {
            bound("whole")
        } else {
            Pat::Con(constructor(1, 0), vec![bound("child")])
        };
        let p = program(function(
            &[0, 1],
            vec![
                bind(2, Operation::Match { object: 1, pattern }),
                bind(
                    3,
                    Operation::Construct {
                        tag: "New".into(),
                        fields: vec![2],
                    },
                ),
            ],
            3,
        ));
        let mut heap = Heap::new();
        let child = heap.literal(&Lit::String("child".into())).expect("child");
        let input = heap.construct("M.Box.Box", vec![child]).expect("candidate");
        let old = input.object_id();
        let result = run(&mut heap, &p, input, 1000).expect("borrowed match then construction");
        let matched = heap.field(&result, 0).expect("match result");
        if bind_whole {
            assert_ne!(result.object_id(), old);
            assert_eq!(
                heap.stats().reuses,
                0,
                "whole-scrutinee binding aliases the candidate"
            );
            assert_eq!(
                heap.tag(heap.field(matched, 1).expect("old whole")),
                Ok("M.Box.Box")
            );
        } else {
            assert_eq!(result.object_id(), old);
            assert_eq!(heap.stats().reuses, 1);
            assert_eq!(
                heap.string(heap.field(matched, 1).expect("selected child")),
                Ok("child"),
                "retain must precede old candidate field release"
            );
        }
        heap.drop_value(result).expect("result release");
        heap.finish().expect("reuse and fallback release aliases");
    }
    let p = program(function(
        &[0, 1],
        vec![
            bind(
                2,
                Operation::Match {
                    object: 1,
                    pattern: bound("unused"),
                },
            ),
            bind(
                3,
                Operation::Construct {
                    tag: "New".into(),
                    fields: vec![],
                },
            ),
        ],
        3,
    ));
    let mut heap = Heap::new();
    let input = heap.construct("Old", vec![]).expect("zero-field candidate");
    let old = input.object_id();
    let result = run(&mut heap, &p, input, 1000).expect("dead match result before reuse");
    assert_eq!(result.object_id(), old);
    assert_eq!(
        heap.stats().reuses,
        1,
        "dead match result drops its whole alias before reuse"
    );
    heap.drop_value(result).expect("result release");
    heap.finish().expect("no temporary alias survives");
}
#[test]
fn match_update_and_failure_are_program_only_in_standalone_execution() {
    use flick_model::memory::execute;
    for operation in [
        Operation::Match {
            object: 0,
            pattern: Pat::Wild,
        },
        Operation::Update {
            object: 0,
            fields: vec![],
        },
        Operation::Intrinsic {
            kind: Intrinsic::MatchFail,
            args: vec![],
        },
    ] {
        let body = function(&[0], vec![bind(1, operation)], 1);
        let mut heap = Heap::new();
        assert_eq!(
            execute(&mut heap, &body, vec![Value::Unit])
                .expect_err("metered operations require program VM"),
            Error::NeedsProgram
        );
        heap.finish().expect("no unmetered object allocation");
    }
}
