#[test]
#[cfg(feature = "json")]
fn scoped_ownership_operations_round_trip() {
    let mut operations = vec![Operation::Within {
        owner: 0,
        code: 4,
        args: vec![1, 2],
    }];
    operations.extend(
        [
            Intrinsic::ScopeRun,
            Intrinsic::ScopeCurrent,
            Intrinsic::ArenaMake,
            Intrinsic::ArenaAlloc,
            Intrinsic::ArenaGet,
        ]
        .into_iter()
        .map(|kind| Operation::Intrinsic {
            kind,
            args: (0..kind.arity())
                .map(|index| u32::try_from(index).expect("small arity"))
                .collect(),
        }),
    );
    let json = serde_json::to_string(&operations).expect("serialize scoped operations");
    let back: Vec<Operation> = serde_json::from_str(&json).expect("read scoped operations");
    assert_eq!(back, operations);
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("serialize scoped operations");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("read scoped operations");
        assert_eq!(back, operations);
    }
}

#[test]
#[cfg(feature = "json")]
fn code_and_qualified_globals_round_trip_without_flattening_identities() {
    let program = forwarding_program();
    let json = serde_json::to_string(&program).expect("serializable memory program");
    let back: Program = serde_json::from_str(&json).expect("readable memory program");
    assert_eq!(back, program);
    assert_eq!(validate_program(&back), Ok(()));
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&program, &mut bytes).expect("serializable program");
        let back: Program = ciborium::from_reader(bytes.as_slice()).expect("readable program");
        assert_eq!(back, program);
    }
}

#[test]
fn malformed_patterns_in_unselected_code_are_validated() {
    let mut program = forwarding_program();
    program.functions.insert(
        3,
        Function {
            params: vec![0],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Match {
                            object: 0,
                            pattern: Pat::Or(vec![]),
                        },
                    },
                    Instruction::Drop(0),
                ],
                exit: Exit::Return(1),
            },
        },
    );
    assert_eq!(
        validate_program(&program),
        Err(Error::InCode {
            code: 3,
            error: Box::new(Error::EmptyOrPattern)
        })
    );
}

#[test]
#[cfg(feature = "json")]
fn pattern_and_update_operations_round_trip() {
    let operations = vec![
        Operation::Match {
            object: 0,
            pattern: Pat::Or(vec![
                Pat::Tuple(vec![Pat::Bind("x".into()), Pat::Lit(Lit::Float(-0.0))]),
                Pat::Tuple(vec![Pat::Lit(Lit::Float(f64::NAN)), Pat::Bind("x".into())]),
            ]),
        },
        Operation::Update {
            object: 1,
            fields: vec![("x".into(), 2), ("x".into(), 3)],
        },
        Operation::Intrinsic {
            kind: Intrinsic::MatchFail,
            args: vec![],
        },
    ];
    let json = serde_json::to_string(&operations).expect("serializable matching operations");
    let back: Vec<Operation> = serde_json::from_str(&json).expect("read matching operations");
    assert_eq!(back, operations);
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("serialize matching operations");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("read matching operations");
        assert_eq!(back, operations);
    }
}

#[test]
#[cfg(feature = "json")]
fn annotations_survive_their_serialized_forms() {
    let returned = Block {
        instructions: vec![],
        exit: Exit::Return(3),
    };
    let f = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                Instruction::Dup(1),
                Instruction::Let {
                    binding: 2,
                    value: Operation::Project {
                        object: 1,
                        field: 0,
                    },
                },
                Instruction::Drop(1),
                Instruction::Let {
                    binding: 3,
                    value: Operation::Reuse {
                        object: 1,
                        tag: "Next".into(),
                        fields: vec![2],
                    },
                },
            ],
            exit: Exit::Branch {
                condition: 0,
                then_: Box::new(returned.clone()),
                else_: Box::new(returned),
            },
        },
    };
    assert_eq!(validate(&f), Ok(()));
    let json = serde_json::to_string(&f).expect("serializable");
    let back: Function = serde_json::from_str(&json).expect("readable");
    assert_eq!(back, f);
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&f, &mut bytes).expect("serializable");
        let back: Function = ciborium::from_reader(bytes.as_slice()).expect("readable");
        assert_eq!(back, f);
    }
}
#[test]
fn reserved_tags_reject_evidence_even_in_unselected_code() {
    for tag in ["$tuple", "$list", "$match"] {
        let value = Operation::ConstructWithDicts {
            tag: tag.into(),
            fields: vec![0],
            dicts: vec![1],
        };
        assert_eq!(value.consumed(), [0, 1]);
        let function = Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![Instruction::Let { binding: 2, value }],
                exit: Exit::Return(2),
            },
        };
        assert_eq!(
            validate(&function),
            Err(Error::ReservedConstructorEvidence(tag.into()))
        );
        let mut program = forwarding_program();
        program.functions.insert(3, function);
        assert_eq!(
            validate_program(&program),
            Err(Error::InCode {
                code: 3,
                error: Box::new(Error::ReservedConstructorEvidence(tag.into()))
            })
        );
        let value = Operation::ReuseWithDicts {
            object: 0,
            tag: tag.into(),
            fields: vec![0, 1],
            dicts: vec![1, 0],
        };
        assert_eq!(value.consumed(), [0, 0, 1, 1, 0]);
        let value = Operation::ReuseWithDicts {
            object: 0,
            tag: tag.into(),
            fields: vec![1],
            dicts: vec![2],
        };
        let function = Function {
            params: vec![0, 1, 2],
            body: Block {
                instructions: vec![Instruction::Let { binding: 3, value }],
                exit: Exit::Return(3),
            },
        };
        assert_eq!(
            validate(&function),
            Err(Error::ReservedConstructorEvidence(tag.into()))
        );
    }
}

#[test]
#[cfg(feature = "json")]
fn evidence_operations_round_trip_without_changing_old_wire_shapes() {
    let old = Operation::Construct {
        tag: "T".into(),
        fields: vec![0],
    };
    assert_eq!(
        serde_json::to_string(&old).expect("old construct"),
        r#"{"Construct":{"tag":"T","fields":[0]}}"#
    );
    let old = Operation::Reuse {
        object: 0,
        tag: "T".into(),
        fields: vec![1],
    };
    assert_eq!(
        serde_json::to_string(&old).expect("old reuse"),
        r#"{"Reuse":{"object":0,"tag":"T","fields":[1]}}"#
    );
    let operations = vec![
        Operation::ConstructWithDicts {
            tag: "T".into(),
            fields: vec![0, 1],
            dicts: vec![2],
        },
        Operation::ReuseWithDicts {
            object: 3,
            tag: "T".into(),
            fields: vec![4],
            dicts: vec![5, 6],
        },
    ];
    let json = serde_json::to_string(&operations).expect("new operations");
    let back: Vec<Operation> = serde_json::from_str(&json).expect("new operations deserialize");
    assert_eq!(back, operations);
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("new CBOR operations");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("new CBOR operations deserialize");
        assert_eq!(back, operations);
    }
}

#[test]
#[cfg(feature = "json")]
fn stage2_operations_and_intrinsics_round_trip() {
    let mut operations = vec![
        Operation::VarMake(0),
        Operation::VarGet(1),
        Operation::VarSet { var: 1, value: 2 },
        Operation::Loop {
            id: "$loop".into(),
            code: 3,
            args: vec![0, 1],
        },
        Operation::LoopExit {
            id: "$loop".into(),
            value: Some(4),
        },
        Operation::LoopExit {
            id: "$loop".into(),
            value: None,
        },
        Operation::FunctionBoundary {
            code: 5,
            args: vec![6],
        },
        Operation::UnwrapOption(7),
        Operation::GenerateStrict {
            code: 8,
            args: vec![9],
        },
        Operation::GenerateLazy {
            code: 10,
            captures: vec![11],
            limit: Some(12),
            sync: true,
        },
        Operation::Yield(13),
        Operation::SeqOpen(14),
        Operation::SeqNext(15),
        Operation::Label("field".into()),
    ];
    for kind in [
        Intrinsic::ListRange,
        Intrinsic::LazyMake,
        Intrinsic::LazyForce,
        Intrinsic::ScopeAddFinalizer,
        Intrinsic::ScopeAddFinalizerExit,
        Intrinsic::ScopeAddClose,
        Intrinsic::AcquireRelease,
        Intrinsic::Bracket,
        Intrinsic::Uninterruptible,
        Intrinsic::RecordGet,
        Intrinsic::RecordExtend,
        Intrinsic::RecordRemove,
        Intrinsic::RecordProject,
        Intrinsic::RecordMergeDisjoint,
        Intrinsic::RecordLabels,
        Intrinsic::SeqOfList,
        Intrinsic::SeqToList,
        Intrinsic::ExitIsSuccess,
        Intrinsic::ExitIsFailure,
        Intrinsic::ExitHasInterrupts,
    ] {
        operations.push(Operation::Intrinsic {
            kind,
            args: (0..kind.arity())
                .map(|i| u32::try_from(i).expect("small arity"))
                .collect(),
        });
    }
    let json = serde_json::to_string(&operations).expect("serialize stage2 ops");
    let back: Vec<Operation> = serde_json::from_str(&json).expect("deserialize stage2 ops");
    assert_eq!(back, operations);
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("serialize stage2 CBOR");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("deserialize stage2 CBOR");
        assert_eq!(back, operations);
    }
}
