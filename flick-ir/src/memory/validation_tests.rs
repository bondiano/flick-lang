use std::collections::BTreeMap;
fn function(instructions: Vec<Instruction>) -> Function {
    Function {
        params: vec![0],
        body: Block {
            instructions,
            exit: Exit::Return(0),
        },
    }
}

#[test]
fn detects_both_leaks_and_double_drops() {
    assert_eq!(validate(&function(vec![])), Ok(()));
    assert_eq!(
        validate(&function(vec![Instruction::Dup(0)])),
        Err(Error::Leftover(vec![(0, 1)]))
    );
    assert_eq!(
        validate(&function(vec![Instruction::Drop(0)])),
        Err(Error::NoToken(0))
    );
}

#[test]
fn validates_both_branch_paths() {
    let mut f = function(vec![]);
    f.params.push(1);
    let returned = Block {
        instructions: vec![],
        exit: Exit::Return(0),
    };
    f.body.exit = Exit::Branch {
        condition: 1,
        then_: Box::new(returned.clone()),
        else_: Box::new(returned),
    };
    assert_eq!(validate(&f), Ok(()));
    if let Exit::Branch { else_, .. } = &mut f.body.exit {
        else_.instructions.push(Instruction::Dup(0));
    }
    assert_eq!(validate(&f), Err(Error::Leftover(vec![(0, 1)])));
}

#[test]
fn cannot_rebind_a_consumed_local() {
    let f = function(vec![
        Instruction::Drop(0),
        Instruction::Let {
            binding: 0,
            value: Operation::Literal(Lit::Unit),
        },
    ]);
    assert_eq!(validate(&f), Err(Error::DuplicateBinding(0)));
}

#[test]
fn projection_requires_a_live_receiver_and_returns_an_owned_token() {
    let projection = Instruction::Let {
        binding: 1,
        value: Operation::Project {
            object: 0,
            field: 0,
            rep: ValueRep::Cell,
        },
    };
    let mut f = function(vec![projection.clone(), Instruction::Drop(1)]);
    assert_eq!(validate(&f), Ok(()));
    f.body.instructions = vec![Instruction::Drop(0), projection];
    assert_eq!(validate(&f), Err(Error::NoToken(0)));
}

fn forwarding_program() -> Program {
    let initializer = |code| Function {
        params: vec![],
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 0,
                value: Operation::Closure {
                    code,
                    captures: vec![],
                },
            }],
            exit: Exit::Return(0),
        },
    };
    let forwarded = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![Instruction::Drop(0)],
            exit: Exit::Return(1),
        },
    };
    Program {
        functions: BTreeMap::from([(0, initializer(2)), (1, initializer(2)), (2, forwarded)]),
        globals: vec![
            Global {
                name: Name::new("A.B", "c"),
                initializer: 0,
            },
            Global {
                name: Name::new("A", "B.c"),
                initializer: 1,
            },
        ],
        op_spans: BTreeMap::new(),
    }
}

#[test]
fn program_validation_checks_qualified_symbols_and_calling_conventions() {
    let mut program = forwarding_program();
    assert_eq!(validate_program(&program), Ok(()));
    program.globals.push(program.globals[0].clone());
    assert_eq!(
        validate_program(&program),
        Err(Error::DuplicateGlobal(Name::new("A.B", "c")))
    );
    program.globals.pop();
    program.functions.remove(&2);
    assert_eq!(
        validate_program(&program),
        Err(Error::InCode {
            code: 0,
            error: Box::new(Error::UnknownCode(2))
        })
    );
    let mut program = forwarding_program();
    program.globals[0].initializer = 2;
    assert_eq!(
        validate_program(&program),
        Err(Error::Arity {
            code: 2,
            expected: 2,
            actual: 0
        })
    );
}

#[test]
fn program_validation_rejects_an_intrinsic_with_wrong_consuming_arity() {
    let function = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 2,
                value: Operation::Intrinsic {
                    kind: Intrinsic::Show,
                    args: vec![0, 1],
                },
            }],
            exit: Exit::Return(2),
        },
    };
    let program = Program {
        functions: BTreeMap::from([(0, function)]),
        globals: vec![],
        op_spans: BTreeMap::new(),
    };
    assert_eq!(
        validate_program(&program),
        Err(Error::InCode {
            code: 0,
            error: Box::new(Error::IntrinsicArity {
                kind: Intrinsic::Show,
                actual: 2
            })
        })
    );
}

#[test]
fn typed_failure_intrinsics_consume_exactly_their_operands_in_all_code() {
    for (kind, arity) in [
        (Intrinsic::Try, 2),
        (Intrinsic::FromResult, 1),
        (Intrinsic::Raise, 1),
    ] {
        assert_eq!(kind.arity(), arity);
        let operation = Operation::Intrinsic {
            kind,
            args: vec![0; arity],
        };
        assert_eq!(operation.consumed(), vec![0; arity]);
        let mut function = Function {
            params: vec![0],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 1,
                    value: operation,
                }],
                exit: Exit::Return(1),
            },
        };
        if arity == 2 {
            assert_eq!(validate(&function), Err(Error::NoToken(0)));
            function.body.instructions.insert(0, Instruction::Dup(0));
        }
        assert_eq!(validate(&function), Ok(()));
        let mut program = forwarding_program();
        program.functions.insert(99, function);
        assert_eq!(validate_program(&program), Ok(()));
        let function = program.functions.get_mut(&99).expect("unselected function");
        function.body.instructions = vec![
            Instruction::Drop(0),
            Instruction::Let {
                binding: 1,
                value: Operation::Intrinsic { kind, args: vec![] },
            },
        ];
        assert_eq!(
            validate_program(&program),
            Err(Error::InCode {
                code: 99,
                error: Box::new(Error::IntrinsicArity { kind, actual: 0 }),
            })
        );
    }
}

#[test]
#[cfg(feature = "json")]
fn typed_failure_intrinsics_round_trip_without_changing_old_wire_names() {
    let operations: Vec<_> = [Intrinsic::Try, Intrinsic::FromResult, Intrinsic::Raise]
        .into_iter()
        .map(|kind| Operation::Intrinsic {
            kind,
            args: (0..kind.arity())
                .map(|i| u32::try_from(i).expect("small arity"))
                .collect(),
        })
        .collect();
    let json = serde_json::to_string(&operations).expect("serialize typed failures");
    assert_eq!(
        serde_json::from_str::<Vec<Operation>>(&json).expect("read typed failures"),
        operations
    );
    assert_eq!(
        serde_json::to_string(&Intrinsic::MatchFail).expect("old variant"),
        "\"MatchFail\""
    );
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("serialize typed failures");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("read typed failures");
        assert_eq!(back, operations);
    }
}

#[test]
fn selected_owner_helpers_validate_their_consumed_arity() {
    let mut program = Program {
        functions: BTreeMap::from([
            (
                0,
                Function {
                    params: vec![0, 1],
                    body: Block {
                        instructions: vec![Instruction::Let {
                            binding: 2,
                            value: Operation::Within {
                                owner: 0,
                                code: 1,
                                args: vec![1],
                            },
                        }],
                        exit: Exit::Return(2),
                    },
                },
            ),
            (
                1,
                Function {
                    params: vec![0],
                    body: Block {
                        instructions: vec![],
                        exit: Exit::Return(0),
                    },
                },
            ),
        ]),
        globals: vec![],
        op_spans: BTreeMap::new(),
    };
    assert_eq!(validate_program(&program), Ok(()));
    program
        .functions
        .get_mut(&1)
        .expect("helper")
        .params
        .push(1);
    assert_eq!(
        validate_program(&program),
        Err(Error::InCode {
            code: 0,
            error: Box::new(Error::Arity {
                code: 1,
                expected: 2,
                actual: 1
            }),
        })
    );
}

#[test]
fn stage2_control_var_loop_and_generator_operations_validate_arity_and_tokens() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let helper_one = Function {
        params: vec![0],
        body: Block {
            instructions: vec![],
            exit: Exit::Return(0),
        },
    };
    let helper_two = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![Instruction::Drop(0)],
            exit: Exit::Return(1),
        },
    };
    let main = Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                bind(1, Operation::VarMake(0)),
                Instruction::Dup(1),
                bind(2, Operation::VarGet(1)),
                bind(3, Operation::VarSet { var: 1, value: 2 }),
                bind(
                    4,
                    Operation::Loop {
                        id: "L".into(),
                        code: 1,
                        args: vec![3],
                    },
                ),
                bind(
                    5,
                    Operation::FunctionBoundary {
                        code: 1,
                        args: vec![4],
                    },
                ),
                bind(
                    6,
                    Operation::GenerateStrict {
                        code: 1,
                        args: vec![5],
                    },
                ),
                bind(
                    7,
                    Operation::GenerateLazy {
                        code: 2,
                        captures: vec![6],
                        limit: None,
                        sync: false,
                    },
                ),
                bind(8, Operation::SeqOpen(7)),
                bind(9, Operation::SeqNext(8)),
                bind(10, Operation::UnwrapOption(9)),
                bind(11, Operation::Yield(10)),
                Instruction::Drop(11),
                bind(12, Operation::Label("name".into())),
                bind(
                    13,
                    Operation::LoopExit {
                        id: "L".into(),
                        value: Some(12),
                    },
                ),
            ],
            exit: Exit::Return(13),
        },
    };
    let mut program = Program {
        functions: BTreeMap::from([(0, main), (1, helper_one), (2, helper_two)]),
        globals: vec![],
        op_spans: BTreeMap::new(),
    };
    assert_eq!(validate_program(&program), Ok(()));
    program.functions.get_mut(&2).expect("gen").params.push(2);
    assert_eq!(
        validate_program(&program),
        Err(Error::InCode {
            code: 0,
            error: Box::new(Error::Arity {
                code: 2,
                expected: 3,
                actual: 2
            }),
        })
    );
}

#[test]
fn validates_foreign_call_arity_and_ownership() {
    let spec = Box::new(ForeignCallSpec {
        module: "App.Main".into(),
        source_dir: Some("/tmp".into()),
        from: Some("./bindings.mjs".into()),
        name: "combine".into(),
        mode: crate::ForeignMode::Pure,
        dicts: 0,
        arity: 2,
        taken: 2,
        runtime: false,
        has_catch: false,
        adapter: crate::Adapter::default(),
        arg_hints: vec![WireHint::Float, WireHint::Float],
        result_hint: WireHint::Float,
        remaining_stages: vec![],
    });
    let valid = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 2,
                value: Operation::ForeignCall {
                    spec: spec.clone(),
                    args: vec![0, 1],
                },
            }],
            exit: Exit::Return(2),
        },
    };
    let program = Program {
        functions: BTreeMap::from([(0, valid)]),
        globals: vec![],
        op_spans: BTreeMap::new(),
    };
    assert_eq!(validate_program(&program), Ok(()));

    let bad_arity = Function {
        params: vec![0],
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 1,
                value: Operation::ForeignCall {
                    spec,
                    args: vec![0],
                },
            }],
            exit: Exit::Return(1),
        },
    };
    let bad_program = Program {
        functions: BTreeMap::from([(0, bad_arity)]),
        globals: vec![],
        op_spans: BTreeMap::new(),
    };
    assert_eq!(
        validate_program(&bad_program),
        Err(Error::InCode {
            code: 0,
            error: Box::new(Error::ForeignArity {
                name: "combine".into(),
                expected: 2,
                actual: 1,
            }),
        })
    );
}

#[test]
fn drop_reuse_and_reuse_record_validate_exact_token_balance() {
    let valid = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                Instruction::Let {
                    binding: 2,
                    value: Operation::DropReuse(0),
                },
                Instruction::Let {
                    binding: 3,
                    value: Operation::ReuseRecord {
                        object: 2,
                        fields: vec![("x".into(), 1)],
                    },
                },
            ],
            exit: Exit::Return(3),
        },
    };
    assert_eq!(validate(&valid), Ok(()));

    let leaked_token = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 2,
                value: Operation::DropReuse(0),
            }],
            exit: Exit::Return(1),
        },
    };
    assert_eq!(validate(&leaked_token), Err(Error::Leftover(vec![(2, 1)])));

    let missing_candidate = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                Instruction::Drop(0),
                Instruction::Let {
                    binding: 2,
                    value: Operation::ReuseRecord {
                        object: 0,
                        fields: vec![("x".into(), 1)],
                    },
                },
            ],
            exit: Exit::Return(2),
        },
    };
    assert_eq!(validate(&missing_candidate), Err(Error::NoToken(0)));
}
