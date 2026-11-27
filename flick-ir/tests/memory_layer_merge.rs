//! Merge is a new consuming operation; old serialized variants are unchanged.
use flick_ir::memory::{
    self, Block, Error, Exit, Function, Instruction, Intrinsic, Operation, Program,
};
use std::collections::BTreeMap;

#[test]
fn merge_requires_two_distinct_owned_tokens() {
    assert_eq!(Intrinsic::LayerMerge.arity(), 2);
    for args in [vec![0, 1], vec![0], vec![0, 0]] {
        let op = Operation::Intrinsic {
            kind: Intrinsic::LayerMerge,
            args: args.clone(),
        };
        assert_eq!(op.consumed(), args);
        assert_eq!(op.borrowed(), None);
        let function = Function {
            params: if args == [0, 1] { vec![0, 1] } else { vec![0] },
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 2,
                    value: op,
                }],
                exit: Exit::Return(2),
            },
        };
        if args == [0, 0] {
            assert_eq!(memory::validate(&function), Err(Error::NoToken(0)));
        } else {
            let result = memory::validate_program(&Program {
                functions: BTreeMap::from([(0, function)]),
                globals: vec![],
                op_spans: BTreeMap::new(),
            });
            if args.len() == 2 {
                assert_eq!(result, Ok(()));
            } else {
                assert_eq!(
                    result,
                    Err(Error::InCode {
                        code: 0,
                        error: Box::new(Error::IntrinsicArity {
                            kind: Intrinsic::LayerMerge,
                            actual: 1
                        })
                    })
                );
            }
        }
    }
}

#[test]
#[cfg(all(feature = "json", feature = "cbor"))]
fn merge_has_its_own_exact_json_and_cbor_wire_name() {
    let operation = Operation::Intrinsic {
        kind: Intrinsic::LayerMerge,
        args: vec![7, 8],
    };
    let json = serde_json::to_string(&operation).expect("serialize merge operation");
    assert_eq!(json, r#"{"Intrinsic":{"kind":"LayerMerge","args":[7,8]}}"#);
    assert_eq!(
        serde_json::from_str::<Operation>(&json).expect("decode merge operation"),
        operation
    );
    let mut operation_cbor = vec![];
    ciborium::into_writer(&operation, &mut operation_cbor).expect("encode merge operation CBOR");
    assert_eq!(
        operation_cbor,
        b"\xa1\x69Intrinsic\xa2\x64kind\x6aLayerMerge\x64args\x82\x07\x08"
    );
    assert_eq!(
        ciborium::from_reader::<Operation, _>(operation_cbor.as_slice())
            .expect("decode merge operation CBOR"),
        operation
    );
    let mut cbor = vec![];
    ciborium::into_writer(&Intrinsic::LayerMerge, &mut cbor).expect("encode merge intrinsic CBOR");
    assert_eq!(cbor, b"\x6aLayerMerge");
    assert_eq!(
        ciborium::from_reader::<Intrinsic, _>(cbor.as_slice())
            .expect("decode merge intrinsic CBOR"),
        Intrinsic::LayerMerge
    );
    for (old, name) in [
        (Intrinsic::LayerSucceed, "LayerSucceed"),
        (Intrinsic::LayerRun, "LayerRun"),
        (Intrinsic::PrimitiveHash, "PrimitiveHash"),
    ] {
        assert_eq!(
            serde_json::to_string(&old).expect("serialize existing intrinsic"),
            format!("\"{name}\"")
        );
    }
}
