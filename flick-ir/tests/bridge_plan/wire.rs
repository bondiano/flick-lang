#[cfg(feature = "json")]
#[test]
fn exact_json_wire_shape_and_rich_graph_roundtrip() {
    let expected = r#"{"kinds":["Type"],"witnesses":[],"constructors":[],"nodes":[{"kind":0,"node":{"Tuple":[]}}],"aliases":[],"entries":[{"root":0,"free":[],"select":"Value"}]}"#;
    assert_eq!(serde_json::to_string(&constant()).expect("JSON"), expected);
    let decoded: BridgePlan = serde_json::from_str(expected).expect("expected fixture");
    assert_eq!(decoded.validate_structure(), Ok(()));
    let plan = rich_graph();
    let encoded = serde_json::to_string(&plan).expect("rich JSON");
    assert!(encoded.contains(r#""native_direct":true"#));
    assert!(encoded.contains(r#"{"Capture":{"Rigid":1234}}"#));
    assert!(encoded.contains(r#"{"Provided":"Live"}"#));
    let decoded: BridgePlan = serde_json::from_str(&encoded).expect("rich decode");
    assert_eq!(decoded, plan);
    assert_eq!(decoded.validate_structure(), Ok(()));
}

#[cfg(feature = "cbor")]
#[test]
fn exact_cbor_shape_and_rich_graph_roundtrip() {
    // Explicit CBOR tokens: fixed maps, text field names, transparent u32 IDs,
    // externally tagged node enum and the unit Value selector string.
    let expected = b"\xa6\x65kinds\x81\x64Type\x69witnesses\x80\x6cconstructors\x80\x65nodes\x81\xa2\x64kind\x00\x64node\xa1\x65Tuple\x80\x67aliases\x80\x67entries\x81\xa3\x64root\x00\x64free\x80\x66select\x65Value";
    let mut bytes = Vec::new();
    ciborium::into_writer(&constant(), &mut bytes).expect("CBOR");
    assert_eq!(bytes, expected);
    let plan = rich_graph();
    bytes.clear();
    ciborium::into_writer(&plan, &mut bytes).expect("rich CBOR");
    let decoded: BridgePlan = ciborium::from_reader(bytes.as_slice()).expect("rich decode");
    assert_eq!(decoded, plan);
    assert_eq!(decoded.validate_structure(), Ok(()));
}

#[cfg(feature = "json")]
#[test]
fn standalone_module_does_not_change_program_serialization_or_version() {
    use flick_ir::{Program, VERSION};
    let program = Program::new("fixture", vec![], vec![], &|_| "unused".to_owned());
    // This standalone module neither pins nor bumps the existing Program
    // version. Its JSON has no bridge fields, on either side of an unrelated
    // typed-IR version change.
    let expected = format!(
        r#"{{"ir_version":"{VERSION}","compiler":"fixture","files":[],"modules":[],"lang":[]}}"#
    );
    assert_eq!(program.to_json(), expected);
    assert_eq!(Program::from_json(&expected).expect("old reader"), program);
    assert!(Program::from_json(&expected.replace(VERSION, "99.0.0")).is_err());
}

#[cfg(feature = "json")]
#[test]
fn decoded_malformed_graphs_require_validation_and_unresolved_cases_cannot_decode() {
    let mut plan = constant();
    plan.nodes[0].node = BridgeNode::Tuple(vec![BridgeNodeId(0)]);
    let json = serde_json::to_string(&plan).expect("malformed graph still has a wire form");
    let decoded: BridgePlan = serde_json::from_str(&json).expect("decode is not validation");
    assert!(decoded.validate_structure().is_err());
    for unresolved in [r#"{"Meta":0}"#, r#""Error""#] {
        assert!(serde_json::from_str::<BridgeNode>(unresolved).is_err());
        assert!(serde_json::from_str::<BridgeKind>(unresolved).is_err());
    }
}

#[cfg(feature = "json")]
#[test]
fn required_witness_json_fields_and_exact_witness_wire_form() {
    let plan = single_witness();
    let expected = r#"{"kinds":["Type","Module"],"witnesses":[{"scope":"Root","binder":"K","contract":1,"members":[{"owner_path":[],"name":"Item","kind":0}]}],"constructors":[{"identity":{"module":"$exists:K","name":"Item"},"origin":{"Witness":0},"kind":0,"meaning":"Nominal","service_impl":null,"dictionary_role":"Passed"}],"nodes":[{"kind":0,"node":{"Tuple":[]}},{"kind":1,"node":{"Rigid":17}},{"kind":0,"node":{"Exists":{"witness":0,"body":3}}},{"kind":0,"node":{"Con":0}}],"aliases":[],"entries":[{"root":2,"free":[],"select":"Value"}]}"#;
    assert_eq!(serde_json::to_string(&plan).expect("JSON"), expected);
    let decoded: BridgePlan = serde_json::from_str(expected).expect("exact witness JSON");
    assert_eq!(decoded.validate_structure(), Ok(()));
    for field in ["witnesses", "origin", "witness"] {
        let mut value = serde_json::to_value(&plan).expect("fixture JSON");
        let object = match field {
            "witnesses" => value.as_object_mut(),
            "origin" => value["constructors"][0].as_object_mut(),
            _ => value["nodes"][2]["node"]["Exists"].as_object_mut(),
        }
        .expect("object fixture");
        object.remove(field);
        assert!(
            serde_json::from_value::<BridgePlan>(value).is_err(),
            "missing {field} must not default"
        );
    }
    let both = both_exists();
    let decoded: BridgePlan =
        serde_json::from_str(&serde_json::to_string(&both).expect("Both JSON"))
            .expect("Both roundtrip");
    assert_eq!(decoded, both);
    assert_eq!(decoded.validate_structure(), Ok(()));
}

#[cfg(feature = "cbor")]
#[test]
fn required_witness_cbor_fields_and_exact_new_wire_tokens() {
    let plan = single_witness();
    let expected =
        b"\x81\xa4\x65scope\x64Root\x66binder\x61K\x68contract\x01\x67members\x81\xa3\x6aowner_path\x80\x64name\x64Item\x64kind\x00";
    let mut bytes = Vec::new();
    ciborium::into_writer(&plan.witnesses, &mut bytes).expect("witness CBOR");
    assert_eq!(bytes, expected);
    bytes.clear();
    ciborium::into_writer(&plan.constructors[0].origin, &mut bytes).expect("origin CBOR");
    assert_eq!(bytes, b"\xa1\x67Witness\x00");
    bytes.clear();
    ciborium::into_writer(&both_exists(), &mut bytes).expect("Both CBOR");
    let decoded: BridgePlan = ciborium::from_reader(bytes.as_slice()).expect("Both roundtrip");
    assert_eq!(decoded, both_exists());
    assert_eq!(decoded.validate_structure(), Ok(()));
    for field in ["witnesses", "origin", "witness"] {
        let mut value = Value::serialized(&plan).expect("CBOR value");
        remove_cbor_field(&mut value, field);
        bytes.clear();
        ciborium::into_writer(&value, &mut bytes).expect("malformed fixture");
        assert!(
            ciborium::from_reader::<BridgePlan, _>(bytes.as_slice()).is_err(),
            "missing {field} must not default"
        );
    }
}

#[cfg(all(feature = "json", feature = "cbor"))]
#[test]
fn witness_and_member_declarations_have_no_missing_field_defaults() {
    let plan = single_witness();
    for field in ["scope", "binder", "contract", "members"] {
        let mut json = serde_json::to_value(&plan.witnesses[0]).expect("witness JSON");
        json.as_object_mut().expect("witness object").remove(field);
        assert!(
            serde_json::from_value::<BridgeWitness>(json).is_err(),
            "required witness {field}"
        );
        let mut value = Value::serialized(&plan.witnesses[0]).expect("witness CBOR value");
        remove_cbor_field(&mut value, field);
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes).expect("malformed witness CBOR");
        assert!(
            ciborium::from_reader::<BridgeWitness, _>(bytes.as_slice()).is_err(),
            "required witness {field}"
        );
    }
    for field in ["owner_path", "name", "kind"] {
        let member = &plan.witnesses[0].members[0];
        let mut json = serde_json::to_value(member).expect("member JSON");
        json.as_object_mut().expect("member object").remove(field);
        assert!(
            serde_json::from_value::<BridgeWitnessMember>(json).is_err(),
            "required member {field}"
        );
        let mut value = Value::serialized(member).expect("member CBOR value");
        remove_cbor_field(&mut value, field);
        let mut bytes = Vec::new();
        ciborium::into_writer(&value, &mut bytes).expect("malformed member CBOR");
        assert!(
            ciborium::from_reader::<BridgeWitnessMember, _>(bytes.as_slice()).is_err(),
            "required member {field}"
        );
    }
}

#[cfg(all(feature = "json", feature = "cbor"))]
#[test]
fn declaration_origin_and_scope_fields_never_default_in_json_or_cbor() {
    let kind = BridgeKind::Named {
        module: "App".to_owned(),
        name: "Mode".to_owned(),
        origin: BridgeConstructorOrigin::Global,
    };
    assert_eq!(
        serde_json::to_string(&kind).expect("kind JSON"),
        r#"{"Named":{"module":"App","name":"Mode","origin":"Global"}}"#
    );
    let mut json = serde_json::to_value(&kind).expect("kind JSON");
    json["Named"]
        .as_object_mut()
        .expect("named object")
        .remove("origin");
    assert!(serde_json::from_value::<BridgeKind>(json).is_err());
    let mut cbor = Value::serialized(&kind).expect("kind CBOR");
    remove_cbor_field(&mut cbor, "origin");
    let mut bytes = vec![];
    ciborium::into_writer(&cbor, &mut bytes).expect("malformed kind CBOR");
    assert!(ciborium::from_reader::<BridgeKind, _>(bytes.as_slice()).is_err());
    assert_eq!(
        serde_json::to_string(&BridgeWitnessScope::Within(BridgeWitnessId(0))).expect("scope JSON"),
        r#"{"Within":0}"#
    );
}

#[cfg(all(feature = "json", feature = "cbor"))]
#[test]
fn declaration_captures_rebuild_from_fresh_json_and_cbor_without_external_origins() {
    let plan = captured_alias();
    let json = serde_json::to_string(&plan).expect("captured alias JSON");
    let json_plan: BridgePlan = serde_json::from_str(&json).expect("fresh JSON consumer");
    assert_eq!(json_plan, plan);
    assert_eq!(json_plan.validate_structure(), Ok(()));
    let mut bytes = Vec::new();
    ciborium::into_writer(&plan, &mut bytes).expect("captured alias CBOR");
    let cbor_plan: BridgePlan =
        ciborium::from_reader(bytes.as_slice()).expect("fresh CBOR consumer");
    assert_eq!(cbor_plan.validate_structure(), Ok(()));
    assert_eq!(cbor_plan, plan);
    bytes.clear();
    ciborium::into_writer(&BridgeWitnessScope::Within(BridgeWitnessId(0)), &mut bytes)
        .expect("parent CBOR");
    assert_eq!(bytes, b"\xa1\x66Within\x00");
}
