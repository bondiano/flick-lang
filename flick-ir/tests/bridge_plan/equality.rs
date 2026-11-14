#[test]
fn held_role_is_raw_and_alias_roles_are_not_normalized() {
    let mut plan = recursive_alias();
    assert_eq!(
        plan.constructors[0].dictionary_role,
        BridgeDictionaryRole::Passed
    );
    plan.constructors[0].identity.module = "$held".to_owned();
    assert_eq!(
        plan.validate_structure().expect_err("held namespace").path,
        "constructors[0].dictionary_role"
    );
    plan.constructors[0].dictionary_role = BridgeDictionaryRole::Held;
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn wire_equality_retains_direct_but_raw_type_eq_ignores_only_direct() {
    let mut plan = constant();
    plan.kinds
        .extend([BridgeKind::ServiceRow, BridgeKind::ErrorRow]);
    let services = push(&mut plan, 1, BridgeNode::Row(row(vec![])));
    let errors = push(&mut plan, 2, BridgeNode::Row(row(vec![])));
    let effects = BridgeEffects {
        services,
        errors,
        sync: false,
        native_direct: false,
    };
    let mut direct = effects.clone();
    direct.native_direct = true;
    assert_ne!(effects, direct);
    let left = push(
        &mut plan,
        0,
        BridgeNode::Arrow {
            param: BridgeNodeId(0),
            effects: Some(effects),
            result: BridgeNodeId(0),
        },
    );
    let right = push(
        &mut plan,
        0,
        BridgeNode::Arrow {
            param: BridgeNodeId(0),
            effects: Some(direct),
            result: BridgeNodeId(0),
        },
    );
    assert!(plan.raw_type_eq(left, right).expect("valid arrows"));
    if let BridgeNode::Arrow {
        effects: Some(e), ..
    } = &mut plan.nodes[right.0 as usize].node
    {
        e.sync = true;
    }
    assert!(
        !plan
            .raw_type_eq(left, right)
            .expect("sync is raw type identity")
    );
}

#[test]
fn raw_equality_retains_literal_names_and_does_not_unfold_aliases() {
    let mut plan = recursive_alias();
    let other = push(&mut plan, 0, BridgeNode::Var("b".to_owned()));
    assert!(
        !plan
            .raw_type_eq(BridgeNodeId(2), other)
            .expect("literal vars")
    );
    assert!(
        !plan
            .raw_type_eq(BridgeNodeId(0), BridgeNodeId(3))
            .expect("raw alias application")
    );
    let mut scheme = |name: &str| {
        let body = push(&mut plan, 0, BridgeNode::Var(name.to_owned()));
        push(
            &mut plan,
            0,
            BridgeNode::Forall {
                vars: vec![binder(name, 0)],
                constraints: vec![],
                body,
            },
        )
    };
    let left = scheme("left");
    let right = scheme("right");
    assert!(!plan.raw_type_eq(left, right).expect("no alpha equivalence"));
}

#[test]
fn raw_equality_keeps_capture_value_and_compares_duplicate_kind_table_entries() {
    let mut plan = constant();
    plan.kinds
        .extend([BridgeKind::CaptureRow, BridgeKind::Type]);
    let captures = push(&mut plan, 1, BridgeNode::Row(row(vec![])));
    let left = push(
        &mut plan,
        0,
        BridgeNode::Captured {
            ty: BridgeNodeId(0),
            captures,
            value: true,
        },
    );
    let right = push(
        &mut plan,
        2,
        BridgeNode::Captured {
            ty: BridgeNodeId(0),
            captures,
            value: true,
        },
    );
    assert!(
        plan.raw_type_eq(left, right)
            .expect("duplicate kind entries are equal")
    );
    if let BridgeNode::Captured { value, .. } = &mut plan.nodes[right.0 as usize].node {
        *value = false;
    }
    assert!(
        !plan
            .raw_type_eq(left, right)
            .expect("value flag is part of raw identity")
    );
}

#[test]
fn raw_literal_var_equality_ignores_implicit_kinds_but_not_explicit_binders() {
    let mut plan = constant();
    plan.kinds
        .push(BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(0)));
    let first_member = push(&mut plan, 0, BridgeNode::Var("f".to_owned()));
    let higher = push(&mut plan, 1, BridgeNode::Var("f".to_owned()));
    let left = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("f", 0)],
            constraints: vec![],
            body: first_member,
        },
    );
    let right = push(
        &mut plan,
        1,
        BridgeNode::Forall {
            vars: vec![binder("f", 1)],
            constraints: vec![],
            body: higher,
        },
    );
    assert_eq!(plan.validate_structure(), Ok(()));
    assert!(
        plan.raw_type_eq(first_member, higher)
            .expect("core compares literal Var spelling")
    );
    assert!(
        !plan
            .raw_type_eq(left, right)
            .expect("explicit binder kinds remain structure")
    );
}

#[test]
fn raw_exists_equality_uses_literal_binder_contract_body_not_registries() {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Module);
    let contract = push(&mut plan, 1, BridgeNode::Rigid(BridgeRigidId(1)));
    let left = exists(&mut plan, "K", contract, BridgeNodeId(0));
    let right = exists(&mut plan, "K", contract, BridgeNodeId(0));
    plan.witnesses[1].members.push(BridgeWitnessMember {
        owner_path: vec![],
        name: "Unused".to_owned(),
        kind: BridgeKindId(1),
    });
    assert!(
        plan.raw_type_eq(left, right)
            .expect("different witness IDs/registries are not raw identity")
    );
    plan.witnesses[1].binder = "Other".to_owned();
    assert!(!plan.raw_type_eq(left, right).expect("no alpha renaming"));
    plan.witnesses[1].binder = "K".to_owned();
    plan.witnesses[1].contract = push(&mut plan, 1, BridgeNode::Rigid(BridgeRigidId(2)));
    assert!(!plan.raw_type_eq(left, right).expect("raw contracts matter"));
}

#[test]
fn raw_explicit_kind_equality_ignores_origin_but_validation_does_not() {
    let mut plan = both_exists();
    let left_kind = named_witness_kind(&mut plan, 0);
    let right_kind = named_witness_kind(&mut plan, 1);
    let left = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("a", left_kind)],
            constraints: vec![],
            body: BridgeNodeId(0),
        },
    );
    let right = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("a", right_kind)],
            constraints: vec![],
            body: BridgeNodeId(0),
        },
    );
    plan.nodes[7].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: left,
    };
    plan.nodes[8].node = BridgeNode::Exists {
        witness: BridgeWitnessId(1),
        body: right,
    };
    assert_eq!(plan.validate_structure(), Ok(()));
    assert!(plan.raw_type_eq(left, right).expect("raw explicit kinds"));
    if let BridgeNode::Forall { vars, .. } = &mut plan.nodes[right.0 as usize].node {
        vars[0].kind = BridgeKindId(left_kind);
    }
    assert!(
        plan.validate_structure().is_err(),
        "same literal kind with inaccessible origin is not semantic equality"
    );
}
