//! Dictionaries own their entries; lookup and structural views borrow receivers.
use flick_ir::memory::{
    self, Block, DictionaryEntry, Error, Exit, Function, Instruction, Operation,
};

fn borrowing(operation: Operation) -> Function {
    Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                Instruction::Let {
                    binding: 1,
                    value: operation,
                },
                Instruction::Drop(0),
            ],
            exit: Exit::Return(1),
        },
    }
}

#[test]
fn entries_preserve_written_order_and_repeated_owned_operands() {
    let operation = Operation::Dictionary {
        entries: vec![
            ("value".into(), DictionaryEntry::Value(0)),
            ("method".into(), DictionaryEntry::Getter(1)),
            ("alias".into(), DictionaryEntry::Value(0)),
        ],
    };
    assert_eq!(operation.consumed(), [0, 1, 0]);
    assert_eq!(operation.borrowed(), None);
    let mut function = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 2,
                value: operation,
            }],
            exit: Exit::Return(2),
        },
    };
    assert_eq!(memory::validate(&function), Err(Error::NoToken(0)));
    function.body.instructions.insert(0, Instruction::Dup(0));
    assert_eq!(memory::validate(&function), Ok(()));
}

#[test]
fn method_and_structure_keep_the_original_receiver_token() {
    for operation in [
        Operation::Method {
            dictionary: 0,
            field: "show".into(),
        },
        Operation::Structure {
            object: 0,
            expected: 2,
        },
    ] {
        assert!(operation.consumed().is_empty());
        assert_eq!(operation.borrowed(), Some(0));
        let function = borrowing(operation);
        assert_eq!(memory::validate(&function), Ok(()));
        let mut missing = function.clone();
        missing.body.instructions.pop();
        assert_eq!(
            memory::validate(&missing),
            Err(Error::Leftover(vec![(0, 1)]))
        );
    }
}

#[test]
#[cfg(feature = "json")]
fn dictionary_ir_round_trips_without_reinterpreting_records() {
    let operations = vec![
        Operation::Dictionary {
            entries: vec![
                ("value".into(), DictionaryEntry::Value(0)),
                ("default".into(), DictionaryEntry::Getter(1)),
            ],
        },
        Operation::Method {
            dictionary: 2,
            field: "default".into(),
        },
        Operation::Structure {
            object: 3,
            expected: 2,
        },
    ];
    let json = serde_json::to_string(&operations).expect("dictionary JSON");
    let back: Vec<Operation> = serde_json::from_str(&json).expect("dictionary JSON read");
    assert_eq!(back, operations);
    assert_eq!(
        serde_json::to_string(&Operation::Record {
            fields: vec![("value".into(), 0)],
        })
        .expect("existing record encoding"),
        r#"{"Record":{"fields":[["value",0]]}}"#
    );
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("dictionary CBOR");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("dictionary CBOR read");
        assert_eq!(back, operations);
    }
}
