use flick_ir::memory::{
    self, Block, Error, Exit, Function, Instruction, Intrinsic, Operation, Program,
};
use std::collections::BTreeMap;

fn function(kind: Intrinsic, args: Vec<u32>) -> Function {
    Function {
        params: args.clone(),
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 9,
                value: Operation::Intrinsic { kind, args },
            }],
            exit: Exit::Return(9),
        },
    }
}

#[test]
fn exact_new_arity_and_consuming_ownership_are_validated_even_in_unselected_code() {
    for (kind, arity) in [
        (Intrinsic::IntUnsafeDiv, 2),
        (Intrinsic::IntUnsafeRem, 2),
        (Intrinsic::StringLength, 1),
    ] {
        assert_eq!(kind.arity(), arity);
        let args: Vec<_> = (0..u32::try_from(arity).expect("small arity")).collect();
        let operation = Operation::Intrinsic {
            kind,
            args: args.clone(),
        };
        assert_eq!(operation.consumed(), args);
        assert_eq!(operation.borrowed(), None);
        assert_eq!(memory::validate(&function(kind, args)), Ok(()));
        let program = Program {
            globals: vec![],
            functions: BTreeMap::from([(17, function(kind, vec![]))]),
            op_spans: BTreeMap::new(),
        };
        assert_eq!(
            memory::validate_program(&program),
            Err(Error::InCode {
                code: 17,
                error: Box::new(Error::IntrinsicArity { kind, actual: 0 }),
            })
        );
    }
}

#[test]
#[cfg(feature = "json")]
fn primitive_wire_names_are_exact_and_old_tokens_stay_unchanged() {
    for (kind, json) in [
        (Intrinsic::IntUnsafeDiv, "\"IntUnsafeDiv\""),
        (Intrinsic::IntUnsafeRem, "\"IntUnsafeRem\""),
        (Intrinsic::StringLength, "\"StringLength\""),
        (Intrinsic::Try, "\"Try\""),
        (Intrinsic::LayerMerge, "\"LayerMerge\""),
    ] {
        assert_eq!(serde_json::to_string(&kind).expect("JSON encode"), json);
        assert_eq!(
            serde_json::from_str::<Intrinsic>(json).expect("JSON decode"),
            kind
        );
        #[cfg(feature = "cbor")]
        {
            let mut bytes = Vec::new();
            ciborium::into_writer(&kind, &mut bytes).expect("CBOR encode");
            assert_eq!(
                ciborium::from_reader::<Intrinsic, _>(bytes.as_slice()).expect("CBOR decode"),
                kind
            );
        }
    }
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&Intrinsic::Try, &mut bytes).expect("old CBOR encode");
        assert_eq!(bytes, [0x63, b'T', b'r', b'y']);
    }
}
