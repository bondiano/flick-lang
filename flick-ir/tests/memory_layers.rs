//! Constant Layer boundaries transfer real owned operands, not callbacks in syntax.
use std::collections::BTreeMap;

use flick_ir::memory::{
    self, Block, Error, Exit, Function, Instruction, Intrinsic, Operation, Program,
};

fn function(params: Vec<u32>, operation: Operation) -> Function {
    Function {
        params,
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 7,
                value: operation,
            }],
            exit: Exit::Return(7),
        },
    }
}

#[test]
fn service_tags_are_owned_results_without_input_tokens() {
    let operation = Operation::ServiceTag("App.Child.Service".into());
    assert!(operation.consumed().is_empty());
    assert_eq!(operation.borrowed(), None);
    assert_eq!(memory::validate(&function(vec![], operation)), Ok(()));
}

#[test]
fn both_layer_intrinsics_consume_exactly_two_owned_tokens() {
    for kind in [Intrinsic::LayerSucceed, Intrinsic::LayerRun] {
        assert_eq!(kind.arity(), 2);
        let operation = Operation::Intrinsic {
            kind,
            args: vec![0, 1],
        };
        assert_eq!(operation.consumed(), [0, 1]);
        assert_eq!(operation.borrowed(), None);
        let valid = function(vec![0, 1], operation);
        let program = Program {
            functions: BTreeMap::from([(3, valid)]),
            globals: vec![],
        };
        assert_eq!(memory::validate_program(&program), Ok(()));

        let invalid = function(
            vec![0],
            Operation::Intrinsic {
                kind,
                args: vec![0],
            },
        );
        let program = Program {
            functions: BTreeMap::from([(3, invalid)]),
            globals: vec![],
        };
        assert_eq!(
            memory::validate_program(&program),
            Err(Error::InCode {
                code: 3,
                error: Box::new(Error::IntrinsicArity { kind, actual: 1 }),
            })
        );

        let repeated = function(
            vec![0],
            Operation::Intrinsic {
                kind,
                args: vec![0, 0],
            },
        );
        assert_eq!(memory::validate(&repeated), Err(Error::NoToken(0)));
    }
}

#[test]
#[cfg(feature = "json")]
fn layer_ir_round_trips_without_changing_old_intrinsic_shapes() {
    assert_eq!(
        serde_json::to_string(&Intrinsic::Try).expect("old intrinsic"),
        "\"Try\""
    );
    let operations = vec![
        Operation::ServiceTag("App.Child.Service".into()),
        Operation::Intrinsic {
            kind: Intrinsic::LayerSucceed,
            args: vec![0, 1],
        },
        Operation::Intrinsic {
            kind: Intrinsic::LayerRun,
            args: vec![2, 3],
        },
    ];
    let json = serde_json::to_string(&operations).expect("Layer JSON");
    let back: Vec<Operation> = serde_json::from_str(&json).expect("Layer JSON read");
    assert_eq!(back, operations);
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("Layer CBOR");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("Layer CBOR read");
        assert_eq!(back, operations);
    }
}
