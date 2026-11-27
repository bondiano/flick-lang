use flick_ir::{
    Lit,
    memory::{self, Block, Exit, Function, Instruction, Operation, Program},
};
use std::collections::BTreeMap;

#[test]
fn recovery_result_owns_only_selected_edge_and_does_not_borrow() {
    for selected in [None, Some(7)] {
        let operation = Operation::RecoveryResult { selected };
        assert_eq!(
            operation.consumed(),
            selected.into_iter().collect::<Vec<_>>()
        );
        assert_eq!(operation.borrowed(), None);
        let function = Function {
            params: selected.into_iter().collect(),
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 8,
                    value: operation,
                }],
                exit: Exit::Return(8),
            },
        };
        assert_eq!(memory::validate(&function), Ok(()));
    }
}

#[test]
fn invalid_selected_closure_arity_is_rejected_even_in_unselected_code() {
    for params in [vec![], vec![0], vec![0, 1, 2]] {
        let mut target = Function {
            params: params.clone(),
            body: Block {
                instructions: vec![],
                exit: Exit::Return(9),
            },
        };
        // Release every parameter to keep this a valid token program, so
        // the symbol/arity check, not a liveness error, rejects the target.
        target
            .body
            .instructions
            .extend(params.iter().copied().map(Instruction::Drop));
        target.body.instructions.push(Instruction::Let {
            binding: 9,
            value: Operation::Literal(Lit::Unit),
        });
        let selector = Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Closure {
                            code: 8,
                            captures: vec![],
                        },
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::RecoveryResult { selected: Some(0) },
                    },
                ],
                exit: Exit::Return(1),
            },
        };
        let program = Program {
            globals: vec![],
            functions: [(7, selector), (8, target)].into(),
            op_spans: BTreeMap::new(),
        };
        assert_eq!(
            memory::validate_program(&program),
            Err(memory::Error::InCode {
                code: 7,
                error: Box::new(memory::Error::Arity {
                    code: 8,
                    expected: params.len(),
                    actual: 2
                }),
            })
        );
    }
}

#[test]
#[cfg(feature = "json")]
fn append_only_recovery_operation_has_explicit_owned_optional_edge_wire_shape() {
    for selected in [None, Some(7)] {
        let operation = Operation::RecoveryResult { selected };
        let json = serde_json::to_string(&operation).expect("JSON encode");
        assert_eq!(
            json,
            match selected {
                None => "{\"RecoveryResult\":{\"selected\":null}}",
                Some(_) => "{\"RecoveryResult\":{\"selected\":7}}",
            }
        );
        assert_eq!(
            serde_json::from_str::<Operation>(&json).expect("JSON decode"),
            operation
        );
        #[cfg(feature = "cbor")]
        {
            let mut bytes = Vec::new();
            ciborium::into_writer(&operation, &mut bytes).expect("CBOR encode");
            let mut expected = vec![0xa1, 0x6e];
            expected.extend_from_slice(b"RecoveryResult");
            expected.extend_from_slice(&[0xa1, 0x68]);
            expected.extend_from_slice(b"selected");
            expected.push(selected.map_or(0xf6, |_| 7));
            assert_eq!(
                bytes, expected,
                "explicit stable operation token, not an enum ordinal"
            );
            assert_eq!(
                ciborium::from_reader::<Operation, _>(bytes.as_slice()).expect("CBOR decode"),
                operation
            );
        }
    }
    assert_eq!(
        serde_json::to_string(&Operation::Literal(Lit::Unit)).expect("old JSON"),
        "{\"Literal\":\"Unit\"}"
    );
}

#[test]
fn invalid_arity_in_a_dead_branch_is_still_rejected_before_execution() {
    let function = Function {
        params: vec![],
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 0,
                value: Operation::Literal(Lit::Bool(false)),
            }],
            exit: Exit::Branch {
                condition: 0,
                then_: Box::new(Block {
                    instructions: vec![
                        Instruction::Let {
                            binding: 1,
                            value: Operation::Closure {
                                code: 8,
                                captures: vec![],
                            },
                        },
                        Instruction::Let {
                            binding: 2,
                            value: Operation::RecoveryResult { selected: Some(1) },
                        },
                    ],
                    exit: Exit::Return(2),
                }),
                else_: Box::new(Block {
                    instructions: vec![Instruction::Let {
                        binding: 1,
                        value: Operation::RecoveryResult { selected: None },
                    }],
                    exit: Exit::Return(1),
                }),
            },
        },
    };
    let target = Function {
        params: vec![0],
        body: Block {
            instructions: vec![],
            exit: Exit::Return(0),
        },
    };
    let program = Program {
        globals: vec![],
        functions: [(7, function), (8, target)].into(),
        op_spans: BTreeMap::new(),
    };
    assert_eq!(
        memory::validate_program(&program),
        Err(memory::Error::InCode {
            code: 7,
            error: Box::new(memory::Error::Arity {
                code: 8,
                expected: 1,
                actual: 2
            }),
        })
    );
}
