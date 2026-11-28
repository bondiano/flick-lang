#[test]
fn validates_nested_aggregates_and_box_unbox_token_balance() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let layout = Some(CellLayout {
        capacity: 2,
        align: 8,
        ownership: "boxed-then-imm".into(),
    });
    let balanced = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                bind(2, Operation::Literal(Lit::Int(42))),
                bind(
                    3,
                    Operation::Construct {
                        tag: "App.Imm.Imm".into(),
                        fields: vec![2],
                        rep: ValueRep::Aggregate,
                        layout: None,
                    },
                ),
                Instruction::Dup(3),
                Instruction::Drop(3),
                bind(
                    4,
                    Operation::Construct {
                        tag: "App.Inner.Inner".into(),
                        fields: vec![0, 3],
                        rep: ValueRep::Aggregate,
                        layout: None,
                    },
                ),
                bind(
                    5,
                    Operation::ConstructWithDicts {
                        tag: "App.Outer.Outer".into(),
                        fields: vec![4],
                        dicts: vec![1],
                        rep: ValueRep::Aggregate,
                        layout: None,
                    },
                ),
                bind(
                    6,
                    Operation::Project {
                        object: 5,
                        field: 0,
                        rep: ValueRep::Aggregate,
                    },
                ),
                bind(
                    7,
                    Operation::Match {
                        object: 6,
                        pattern: Box::new(Pat::Wild),
                        rep: ValueRep::Aggregate,
                    },
                ),
                Instruction::Drop(7),
                Instruction::Drop(5),
                bind(8, Operation::BoxAggregate { value: 6, layout }),
                bind(9, Operation::UnboxBorrow { object: 8 }),
                Instruction::Drop(9),
                bind(10, Operation::UnboxConsume { object: 8 }),
            ],
            exit: Exit::Return(10),
        },
    };
    assert_eq!(validate(&balanced), Ok(()));
}

#[test]
fn rejects_unbalanced_aggregate_dup_and_stale_unbox() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let unbalanced_dup = Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                bind(
                    1,
                    Operation::Construct {
                        tag: "App.Box.Box".into(),
                        fields: vec![0],
                        rep: ValueRep::Aggregate,
                        layout: None,
                    },
                ),
                Instruction::Dup(1),
            ],
            exit: Exit::Return(1),
        },
    };
    assert_eq!(
        validate(&unbalanced_dup),
        Err(Error::Leftover(vec![(1, 1)]))
    );

    let stale_unbox = Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                bind(1, Operation::UnboxConsume { object: 0 }),
                Instruction::Drop(1),
                bind(2, Operation::UnboxBorrow { object: 0 }),
            ],
            exit: Exit::Return(2),
        },
    };
    assert_eq!(validate(&stale_unbox), Err(Error::NoToken(0)));
}

#[test]
fn rejects_statically_produced_aggregate_reuse_candidates() {
    let bind = |binding, value| Instruction::Let { binding, value };
    for (op_name, reuse_op) in [
        ("DropReuse", Operation::DropReuse(1)),
        (
            "Reuse",
            Operation::Reuse {
                object: 1,
                tag: "App.T.T".into(),
                fields: vec![],
                rep: ValueRep::Cell,
                layout: None,
            },
        ),
        (
            "ReuseWithDicts",
            Operation::ReuseWithDicts {
                object: 1,
                tag: "App.T.T".into(),
                fields: vec![],
                dicts: vec![],
                rep: ValueRep::Cell,
                layout: None,
            },
        ),
        (
            "ReuseRecord",
            Operation::ReuseRecord {
                object: 1,
                fields: vec![],
            },
        ),
    ] {
        let f = Function {
            params: vec![0],
            body: Block {
                instructions: vec![
                    bind(
                        1,
                        Operation::Construct {
                            tag: "App.Agg.Agg".into(),
                            fields: vec![0],
                            rep: ValueRep::Aggregate,
                            layout: None,
                        },
                    ),
                    bind(2, reuse_op),
                ],
                exit: Exit::Return(2),
            },
        };
        assert_eq!(
            validate(&f),
            Err(Error::AggregateReuseCandidate(1)),
            "{op_name} must reject aggregate produced by Construct"
        );
    }

    for (name, unbox) in [
        ("UnboxBorrow", Operation::UnboxBorrow { object: 0 }),
        ("UnboxConsume", Operation::UnboxConsume { object: 0 }),
    ] {
        let mut instructions = vec![bind(1, unbox)];
        if name == "UnboxBorrow" {
            instructions.push(Instruction::Drop(0));
        }
        instructions.push(bind(2, Operation::DropReuse(1)));
        let f = Function {
            params: vec![0],
            body: Block {
                instructions,
                exit: Exit::Return(2),
            },
        };
        assert_eq!(
            validate(&f),
            Err(Error::AggregateReuseCandidate(1)),
            "DropReuse must reject aggregate produced by {name}"
        );
    }
}

#[test]
fn rejects_explicit_and_observed_aggregate_reuse_candidates() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let explicit_reuse = Function {
        params: vec![0],
        body: Block {
            instructions: vec![bind(
                1,
                Operation::Reuse {
                    object: 0,
                    tag: "App.T.T".into(),
                    fields: vec![],
                    rep: ValueRep::Aggregate,
                    layout: None,
                },
            )],
            exit: Exit::Return(1),
        },
    };
    assert_eq!(
        validate(&explicit_reuse),
        Err(Error::AggregateReuseCandidate(0))
    );

    let explicit_reuse_dicts = Function {
        params: vec![0],
        body: Block {
            instructions: vec![bind(
                1,
                Operation::ReuseWithDicts {
                    object: 0,
                    tag: "App.T.T".into(),
                    fields: vec![],
                    dicts: vec![],
                    rep: ValueRep::Aggregate,
                    layout: None,
                },
            )],
            exit: Exit::Return(1),
        },
    };
    assert_eq!(
        validate(&explicit_reuse_dicts),
        Err(Error::AggregateReuseCandidate(0))
    );

    let revealed_by_project = Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                bind(
                    1,
                    Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Aggregate,
                    },
                ),
                Instruction::Drop(1),
                bind(2, Operation::DropReuse(0)),
            ],
            exit: Exit::Return(2),
        },
    };
    assert_eq!(
        validate(&revealed_by_project),
        Err(Error::AggregateReuseCandidate(0))
    );

    let revealed_by_match = Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                bind(
                    1,
                    Operation::Match {
                        object: 0,
                        pattern: Box::new(Pat::Wild),
                        rep: ValueRep::Aggregate,
                    },
                ),
                Instruction::Drop(1),
                bind(
                    2,
                    Operation::Reuse {
                        object: 0,
                        tag: "App.T.T".into(),
                        fields: vec![],
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                ),
            ],
            exit: Exit::Return(2),
        },
    };
    assert_eq!(
        validate(&revealed_by_match),
        Err(Error::AggregateReuseCandidate(0))
    );
}

#[test]
fn rejects_duplicate_parameters_even_with_balanced_body() {
    let dup_params = Function {
        params: vec![0, 0],
        body: Block {
            instructions: vec![],
            exit: Exit::Return(0),
        },
    };
    assert_eq!(validate(&dup_params), Err(Error::DuplicateBinding(0)));
}

#[test]
fn rejects_aggregate_reuse_in_nested_unselected_branch() {
    let bind = |binding, value| Instruction::Let { binding, value };
    let nested_bad = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                bind(2, Operation::UnboxBorrow { object: 1 }),
                Instruction::Drop(1),
            ],
            exit: Exit::Branch {
                condition: 0,
                then_: Box::new(Block {
                    instructions: vec![],
                    exit: Exit::Return(2),
                }),
                else_: Box::new(Block {
                    instructions: vec![bind(3, Operation::DropReuse(2))],
                    exit: Exit::Return(3),
                }),
            },
        },
    };
    assert_eq!(
        validate(&nested_bad),
        Err(Error::AggregateReuseCandidate(2))
    );
}
