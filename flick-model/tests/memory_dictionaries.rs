//! Raw counted dictionary lookup and structural views have independent contracts.
#[path = "support/causes.rs"]
pub mod causes;
use flick_core::ir::{
    Lit,
    memory::{
        Block, DictionaryEntry, Exit, Function, Global, Instruction, Intrinsic, Name, Operation,
        Program,
    },
};
use flick_core::opt::perceus;
use flick_model::memory::{
    Error, Heap, ProgramOutcome, Value, execute_program, execute_program_outcome,
};

fn let_(binding: u32, value: Operation) -> Instruction {
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
    .expect("valid ownership source")
}
fn program(body: Function, extras: Vec<(u32, Function)>) -> Program {
    let mut functions = vec![(0, function(&[], vec![closure(0, 1, &[])], 0)), (1, body)];
    functions.extend(extras);
    Program {
        functions: functions.into_iter().collect(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    }
}
fn entry() -> Name {
    Name::new("Dictionary", "main")
}
fn literal(binding: u32, text: &str) -> Instruction {
    let_(binding, Operation::Literal(Lit::String(text.into())))
}

fn method(binding: u32, dictionary: u32, field: &str) -> Instruction {
    let_(
        binding,
        Operation::Method {
            dictionary,
            field: field.into(),
        },
    )
}
fn closure(binding: u32, code: u32, captures: &[u32]) -> Instruction {
    let_(
        binding,
        Operation::Closure {
            code,
            captures: captures.to_vec(),
        },
    )
}
fn tuple(binding: u32, fields: &[u32]) -> Instruction {
    let_(
        binding,
        Operation::Construct {
            tag: "$tuple".into(),
            fields: fields.to_vec(),
        },
    )
}
fn intrinsic(binding: u32, kind: Intrinsic, args: &[u32]) -> Instruction {
    let_(
        binding,
        Operation::Intrinsic {
            kind,
            args: args.to_vec(),
        },
    )
}

fn getters() -> Program {
    program(
        function(
            &[0, 1],
            vec![
                literal(2, "owned"),
                closure(3, 2, &[]),
                closure(4, 3, &[]),
                let_(
                    5,
                    Operation::Dictionary {
                        entries: vec![
                            ("value".into(), DictionaryEntry::Value(2)),
                            ("computed".into(), DictionaryEntry::Getter(3)),
                            ("callback".into(), DictionaryEntry::Getter(4)),
                        ],
                    },
                ),
                method(6, 5, "computed"),
                method(7, 5, "computed"),
                method(8, 5, "callback"),
                let_(
                    9,
                    Operation::Apply {
                        function: 8,
                        argument: 1,
                    },
                ),
                tuple(10, &[6, 7, 9]),
            ],
            10,
        ),
        vec![
            (
                2,
                function(
                    &[0, 1],
                    vec![
                        literal(2, "getter"),
                        intrinsic(3, Intrinsic::PrintLine, &[2]),
                        method(4, 1, "value"),
                    ],
                    4,
                ),
            ),
            (3, function(&[0, 1], vec![closure(2, 4, &[1])], 2)),
            (
                4,
                function(
                    &[0, 1],
                    vec![
                        let_(
                            2,
                            Operation::Project {
                                object: 0,
                                field: 0,
                            },
                        ),
                        method(3, 2, "value"),
                    ],
                    3,
                ),
            ),
        ],
    )
}

#[test]
fn getters_receive_actual_self_recompute_and_may_return_self_owning_methods() {
    let p = getters();
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &p, &entry(), vec![Value::Unit], 10_000)
        .expect("lazy dictionary lookup");
    assert_eq!(
        ran.stdout, "getter\ngetter\n",
        "getter values are not cached"
    );
    for field in 0..3 {
        assert_eq!(
            heap.string(heap.field(&ran.value, field).expect("result field")),
            Ok("owned")
        );
    }
    heap.drop_value(ran.value).expect("release result");
    let stats = heap.finish().expect("no dictionary-method cycle");
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn dictionary_getter_roots_are_released_at_every_fuel_prefix() {
    let p = getters();
    let mut completed = false;
    for fuel in 0..=1024 {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![Value::Unit], fuel);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                completed = true;
                assert_eq!(report.stdout, "getter\ngetter\n");
                heap.drop_value(value).expect("result");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {
                assert!("getter\ngetter\n".starts_with(&report.stdout));
            }
            other => {
                let diagnostic = format!("fuel={fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        assert_eq!(heap.string(&outside), Ok("outside"));
        assert_eq!(heap.stats().live_objects, 1, "fuel={fuel}");
        heap.drop_value(outside).expect("external root");
        let stats = heap
            .finish()
            .unwrap_or_else(|error| panic!("fuel={fuel}: {error}"));
        assert_eq!(stats.allocations, stats.frees);
    }
    assert!(completed);
}

fn structure(expected: usize) -> Program {
    program(
        function(
            &[0, 1],
            vec![let_(
                2,
                Operation::Structure {
                    object: 1,
                    expected,
                },
            )],
            2,
        ),
        vec![],
    )
}

#[test]
fn record_structure_uses_utf16_labels_and_retains_independent_owned_values() {
    let p = structure(2);
    let mut completed = false;
    for fuel in 0..=160 {
        let mut heap = Heap::new();
        let bmp = heap.literal(&Lit::String("bmp".into())).expect("bmp");
        let astral = heap.literal(&Lit::String("astral".into())).expect("astral");
        let record = heap
            .record(vec![("\u{e000}".into(), bmp), ("😀".into(), astral)])
            .expect("record");
        let alias = heap.dup(&record).expect("external alias");
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![record], fuel);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                completed = true;
                assert_eq!(heap.field(&value, 0), Ok(&Value::Bool(true)));
                let values = heap.field(&value, 1).expect("values");
                assert_eq!(
                    heap.string(heap.field(values, 0).expect("first value")),
                    Ok("astral")
                );
                assert_eq!(
                    heap.string(heap.field(values, 1).expect("second value")),
                    Ok("bmp")
                );
                let labels = heap.field(&value, 2).expect("labels");
                assert_eq!(
                    heap.string(heap.field(labels, 0).expect("first label")),
                    Ok("😀")
                );
                assert_eq!(
                    heap.string(heap.field(labels, 1).expect("second label")),
                    Ok("\u{e000}")
                );
                heap.drop_value(value).expect("view");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => (),
            other => {
                let diagnostic = format!("fuel={fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        assert_eq!(
            heap.string(heap.named_field(&alias, "😀").expect("original field")),
            Ok("astral")
        );
        heap.drop_value(alias).expect("external alias");
        let stats = heap
            .finish()
            .unwrap_or_else(|error| panic!("fuel={fuel}: {error}"));
        assert_eq!(stats.allocations, stats.frees);
    }
    assert!(completed);
}

#[test]
fn tuple_structure_has_unit_labels_and_empty_unit_is_a_tuple() {
    for count in [0, 2] {
        let mut heap = Heap::new();
        let input = if count == 0 {
            Value::Unit
        } else {
            heap.construct("$tuple", vec![Value::Int(3), Value::Int(4)])
                .expect("tuple")
        };
        let ran = execute_program(&mut heap, &structure(count), &entry(), vec![input], 1000)
            .expect("tuple view");
        assert_eq!(heap.field(&ran.value, 0), Ok(&Value::Bool(false)));
        let labels = heap.field(&ran.value, 2).expect("labels");
        for index in 0..count {
            assert_eq!(heap.field(labels, index), Ok(&Value::Unit));
        }
        heap.drop_value(ran.value).expect("view");
        heap.finish().expect("tuple storage released");
    }
}

#[test]
fn bad_dictionary_entries_and_lookups_release_all_real_roots() {
    let cases = [
        (
            vec![("bad".into(), DictionaryEntry::Getter(2))],
            Operation::Method {
                dictionary: 3,
                field: "bad".into(),
            },
            Error::ExpectedClosure,
        ),
        (
            vec![("value".into(), DictionaryEntry::Value(2))],
            Operation::Method {
                dictionary: 3,
                field: "absent".into(),
            },
            Error::MissingMethod("absent".into()),
        ),
        (
            vec![
                ("duplicate".into(), DictionaryEntry::Value(2)),
                ("duplicate".into(), DictionaryEntry::Value(2)),
            ],
            Operation::Method {
                dictionary: 3,
                field: "duplicate".into(),
            },
            Error::DuplicateField("duplicate".into()),
        ),
        (
            vec![("value".into(), DictionaryEntry::Value(2))],
            Operation::Field {
                object: 3,
                field: "value".into(),
            },
            Error::ExpectedRecord,
        ),
        (
            vec![("value".into(), DictionaryEntry::Value(2))],
            Operation::Project {
                object: 3,
                field: 0,
            },
            Error::ExpectedObject,
        ),
        (
            vec![("value".into(), DictionaryEntry::Value(2))],
            Operation::Structure {
                object: 3,
                expected: 1,
            },
            Error::ExpectedStructure,
        ),
    ];
    for (entries, operation, expected) in cases {
        let p = program(
            function(
                &[0, 1],
                vec![
                    literal(2, "payload"),
                    let_(3, Operation::Dictionary { entries }),
                    let_(4, operation),
                ],
                4,
            ),
            vec![],
        );
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("external".into()))
            .expect("external");
        let error = execute_program(&mut heap, &p, &entry(), vec![Value::Unit], 5000)
            .expect_err("invalid dictionary operation");
        assert_eq!(error, expected);
        assert_eq!(heap.string(&external), Ok("external"));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external).expect("external");
        let stats = heap.finish().expect("no retained malformed entries");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn structural_views_reject_wrong_arity_and_nonstructural_shapes() {
    for tag in ["$list", "M.T.Tuple", "Nominal"] {
        let mut heap = Heap::new();
        let value = heap.literal(&Lit::String("payload".into())).expect("field");
        let value = heap
            .construct(tag, vec![value])
            .expect("nonstructural object");
        assert_eq!(
            execute_program(&mut heap, &structure(1), &entry(), vec![value], 1000)
                .expect_err("not a tuple"),
            Error::ExpectedStructure
        );
        heap.finish().expect("wrong object released");
    }
    let mut heap = Heap::new();
    let value = heap.literal(&Lit::String("payload".into())).expect("field");
    let value = heap.record(vec![("value".into(), value)]).expect("record");
    assert_eq!(
        execute_program(&mut heap, &structure(2), &entry(), vec![value], 1000)
            .expect_err("wrong field count"),
        Error::StructureArity {
            expected: 2,
            actual: 1
        }
    );
    heap.finish().expect("wrong record released");
    let p = program(
        function(
            &[0, 1],
            vec![
                let_(2, Operation::Record { fields: vec![] }),
                method(3, 2, "value"),
            ],
            3,
        ),
        vec![],
    );
    assert_eq!(
        execute_program(&mut heap, &p, &entry(), vec![Value::Unit], 1000)
            .expect_err("record is not a dictionary"),
        Error::ExpectedDictionary
    );
    heap.finish().expect("wrong dictionary released");
}

#[test]
fn getter_typed_failure_transfers_one_payload_and_releases_the_actual_receiver() {
    let p = program(
        function(
            &[0, 1],
            vec![
                literal(2, "raised"),
                closure(3, 2, &[]),
                let_(
                    4,
                    Operation::Dictionary {
                        entries: vec![
                            ("value".into(), DictionaryEntry::Value(2)),
                            ("fail".into(), DictionaryEntry::Getter(3)),
                        ],
                    },
                ),
                method(5, 4, "fail"),
            ],
            5,
        ),
        vec![(
            2,
            function(
                &[0, 1],
                vec![method(2, 1, "value"), intrinsic(3, Intrinsic::Raise, &[2])],
                3,
            ),
        )],
    );
    let mut raised = false;
    for fuel in 0..=300 {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("external".into()))
            .expect("external");
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![Value::Unit], fuel);
        match report.outcome {
            ProgramOutcome::Raised(value) => {
                raised = true;
                assert_eq!(heap.string(&value), Ok("raised"));
                heap.drop_value(value).expect("typed payload");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => (),
            other => {
                let diagnostic = format!("fuel={fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        assert_eq!(heap.string(&external), Ok("external"));
        assert_eq!(heap.stats().live_objects, 1, "fuel={fuel}");
        heap.drop_value(external).expect("external");
        heap.finish()
            .unwrap_or_else(|error| panic!("fuel={fuel}: {error}"));
    }
    assert!(raised);
}

#[test]
fn getters_use_lookup_context_not_construction_context_and_restore_absence() {
    let p = program(
        function(
            &[0, 1],
            vec![
                closure(2, 2, &[]),
                let_(
                    3,
                    Operation::Dictionary {
                        entries: vec![("context".into(), DictionaryEntry::Getter(2))],
                    },
                ),
                closure(4, 3, &[3]),
                intrinsic(5, Intrinsic::ScopeRun, &[4]),
            ],
            5,
        ),
        vec![
            (
                2,
                function(
                    &[0, 1],
                    vec![
                        let_(2, Operation::ReadOwnerEvidence),
                        let_(3, Operation::Literal(Lit::Unit)),
                        intrinsic(4, Intrinsic::ScopeCurrent, &[3]),
                        tuple(5, &[2, 4]),
                    ],
                    5,
                ),
            ),
            (
                3,
                function(
                    &[0, 1],
                    vec![
                        let_(
                            2,
                            Operation::Project {
                                object: 0,
                                field: 0,
                            },
                        ),
                        method(3, 2, "context"),
                        let_(4, Operation::Literal(Lit::Unit)),
                        let_(
                            5,
                            Operation::WithOwnerEvidence {
                                owner: 4,
                                code: 4,
                                args: vec![2],
                            },
                        ),
                        let_(6, Operation::ReadOwnerEvidence),
                        tuple(7, &[3, 5, 1, 6]),
                    ],
                    7,
                ),
            ),
            (4, function(&[0], vec![method(1, 0, "context")], 1)),
        ],
    );
    let mut completed = false;
    for fuel in 0..=500 {
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &p, &entry(), vec![Value::Unit], fuel);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                completed = true;
                let normal = heap.field(&value, 0).expect("normal lookup");
                let absent = heap.field(&value, 1).expect("absent lookup");
                let owner = heap.field(&value, 2).expect("scope owner");
                assert!(matches!(owner, Value::Owner(_)));
                assert_eq!(heap.field(normal, 0).expect("lookup C"), owner);
                assert_eq!(heap.field(normal, 1).expect("lookup D"), owner);
                assert_eq!(heap.field(absent, 0), Ok(&Value::Unit));
                assert_eq!(
                    heap.field(absent, 1).expect("D unaffected by absence"),
                    owner
                );
                assert_eq!(heap.field(&value, 3).expect("restored C"), owner);
                heap.drop_value(value).expect("returned view");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => (),
            other => {
                let diagnostic = format!("fuel={fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        heap.finish()
            .unwrap_or_else(|error| panic!("fuel={fuel}: {error}"));
    }
    assert!(completed);
}
