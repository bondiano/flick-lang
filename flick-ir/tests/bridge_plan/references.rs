#[test]
fn validates_constant_and_rejects_invalid_reference() {
    let mut plan = constant();
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.entries[0].root = BridgeNodeId(7);
    let error = plan
        .validate_structure()
        .expect_err("invalid root must fail");
    assert_eq!(error.path, "entries[0].root");
}

#[test]
fn structural_cycles_are_not_alias_recursion() {
    let mut plan = constant();
    plan.nodes[0].node = BridgeNode::Tuple(vec![BridgeNodeId(0)]);
    assert!(plan.validate_structure().is_err());
}

#[test]
fn alias_recursion_and_forward_structural_references_are_legal() {
    let mut plan = recursive_alias();
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.nodes[0].node = BridgeNode::Tuple(vec![BridgeNodeId(4)]);
    push(&mut plan, 0, BridgeNode::Tuple(vec![]));
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.aliases[0].params.clear();
    assert!(
        plan.validate_structure().is_err(),
        "alias arity/body kind must agree"
    );
}

#[test]
fn rejects_kind_cycles_bad_application_and_duplicate_identities() {
    let mut plan = recursive_alias();
    plan.kinds[1] = BridgeKind::Row(BridgeKindId(1));
    assert_eq!(
        plan.validate_structure().expect_err("kind cycle").path,
        "kinds[1].row"
    );
    let mut plan = recursive_alias();
    plan.nodes[3].node = BridgeNode::App {
        head: BridgeNodeId(0),
        arg: BridgeNodeId(2),
    };
    assert_eq!(
        plan.validate_structure().expect_err("non-arrow head").path,
        "nodes[3].head"
    );
    let mut plan = recursive_alias();
    plan.constructors.push(plan.constructors[0].clone());
    assert_eq!(
        plan.validate_structure()
            .expect_err("duplicate identity")
            .path,
        "constructors[1].identity"
    );
}

#[test]
fn rejects_bad_ids_at_the_actual_table_field() {
    type Mutation = (&'static str, fn(&mut BridgePlan));
    let cases: [Mutation; 6] = [
        ("nodes[0].kind", |p| p.nodes[0].kind = BridgeKindId(99)),
        ("nodes[1].constructor", |p| {
            p.nodes[1].node = BridgeNode::Con(BridgeConstructorId(99));
        }),
        ("constructors[0].meaning", |p| {
            p.constructors[0].meaning = BridgeMeaning::Alias(BridgeAliasId(99));
        }),
        ("aliases[0].body", |p| p.aliases[0].body = BridgeNodeId(99)),
        ("aliases[0].constructor", |p| {
            p.aliases[0].constructor = BridgeConstructorId(99);
        }),
        ("kinds[1].param", |p| {
            p.kinds[1] = BridgeKind::Arrow(BridgeKindId(99), BridgeKindId(0));
        }),
    ];
    for (path, mutate) in cases {
        let mut plan = recursive_alias();
        mutate(&mut plan);
        assert_eq!(plan.validate_structure().expect_err(path).path, path);
    }
}

#[test]
fn forall_masks_exact_names_in_constraints_and_body_but_not_other_names() {
    let mut plan = constant();
    plan.kinds
        .extend([BridgeKind::Constraint, BridgeKind::Symbol]);
    let constraint = push(&mut plan, 1, BridgeNode::Var("c".to_owned()));
    let value = push(&mut plan, 0, BridgeNode::Var("a".to_owned()));
    let scheme = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("a", 0), binder("c", 1)],
            constraints: vec![constraint],
            body: value,
        },
    );
    plan.entries[0].root = scheme;
    // Different outer kind is legal: the inner exact-name binding shadows it.
    plan.entries[0].free.push(binder("a", 2));
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.nodes[2].node = BridgeNode::Var("a$Dict".to_owned());
    assert!(
        plan.validate_structure()
            .expect_err("literal suffixed name is not a")
            .message
            .contains("a$Dict")
    );
    plan.entries[0].free.push(binder("a$Dict", 0));
    assert_eq!(plan.validate_structure(), Ok(()));
    if let BridgeNode::Forall { vars, .. } = &mut plan.nodes[3].node {
        vars.push(binder("a", 0));
    }
    assert_eq!(
        plan.validate_structure()
            .expect_err("duplicate binder")
            .path,
        "nodes[3].vars[2].name"
    );
}

#[test]
fn rejects_unbound_wrong_kind_and_overlapping_alias_binders() {
    let mut plan = recursive_alias();
    plan.aliases[0].params[0].name = "b".to_owned();
    assert!(
        plan.validate_structure()
            .expect_err("unbound a")
            .message
            .contains("unbound variable")
    );
    let mut plan = recursive_alias();
    plan.aliases[0].free.push(binder("a", 0));
    assert_eq!(
        plan.validate_structure().expect_err("overlap").path,
        "aliases[0].free"
    );
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Symbol);
    plan.nodes[0].node = BridgeNode::Var("a".to_owned());
    plan.entries[0].free.push(binder("a", 1));
    assert_eq!(
        plan.validate_structure().expect_err("wrong kind").path,
        "entries[0].root"
    );
}

#[test]
fn rigid_ids_are_literal_not_table_references_and_keep_consistent_kinds() {
    let mut plan = constant();
    plan.nodes[0].node = BridgeNode::Rigid(BridgeRigidId(u32::MAX));
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.kinds.push(BridgeKind::Symbol);
    push(&mut plan, 1, BridgeNode::Rigid(BridgeRigidId(u32::MAX)));
    assert_eq!(
        plan.validate_structure()
            .expect_err("same literal cannot change kind")
            .path,
        "nodes[1].kind"
    );
}

#[test]
fn rich_raw_graph_preserves_all_resolved_node_information() {
    let plan = rich_graph();
    assert_eq!(plan.validate_structure(), Ok(()));
    assert!(
        plan.raw_type_eq(BridgeNodeId(0), BridgeNodeId(0))
            .expect("valid equality")
    );
    let mut plan = plan;
    let root = plan.entries[0].root;
    let BridgeNode::Exists { witness, .. } = plan.nodes[root.0 as usize].node else {
        panic!("fixture root is existential");
    };
    plan.witnesses[witness.0 as usize].contract = BridgeNodeId(0);
    assert!(
        plan.validate_structure().is_err(),
        "existential contract must have Module kind"
    );
}

#[test]
fn schema_names_every_existing_opaque_or_projected_builtin_role() {
    // Deliberately exhaustive: a new role must extend this catalogue even
    // when neither optional wire format is enabled.
    fn wire_name(role: BridgeBuiltin) -> &'static str {
        match role {
            BridgeBuiltin::Eff => "Eff",
            BridgeBuiltin::List => "List",
            BridgeBuiltin::Option => "Option",
            BridgeBuiltin::Result => "Result",
            BridgeBuiltin::Layer => "Layer",
            BridgeBuiltin::Schema => "Schema",
            BridgeBuiltin::PureCodec => "PureCodec",
            BridgeBuiltin::Codec => "Codec",
            BridgeBuiltin::Fiber => "Fiber",
            BridgeBuiltin::Duration => "Duration",
            BridgeBuiltin::Schedule => "Schedule",
            BridgeBuiltin::Chunk => "Chunk",
            BridgeBuiltin::NonEmptyChunk => "NonEmptyChunk",
            BridgeBuiltin::HashMap => "HashMap",
            BridgeBuiltin::HashSet => "HashSet",
            BridgeBuiltin::Handlers => "Handlers",
            BridgeBuiltin::Exit => "Exit",
            BridgeBuiltin::Failure => "Failure",
            BridgeBuiltin::Errors => "Errors",
            BridgeBuiltin::Field => "Field",
            BridgeBuiltin::ServiceImpl => "ServiceImpl",
        }
    }
    let roles = [
        BridgeBuiltin::Eff,
        BridgeBuiltin::List,
        BridgeBuiltin::Option,
        BridgeBuiltin::Result,
        BridgeBuiltin::Layer,
        BridgeBuiltin::Schema,
        BridgeBuiltin::PureCodec,
        BridgeBuiltin::Codec,
        BridgeBuiltin::Fiber,
        BridgeBuiltin::Duration,
        BridgeBuiltin::Schedule,
        BridgeBuiltin::Chunk,
        BridgeBuiltin::NonEmptyChunk,
        BridgeBuiltin::HashMap,
        BridgeBuiltin::HashSet,
        BridgeBuiltin::Handlers,
        BridgeBuiltin::Exit,
        BridgeBuiltin::Failure,
        BridgeBuiltin::Errors,
        BridgeBuiltin::Field,
        BridgeBuiltin::ServiceImpl,
    ];
    let names = roles.map(wire_name);
    assert_eq!(
        names,
        [
            "Eff",
            "List",
            "Option",
            "Result",
            "Layer",
            "Schema",
            "PureCodec",
            "Codec",
            "Fiber",
            "Duration",
            "Schedule",
            "Chunk",
            "NonEmptyChunk",
            "HashMap",
            "HashSet",
            "Handlers",
            "Exit",
            "Failure",
            "Errors",
            "Field",
            "ServiceImpl",
        ]
    );
    #[cfg(any(feature = "json", feature = "cbor"))]
    for (role, name) in roles.iter().zip(names) {
        #[cfg(feature = "json")]
        {
            let encoded = serde_json::to_string(role).expect("builtin role JSON");
            assert_eq!(encoded, format!("\"{name}\""));
            let decoded: BridgeBuiltin =
                serde_json::from_str(&encoded).expect("builtin role decode");
            assert_eq!(&decoded, role);
        }
        #[cfg(feature = "cbor")]
        {
            let mut encoded = Vec::new();
            ciborium::into_writer(role, &mut encoded).expect("builtin role CBOR");
            let actual: Value = ciborium::from_reader(encoded.as_slice()).expect("CBOR wire value");
            assert_eq!(actual, Value::Text(name.to_owned()));
            let decoded: BridgeBuiltin =
                ciborium::from_reader(encoded.as_slice()).expect("builtin role decode");
            assert_eq!(&decoded, role);
        }
    }
}

#[test]
fn service_impl_links_need_a_record_alias_not_an_arbitrary_recursive_target() {
    let mut plan = recursive_alias();
    plan.constructors[0].service_impl = Some(BridgeAliasId(0));
    assert_eq!(
        plan.validate_structure()
            .expect_err("recursive app is not generated record")
            .path,
        "constructors[0].service_impl"
    );
    let body = push(&mut plan, 0, BridgeNode::Record(row(vec![])));
    plan.aliases[0].body = body;
    assert_eq!(plan.validate_structure(), Ok(()));
    // The accepted structural link still needs extraction's provenance proof;
    // this validator does not certify it as an actual compiler-generated Impl.
}
