//! Numeric primitive operands remain explicit owned tokens.
use flick_ir::memory::{
    self, Block, Error, Exit, Function, Instruction, Intrinsic, Operation, Program,
};
use std::collections::BTreeMap;

fn function(params: Vec<u32>, kind: Intrinsic, args: Vec<u32>) -> Function {
    Function {
        params,
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 7,
                value: Operation::Intrinsic { kind, args },
            }],
            exit: Exit::Return(7),
        },
    }
}

#[test]
fn numeric_arity_and_owned_operand_multiplicity_are_checked() {
    for (kind, arity) in [
        (Intrinsic::PrimitiveCompare, 2),
        (Intrinsic::PrimitiveHash, 1),
    ] {
        assert_eq!(kind.arity(), arity);
        let locals: Vec<_> = (0..u32::try_from(arity).expect("small")).collect();
        let operation = Operation::Intrinsic {
            kind,
            args: locals.clone(),
        };
        assert_eq!(operation.consumed(), locals);
        assert_eq!(operation.borrowed(), None);
        let program = Program {
            functions: BTreeMap::from([(3, function(locals.clone(), kind, locals))]),
            globals: vec![],
        };
        assert_eq!(memory::validate_program(&program), Ok(()));
        let program = Program {
            functions: BTreeMap::from([(3, function(vec![], kind, vec![]))]),
            globals: vec![],
        };
        assert_eq!(
            memory::validate_program(&program),
            Err(Error::InCode {
                code: 3,
                error: Box::new(Error::IntrinsicArity { kind, actual: 0 })
            })
        );
    }
    assert_eq!(
        memory::validate(&function(vec![0], Intrinsic::PrimitiveCompare, vec![0, 0])),
        Err(Error::NoToken(0))
    );
    let mut duplicated = function(vec![0], Intrinsic::PrimitiveCompare, vec![0, 0]);
    duplicated.body.instructions.insert(0, Instruction::Dup(0));
    assert_eq!(memory::validate(&duplicated), Ok(()));
}

#[test]
#[cfg(feature = "json")]
fn numeric_operations_round_trip_without_changing_existing_intrinsics() {
    assert_eq!(
        serde_json::to_string(&Intrinsic::Try).expect("old shape"),
        "\"Try\""
    );
    let operations = [
        Operation::Intrinsic {
            kind: Intrinsic::PrimitiveCompare,
            args: vec![0, 1],
        },
        Operation::Intrinsic {
            kind: Intrinsic::PrimitiveHash,
            args: vec![2],
        },
    ];
    let json = serde_json::to_string(&operations).expect("numeric JSON");
    let back: Vec<Operation> = serde_json::from_str(&json).expect("numeric JSON read");
    assert_eq!(back, operations);
    #[cfg(feature = "cbor")]
    {
        let mut bytes = Vec::new();
        ciborium::into_writer(&operations, &mut bytes).expect("numeric CBOR");
        let back: Vec<Operation> =
            ciborium::from_reader(bytes.as_slice()).expect("numeric CBOR read");
        assert_eq!(back, operations);
    }
}
