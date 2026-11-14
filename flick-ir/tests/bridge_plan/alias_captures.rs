#[test]
fn transparent_witness_higher_kind_alias_has_a_real_named_head() {
    let plan = witness_manifest_alias();
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn witness_alias_observes_outer_witness_in_its_declaration_not_a_global_root() {
    let mut plan = single_witness();
    let inner = exists(&mut plan, "Inner", BridgeNodeId(1), BridgeNodeId(0));
    plan.witnesses[1].scope = BridgeWitnessScope::Within(BridgeWitnessId(0));
    let alias = manifest(
        &mut plan,
        1,
        "OuterItem",
        0,
        vec![],
        vec![],
        BridgeNodeId(3),
    );
    plan.nodes[inner.0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(1),
        body: alias,
    };
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: inner,
    };
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.witnesses[1].scope = BridgeWitnessScope::Root;
    assert_eq!(
        plan.validate_structure().expect_err("wrong parent").path,
        "witnesses[1].scope"
    );
    plan.witnesses[1].scope = BridgeWitnessScope::Within(BridgeWitnessId(1));
    assert!(
        plan.validate_structure()
            .expect_err("parent cycle")
            .message
            .contains("cycle")
    );
}

#[test]
fn alias_free_capture_uses_before_owner_bindings_despite_caller_shadow() {
    let plan = captured_alias();
    assert_eq!(plan.validate_structure(), Ok(()));
    let mut missing = plan.clone();
    missing.aliases[0].free[0].name = "missing".to_owned();
    assert_eq!(
        missing
            .validate_structure()
            .expect_err("missing capture")
            .path,
        "aliases[0].free[0]"
    );
    let mut wrong = plan;
    wrong.aliases[0].free[0].kind = BridgeKindId(2);
    assert_eq!(
        wrong
            .validate_structure()
            .expect_err("wrong capture kind")
            .path,
        "aliases[0].free[0].kind"
    );
}

#[test]
fn witness_alias_formals_shadow_outer_names_without_capturing_them() {
    let mut plan = witness_manifest_alias();
    plan.kinds.push(BridgeKind::Symbol);
    let root = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("a", 3)],
            constraints: vec![],
            body: BridgeNodeId(2),
        },
    );
    plan.entries[0].root = root;
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.aliases[0].free.push(binder("a", 3));
    assert_eq!(
        plan.validate_structure()
            .expect_err("formal/free overlap")
            .path,
        "aliases[0].free"
    );
}

#[test]
fn global_identity_may_receive_a_witness_descriptor_but_not_capture_it() {
    let mut plan = single_witness();
    plan.kinds
        .push(BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(0)));
    let parameter = push(&mut plan, 0, BridgeNode::Var("a".to_owned()));
    let identity = add_global_alias(&mut plan, parameter);
    let BridgeNode::Con(ctor) = plan.nodes[identity.0 as usize].node else {
        unreachable!()
    };
    plan.constructors[ctor.0 as usize].kind = BridgeKindId(2);
    plan.nodes[identity.0 as usize].kind = BridgeKindId(2);
    plan.aliases[0].params.push(binder("a", 0));
    let result = push(
        &mut plan,
        0,
        BridgeNode::App {
            head: identity,
            arg: BridgeNodeId(3),
        },
    );
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: result,
    };
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.aliases[0].body = BridgeNodeId(3);
    assert_eq!(
        plan.validate_structure()
            .expect_err("direct caller capture")
            .path,
        "aliases[0].body"
    );
}

#[test]
fn alias_owning_exists_is_one_static_body_at_multiple_argument_shapes() {
    let mut plan = single_witness();
    plan.kinds
        .push(BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(0)));
    let parameter = push(&mut plan, 0, BridgeNode::Var("a".to_owned()));
    let pair = push(
        &mut plan,
        0,
        BridgeNode::Tuple(vec![parameter, BridgeNodeId(3)]),
    );
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: pair,
    };
    let alias = add_global_alias(&mut plan, BridgeNodeId(2));
    let BridgeNode::Con(ctor) = plan.nodes[alias.0 as usize].node else {
        unreachable!()
    };
    plan.constructors[ctor.0 as usize].kind = BridgeKindId(2);
    plan.nodes[alias.0 as usize].kind = BridgeKindId(2);
    plan.aliases[0].params.push(binder("a", 0));
    let first = push(
        &mut plan,
        0,
        BridgeNode::App {
            head: alias,
            arg: BridgeNodeId(0),
        },
    );
    let other = push(&mut plan, 0, BridgeNode::Record(row(vec![])));
    let second = push(
        &mut plan,
        0,
        BridgeNode::App {
            head: alias,
            arg: other,
        },
    );
    let root = push(&mut plan, 0, BridgeNode::Tuple(vec![first, second]));
    plan.entries[0].root = root;
    assert_eq!(plan.aliases.len(), 1);
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn one_owner_cannot_choose_between_two_same_spelled_forall_contexts() {
    let mut plan = single_witness();
    let left = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("a", 0)],
            constraints: vec![],
            body: BridgeNodeId(2),
        },
    );
    let right = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("a", 0)],
            constraints: vec![],
            body: BridgeNodeId(2),
        },
    );
    let root = push(&mut plan, 0, BridgeNode::Tuple(vec![left, right]));
    plan.entries[0].root = root;
    assert_eq!(
        plan.validate_structure()
            .expect_err("ambiguous owner capture")
            .path,
        "nodes[2].witness"
    );
}

#[test]
fn underapplied_witness_alias_can_be_passed_through_a_higher_kind_parameter() {
    let mut plan = witness_manifest_alias();
    let f = push(&mut plan, 2, BridgeNode::Var("f".to_owned()));
    let a = push(&mut plan, 0, BridgeNode::Var("a".to_owned()));
    let body = push(&mut plan, 0, BridgeNode::App { head: f, arg: a });
    let forward = add_global_alias(&mut plan, body);
    let higher = BridgeKindId(u32::try_from(plan.kinds.len()).expect("small kinds"));
    plan.kinds
        .push(BridgeKind::Arrow(BridgeKindId(2), BridgeKindId(2)));
    let BridgeNode::Con(ctor) = plan.nodes[forward.0 as usize].node else {
        unreachable!()
    };
    plan.constructors[ctor.0 as usize].kind = higher;
    plan.nodes[forward.0 as usize].kind = higher;
    plan.aliases[1].params = vec![binder("f", 2), binder("a", 0)];
    let partial = push(
        &mut plan,
        2,
        BridgeNode::App {
            head: forward,
            arg: BridgeNodeId(3),
        },
    );
    let complete = push(
        &mut plan,
        0,
        BridgeNode::App {
            head: partial,
            arg: BridgeNodeId(0),
        },
    );
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: complete,
    };
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.aliases[0].params[0].kind = BridgeKindId(1);
    assert_eq!(
        plan.validate_structure()
            .expect_err("wrong alias parameter kind")
            .path,
        "aliases[0].params[0].kind"
    );
}

#[test]
fn unresolved_alias_ownership_does_not_bootstrap_a_detached_environment() {
    let mut plan = single_witness();
    // The sole owner sits inside an alias declared BY that same witness.
    // A detached-root fallback would invent the missing declaration frame.
    let _ = manifest(
        &mut plan,
        0,
        "CircularDeclaration",
        0,
        vec![],
        vec![],
        BridgeNodeId(2),
    );
    plan.entries[0].root = BridgeNodeId(0);
    assert_eq!(
        plan.validate_structure()
            .expect_err("circular declaration capture")
            .path,
        "witnesses[0].scope"
    );
}
