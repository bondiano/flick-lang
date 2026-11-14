#[test]
fn named_kind_provenance_is_scoped_in_forall_entries_and_unused_declarations() {
    let mut plan = single_witness();
    let mode = named_witness_kind(&mut plan, 0);
    let body = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("m", mode)],
            constraints: vec![],
            body: BridgeNodeId(0),
        },
    );
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body,
    };
    assert_eq!(plan.validate_structure(), Ok(()));
    let mut outside = plan.clone();
    outside.entries[0].free.push(binder("m", mode));
    assert_eq!(
        outside
            .validate_structure()
            .expect_err("entry binder before witness")
            .path,
        "entries[0].free[0].kind"
    );
    let mut global = plan.clone();
    global.constructors.push(BridgeConstructor {
        identity: TypeName {
            module: "App".to_owned(),
            name: "Fast".to_owned(),
        },
        origin: BridgeConstructorOrigin::Global,
        kind: BridgeKindId(mode),
        meaning: BridgeMeaning::Nominal,
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    assert!(
        global
            .validate_structure()
            .expect_err("unused global kind capture")
            .path
            .starts_with("constructors[")
    );
    let mut bad_origin = plan;
    if let BridgeKind::Named { origin, .. } = &mut bad_origin.kinds[mode as usize] {
        *origin = BridgeConstructorOrigin::Global;
    }
    assert_eq!(
        bad_origin
            .validate_structure()
            .expect_err("reserved named kind origin")
            .path,
        format!("kinds[{mode}].origin")
    );
}

#[test]
fn semantic_named_kinds_keep_captured_ancestry_beneath_a_literal_name_shadow() {
    let mut plan = single_witness();
    let outer_mode = named_witness_kind(&mut plan, 0);
    let inner = exists(&mut plan, "K", BridgeNodeId(1), BridgeNodeId(0));
    plan.witnesses[1].scope = BridgeWitnessScope::Within(BridgeWitnessId(0));
    let inner_mode = named_witness_kind(&mut plan, 1);
    let arrow = u32::try_from(plan.kinds.len()).expect("small kinds");
    plan.kinds
        .push(BridgeKind::Arrow(BridgeKindId(outer_mode), BridgeKindId(0)));
    let f = push(&mut plan, arrow, BridgeNode::Var("f".to_owned()));
    let x = push(&mut plan, outer_mode, BridgeNode::Var("x".to_owned()));
    let application = push(&mut plan, 0, BridgeNode::App { head: f, arg: x });
    plan.nodes[inner.0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(1),
        body: application,
    };
    let bind = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("f", arrow), binder("x", outer_mode)],
            constraints: vec![],
            body: inner,
        },
    );
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: bind,
    };
    assert_eq!(
        plan.validate_structure(),
        Ok(()),
        "semantic captured kinds do not re-resolve printed K"
    );
    let mut wrong_kind = plan.clone();
    wrong_kind.nodes[x.0 as usize].kind = BridgeKindId(inner_mode);
    assert_eq!(
        wrong_kind
            .validate_structure()
            .expect_err("both origins accessible but semantically different")
            .path,
        format!("nodes[{}].arg", application.0)
    );
    let mut wrong_binding = plan.clone();
    let inner_arrow = u32::try_from(wrong_binding.kinds.len()).expect("small kinds");
    wrong_binding
        .kinds
        .push(BridgeKind::Arrow(BridgeKindId(inner_mode), BridgeKindId(0)));
    wrong_binding.nodes[f.0 as usize].kind = BridgeKindId(inner_arrow);
    wrong_binding.nodes[x.0 as usize].kind = BridgeKindId(inner_mode);
    assert_eq!(
        wrong_binding
            .validate_structure()
            .expect_err("kind must match saved f/x binders")
            .path,
        format!("nodes[{}].vars", bind.0)
    );
    plan.nodes[inner.0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(1),
        body: BridgeNodeId(3),
    };
    assert!(
        plan.validate_structure()
            .expect_err("literal outer Con still crosses a barrier")
            .message
            .contains("shadow")
    );
}

#[test]
fn kind_metadata_cannot_see_a_future_descendant_or_its_own_contract_witness() {
    let mut plan = single_witness();
    let mode = named_witness_kind(&mut plan, 0);
    let contract = push(
        &mut plan,
        1,
        BridgeNode::Forall {
            vars: vec![binder("a", mode)],
            constraints: vec![],
            body: BridgeNodeId(1),
        },
    );
    plan.witnesses[0].contract = contract;
    assert!(
        plan.validate_structure()
            .expect_err("contract kind cannot see new witness")
            .message
            .contains("witness")
    );
    let mut plan = single_witness();
    let inner = exists(&mut plan, "Inner", BridgeNodeId(1), BridgeNodeId(0));
    plan.witnesses[1].scope = BridgeWitnessScope::Within(BridgeWitnessId(0));
    let inner_mode = named_witness_kind(&mut plan, 1);
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: inner,
    };
    // Keep the unused constructor/node intrinsically well-kinded while giving
    // the OUTER declaration a kind only available inside its future descendant.
    plan.witnesses[0].members[0].kind = BridgeKindId(inner_mode);
    plan.constructors[0].kind = BridgeKindId(inner_mode);
    plan.nodes[3].kind = BridgeKindId(inner_mode);
    assert_eq!(
        plan.validate_structure()
            .expect_err("unused member metadata checked in owner frame")
            .path,
        "witnesses[0].members[0].kind"
    );
}
