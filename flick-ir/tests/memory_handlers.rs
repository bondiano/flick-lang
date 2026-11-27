//! The ownership boundary for dynamic handler installation and dispatch.
use std::collections::BTreeMap;

use flick_ir::OpClass;
use flick_ir::memory::{
    self, Block, Error, Exit, Function, HandlerEffect, HandlerOperation, Instruction, Operation,
    Program,
};

fn handled(function: u32) -> HandlerEffect {
    HandlerEffect {
        effect: "App.Pair".into(),
        operations: vec![HandlerOperation {
            name: "right".into(),
            class: OpClass::Tail,
            dicts: 1,
            arity: 2,
            function,
        }],
    }
}

fn function(operation: Operation, params: Vec<u32>) -> Function {
    Function {
        params,
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 9,
                value: operation,
            }],
            exit: Exit::Return(9),
        },
    }
}

#[test]
fn handle_and_perform_consume_every_operand_with_multiplicity() {
    let install = Operation::Handle {
        body: 0,
        effects: vec![handled(1)],
        ret: Some(2),
        errors: Some(3),
    };
    assert_eq!(install.consumed(), [0, 1, 2, 3]);
    assert_eq!(install.borrowed(), None);
    assert_eq!(
        memory::validate(&function(install, vec![0, 1, 2, 3])),
        Ok(())
    );

    let perform = Operation::Perform {
        effect: "App.Pair".into(),
        op: "right".into(),
        dicts: vec![0],
        args: vec![1, 0],
    };
    assert_eq!(perform.consumed(), [0, 1, 0]);
    assert_eq!(perform.borrowed(), None);
    assert_eq!(
        memory::validate(&function(perform, vec![0, 1])),
        Err(Error::NoToken(0))
    );
}

#[test]
fn handler_metadata_is_checked_even_in_unreferenced_code() {
    let duplicate_effect = Operation::Handle {
        body: 0,
        effects: vec![handled(1), handled(2)],
        ret: None,
        errors: None,
    };
    assert_eq!(
        memory::validate(&function(duplicate_effect, vec![0, 1, 2])),
        Err(Error::DuplicateHandledEffect("App.Pair".into()))
    );

    let mut effect = handled(1);
    effect.operations.push(effect.operations[0].clone());
    let duplicate_operation = Operation::Handle {
        body: 0,
        effects: vec![effect],
        ret: None,
        errors: None,
    };
    assert_eq!(
        memory::validate(&function(duplicate_operation, vec![0, 1])),
        Err(Error::DuplicateHandledOperation {
            effect: "App.Pair".into(),
            op: "right".into(),
        })
    );

    for (class, dicts, arity) in [
        (OpClass::Tail, usize::MAX, 1),
        (OpClass::Control, usize::MAX, 0),
    ] {
        let mut effect = handled(1);
        effect.operations[0].class = class;
        effect.operations[0].dicts = dicts;
        effect.operations[0].arity = arity;
        let function = function(
            Operation::Handle {
                body: 0,
                effects: vec![effect],
                ret: None,
                errors: None,
            },
            vec![0, 1],
        );
        let expected = Error::HandlerArityOverflow {
            effect: "App.Pair".into(),
            op: "right".into(),
        };
        assert_eq!(memory::validate(&function), Err(expected.clone()));
        let program = Program {
            functions: BTreeMap::from([(7, function)]),
            globals: vec![],
            op_spans: BTreeMap::new(),
        };
        assert_eq!(
            memory::validate_program(&program),
            Err(Error::InCode {
                code: 7,
                error: Box::new(expected)
            })
        );
    }
}

#[test]
#[cfg(feature = "json")]
fn dynamic_handler_operations_round_trip() {
    let operations = vec![
        Operation::Handle {
            body: 0,
            effects: vec![handled(1)],
            ret: Some(2),
            errors: Some(3),
        },
        Operation::Perform {
            effect: "App.Pair".into(),
            op: "right".into(),
            dicts: vec![4],
            args: vec![5, 6],
        },
    ];
    let json = serde_json::to_string(&operations).expect("handler JSON");
    let back: Vec<Operation> = serde_json::from_str(&json).expect("handler JSON read");
    assert_eq!(back, operations);
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("handler CBOR");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("handler CBOR read");
        assert_eq!(back, operations);
    }
}
