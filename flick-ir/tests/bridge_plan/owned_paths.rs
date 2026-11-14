#[test]
fn owned_paths_distinguish_children_compound_terminals_and_foreign_reexports() {
    let mut plan = single_witness();
    let a = member_at(&mut plan, &["ChildA"], "Key");
    let b = member_at(&mut plan, &["ChildB"], "Key");
    let compound = member_at(&mut plan, &[], "Read.Impl");
    let nested = member_at(&mut plan, &["Read"], "Impl");
    let body = push(
        &mut plan,
        0,
        BridgeNode::Tuple(vec![a, b, compound, nested]),
    );
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body,
    };
    assert_eq!(plan.validate_structure(), Ok(()));
    let mut duplicate = plan.clone();
    let repeated = duplicate.witnesses[0].members[1].clone();
    duplicate.witnesses[0].members.push(repeated);
    assert!(
        duplicate
            .validate_structure()
            .expect_err("duplicate full path")
            .message
            .contains("duplicate")
    );
    let mut collision = plan.clone();
    collision.witnesses[0].members.push(BridgeWitnessMember {
        owner_path: vec!["ChildA".to_owned()],
        name: "Key".to_owned(),
        kind: BridgeKindId(0),
    });
    assert!(collision.validate_structure().is_err());
    let BridgeNode::Con(id) = plan.nodes[a.0 as usize].node else {
        unreachable!()
    };
    plan.constructors[id.0 as usize].origin = BridgeConstructorOrigin::Global;
    assert_eq!(
        plan.validate_structure()
            .expect_err("witness module cannot claim global origin")
            .path,
        format!("constructors[{}].origin", id.0)
    );
}

#[test]
fn encoded_module_components_cannot_smuggle_multiple_ownership_edges() {
    let mut plan = single_witness();
    let _ = member_at(&mut plan, &["A.B"], "Key");
    let _ = member_at(&mut plan, &["A", "B"], "Key");
    let failure = plan
        .validate_structure()
        .expect_err("non-atomic ownership edge");
    assert_eq!(failure.path, "witnesses[0].members[1].owner_path");
    assert!(failure.message.contains("atomic"));
}

#[test]
fn declaration_boundary_global_service_cannot_capture_a_witness_alias() {
    let mut plan = single_witness();
    let record = push(&mut plan, 0, BridgeNode::Record(row(vec![])));
    let implementation = manifest(&mut plan, 0, "Read.Impl", 0, vec![], vec![], record);
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: implementation,
    };
    plan.kinds.push(BridgeKind::Service);
    let service = plan.constructors.len();
    plan.constructors.push(BridgeConstructor {
        identity: TypeName {
            module: "App".to_owned(),
            name: "Service".to_owned(),
        },
        origin: BridgeConstructorOrigin::Global,
        kind: BridgeKindId(2),
        meaning: BridgeMeaning::Nominal,
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    assert_eq!(
        plan.validate_structure(),
        Ok(()),
        "ordinary witness record aliases remain valid"
    );
    let mut global = plan.clone();
    let _ = add_global_alias(&mut global, record);
    global.constructors[service].service_impl = Some(BridgeAliasId(1));
    assert_eq!(
        global.validate_structure(),
        Ok(()),
        "global record alias links retain their existing structural contract"
    );
    plan.constructors[service].service_impl = Some(BridgeAliasId(0));
    assert_eq!(
        plan.validate_structure()
            .expect_err(
                "a global constructor cannot capture a witness declaration via service_impl"
            )
            .path,
        format!("constructors[{service}].service_impl")
    );
}

#[test]
fn declaration_boundary_owned_module_components_use_the_atomic_compiler_grammar() {
    for component in ["Child", "_Private", "Child9", "Child$Generated", "Child$1"] {
        let mut plan = single_witness();
        let member = member_at(&mut plan, &[component], "Read.Impl");
        plan.nodes[2].node = BridgeNode::Exists {
            witness: BridgeWitnessId(0),
            body: member,
        };
        assert_eq!(
            plan.validate_structure(),
            Ok(()),
            "valid atomic edge {component:?}; terminal dots are not path separators"
        );
    }
    let invalid = [
        "",
        "A.B",
        "../Child",
        "Child name",
        "Child$",
        "Child$$X",
        "$Child",
        "9Child",
        "Chíld",
    ];
    let mut accepted = Vec::new();
    for component in invalid {
        let mut plan = single_witness();
        let member = member_at(&mut plan, &[component], "Read.Impl");
        plan.nodes[2].node = BridgeNode::Exists {
            witness: BridgeWitnessId(0),
            body: member,
        };
        match plan.validate_structure() {
            Ok(()) => accepted.push(component),
            Err(error) => assert_eq!(
                error.path, "witnesses[0].members[1].owner_path",
                "{component:?}"
            ),
        }
    }
    assert!(
        accepted.is_empty(),
        "non-atomic path edges accepted: {accepted:?}"
    );
}
