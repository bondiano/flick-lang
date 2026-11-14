use flick_ir::memory::{
    self, Block, Error, Exit, Function, Instruction, Intrinsic, Operation, Program,
};

fn function(args: Vec<u32>) -> Function {
    Function {
        params: args.clone(),
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 9,
                value: Operation::Intrinsic {
                    kind: Intrinsic::ArrayToList,
                    args,
                },
            }],
            exit: Exit::Return(9),
        },
    }
}

#[test]
fn array_normalization_consumes_exactly_one_token_even_in_unselected_code() {
    assert_eq!(Intrinsic::ArrayToList.arity(), 1);
    let operation = Operation::Intrinsic {
        kind: Intrinsic::ArrayToList,
        args: vec![0],
    };
    assert_eq!(operation.consumed(), [0]);
    assert_eq!(operation.borrowed(), None);
    assert_eq!(memory::validate(&function(vec![0])), Ok(()));
    for args in [vec![], vec![0, 1]] {
        let actual = args.len();
        let program = Program {
            globals: vec![],
            functions: [(17, function(args))].into(),
        };
        assert_eq!(
            memory::validate_program(&program),
            Err(Error::InCode {
                code: 17,
                error: Box::new(Error::IntrinsicArity {
                    kind: Intrinsic::ArrayToList,
                    actual,
                }),
            })
        );
    }
}

#[test]
#[cfg(feature = "json")]
fn append_only_intrinsic_names_roundtrip_without_changing_prior_json_or_cbor() {
    for (kind, name) in [
        (Intrinsic::ArrayToList, "ArrayToList"),
        (Intrinsic::Try, "Try"),
        (Intrinsic::LayerMerge, "LayerMerge"),
        (Intrinsic::FloatMax, "FloatMax"),
    ] {
        let json = format!("\"{name}\"");
        assert_eq!(serde_json::to_string(&kind).expect("JSON encode"), json);
        assert_eq!(
            serde_json::from_str::<Intrinsic>(&json).expect("JSON decode"),
            kind
        );
        #[cfg(feature = "cbor")]
        {
            let mut bytes = Vec::new();
            ciborium::into_writer(&kind, &mut bytes).expect("CBOR encode");
            let mut expected = vec![0x60 + u8::try_from(name.len()).expect("short token")];
            expected.extend_from_slice(name.as_bytes());
            assert_eq!(bytes, expected, "append-only exact token");
            assert_eq!(
                ciborium::from_reader::<Intrinsic, _>(bytes.as_slice()).expect("CBOR decode"),
                kind
            );
        }
    }
}
