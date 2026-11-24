mod common;

use flick_backend::*;

use common::{descriptor, request};

fn sample_value_abi() -> ValueAbi {
    ValueAbi {
        abi: Abi {
            name: "test.sample-ffi".into(),
            version: "0.1.0".into(),
        },
        opaque_type: "void *".into(),
        catch_error_type: "const char *".into(),
        catch_lang_item: Some("string".into()),
        entries: vec![
            ValueAbiEntry {
                key: "int".into(),
                flick_type: "Int".into(),
                host_type: "int32_t".into(),
                conversion: AbiConversion::None,
                direction: AbiDirection::Both,
                notes: "32-bit signed integer".into(),
            },
            ValueAbiEntry {
                key: "function".into(),
                flick_type: "a -> b".into(),
                host_type: "void (*)(...)".into(),
                conversion: AbiConversion::None,
                direction: AbiDirection::In,
                notes: "Inbound callback pointer".into(),
            },
        ],
        unsupported: vec![
            ValueAbiRefusal {
                key: "bigint".into(),
                reason: "no direct BigInt ABI".into(),
            },
            ValueAbiRefusal {
                key: "record".into(),
                reason: "records have no stable layout".into(),
            },
        ],
        fallback_reason: "type is outside the test.sample-ffi table".into(),
        admits_copied_data: false,
    }
}

#[test]
fn valid_value_abi_passes_validation_and_handshake() {
    let mut desc = descriptor();
    desc.value_abi = Some(sample_value_abi());
    assert_eq!(validate_value_abi(&desc), Ok(()));
    let req = request();
    assert_eq!(handshake(&req.backend, &desc, &req.target), Ok(()));
}

#[test]
fn malformed_value_abi_tables_are_rejected_in_handshake() {
    let cases = [
        "bad-abi-name",
        "bad-semver",
        "empty-opaque",
        "empty-catch",
        "bad-catch-lang-item",
        "empty-fallback",
        "record-in-entries",
        "adt-in-entries",
        "unknown-entry-key",
        "duplicate-entry-key",
        "empty-flick-type",
        "empty-host-type",
        "unknown-refusal-key",
        "entry-and-refusal-collision",
        "duplicate-refusal-key",
        "empty-refusal-reason",
    ];
    let req = request();
    for case in cases {
        let mut desc = descriptor();
        let mut abi = sample_value_abi();
        match case {
            "bad-abi-name" => abi.abi.name = "BadName".into(),
            "bad-semver" => abi.abi.version = "not-semver".into(),
            "empty-opaque" => abi.opaque_type = "   ".into(),
            "empty-catch" => abi.catch_error_type = String::new(),
            "bad-catch-lang-item" => abi.catch_lang_item = Some("not_a_lang_item".into()),
            "empty-fallback" => abi.fallback_reason = String::new(),
            "record-in-entries" => abi.entries.push(ValueAbiEntry {
                key: "record".into(),
                flick_type: "{ ... }".into(),
                host_type: "struct".into(),
                conversion: AbiConversion::Copy,
                direction: AbiDirection::Both,
                notes: String::new(),
            }),
            "adt-in-entries" => abi.entries.push(ValueAbiEntry {
                key: "adt".into(),
                flick_type: "Adt".into(),
                host_type: "union".into(),
                conversion: AbiConversion::Copy,
                direction: AbiDirection::Both,
                notes: String::new(),
            }),
            "unknown-entry-key" => abi.entries.push(ValueAbiEntry {
                key: "nonexistent_type".into(),
                flick_type: "Foo".into(),
                host_type: "foo_t".into(),
                conversion: AbiConversion::None,
                direction: AbiDirection::Both,
                notes: String::new(),
            }),
            "duplicate-entry-key" => {
                let first = abi.entries[0].clone();
                abi.entries.push(first);
            }
            "empty-flick-type" => abi.entries[0].flick_type = String::new(),
            "empty-host-type" => abi.entries[0].host_type = "  ".into(),
            "unknown-refusal-key" => abi.unsupported.push(ValueAbiRefusal {
                key: "unknown_key".into(),
                reason: "nope".into(),
            }),
            "entry-and-refusal-collision" => abi.unsupported.push(ValueAbiRefusal {
                key: "int".into(),
                reason: "contradiction".into(),
            }),
            "duplicate-refusal-key" => {
                let first = abi.unsupported[0].clone();
                abi.unsupported.push(first);
            }
            "empty-refusal-reason" => abi.unsupported[0].reason = " ".into(),
            _ => unreachable!(),
        }
        desc.value_abi = Some(abi);
        assert!(validate_value_abi(&desc).is_err(), "{case}");
        assert!(
            handshake(&req.backend, &desc, &req.target).is_err(),
            "{case}"
        );
    }
}

#[test]
fn literal_backend_rejects_mismatched_foreign_binding_abi_with_f014() {
    let backend = flick_test_backend::Literal;
    let desc = backend.describe();
    let mut req = request();
    req.backend = BackendPin {
        identity: desc.identity.clone(),
        sha256: sha256(b"literal-fixture"),
    };
    req.target.environment = Environment {
        backend: "literal".into(),
        platform: "native".into(),
    };
    req.target.options.push(OptionValue {
        key: "option.case".into(),
        value: "foreign".into(),
    });
    req.foreign.push(ForeignBinding {
        module: "App".into(),
        specifier: "./native.c".into(),
        resolved: "./native.c".into(),
        abi: Abi {
            name: "test.literal-ffi".into(),
            version: "9.9.9".into(),
        },
        inputs: vec![],
    });
    let mut inputs = DeclaredInputs::new(&req.inputs, |_| Some(common::TEXT.as_bytes().to_vec()))
        .expect("inputs");
    let check_err = backend
        .check_target(&req, &mut inputs)
        .expect_err("check_target must reject mismatched foreign binding ABI");
    assert!(
        check_err.iter().any(|d| d.code == "F014"
            && d.message.contains("test.literal-ffi@0.1.0")
            && d.message.contains("test.literal-ffi@9.9.9")),
        "expected F014 from check_target, got {check_err:?}"
    );
    let gen_err = backend
        .generate(&req, &mut inputs)
        .expect_err("generate must reject mismatched foreign binding ABI");
    assert!(
        gen_err.iter().any(|d| d.code == "F014"
            && d.message.contains("test.literal-ffi@0.1.0")
            && d.message.contains("test.literal-ffi@9.9.9")),
        "expected F014 from generate, got {gen_err:?}"
    );
}
