fn nonmonotone_kind_forest() -> (BridgePlan, [BridgeNodeId; 3], [u32; 3], u32) {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Module);
    let contract = push(&mut plan, 1, BridgeNode::Rigid(BridgeRigidId(39)));
    let owners = ["A", "B", "C"].map(|name| exists(&mut plan, name, contract, BridgeNodeId(0)));
    // Lexical ancestry is 2 -> 0 -> 1, deliberately not numeric ID order.
    plan.witnesses[0].scope = BridgeWitnessScope::Within(BridgeWitnessId(2));
    plan.witnesses[1].scope = BridgeWitnessScope::Within(BridgeWitnessId(0));
    plan.nodes[owners[2].0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(2),
        body: owners[0],
    };
    plan.nodes[owners[0].0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: owners[1],
    };
    let modes = [0, 1, 2].map(|id| named_witness_kind(&mut plan, id));
    let ab = u32::try_from(plan.kinds.len()).expect("fixture IDs");
    plan.kinds.push(BridgeKind::Arrow(
        BridgeKindId(modes[0]),
        BridgeKindId(modes[1]),
    ));
    let all = u32::try_from(plan.kinds.len()).expect("fixture IDs");
    plan.kinds
        .push(BridgeKind::Arrow(BridgeKindId(ab), BridgeKindId(modes[2])));
    plan.witnesses[1].members.push(BridgeWitnessMember {
        owner_path: vec![],
        name: "Fn".into(),
        kind: BridgeKindId(all),
    });
    plan.entries[0].root = owners[2];
    (plan, owners, modes, all)
}

#[test]
fn named_support_uses_forest_ancestry_and_reports_smallest_missing_id() {
    let (plan, _, modes, all) = nonmonotone_kind_forest();
    assert_eq!(plan.validate_structure(), Ok(()));
    let mut outside = plan.clone();
    outside.entries[0].free.push(binder("f", all));
    let error = outside
        .validate_structure()
        .expect_err("no witnesses exist before the entry");
    assert_eq!(error.path, "entries[0].free[0].kind");
    assert_eq!(
        error.message, "unbound kind witness BridgeWitnessId(0)",
        "not deepest ID1 or outer ID2"
    );

    let mut future = plan;
    future.witnesses[2].members.push(BridgeWitnessMember {
        owner_path: vec![],
        name: "Future".into(),
        kind: BridgeKindId(modes[1]),
    });
    let error = future
        .validate_structure()
        .expect_err("an outer member cannot use a future descendant");
    assert_eq!(error.path, "witnesses[2].members[1].kind");
    assert_eq!(error.message, "unbound kind witness BridgeWitnessId(1)");
}

#[test]
fn named_ancestry_remains_distinct_from_literal_witness_shadowing() {
    let (mut plan, owners, _, _) = nonmonotone_kind_forest();
    for witness in &mut plan.witnesses {
        witness.binder = "K".into();
    }
    for constructor in &mut plan.constructors {
        constructor.identity.module = "$exists:K".into();
    }
    for kind in &mut plan.kinds {
        if let BridgeKind::Named { module, .. } = kind {
            *module = "$exists:K".into();
        }
    }
    assert_eq!(
        plan.validate_structure(),
        Ok(()),
        "explicit kind origins keep captured ancestors"
    );
    let outer_mode = plan
        .nodes
        .iter()
        .enumerate()
        .find_map(|(id, node)| {
            let BridgeNode::Con(constructor) = node.node else {
                return None;
            };
            (plan.constructors[constructor.0 as usize].origin
                == BridgeConstructorOrigin::Witness(BridgeWitnessId(2)))
            .then_some(BridgeNodeId(u32::try_from(id).expect("fixture IDs")))
        })
        .expect("outer Mode constructor");
    plan.nodes[owners[1].0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(1),
        body: outer_mode,
    };
    let error = plan
        .validate_structure()
        .expect_err("literal constructor still crosses a name shadow barrier");
    assert_eq!(error.path, format!("nodes[{}].body", owners[1].0));
    assert_eq!(
        error.message,
        "different witness crosses a same-name shadow barrier"
    );
}

fn unused_branched_supports() -> BridgePlan {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Module);
    let contract = push(&mut plan, 1, BridgeNode::Rigid(BridgeRigidId(40)));
    let mut previous = BridgeKindId(0);
    let mut bodies = vec![];
    for i in 0..3 {
        let binder = format!("K{i}");
        let body = exists(&mut plan, &binder, contract, BridgeNodeId(0));
        plan.witnesses[i].members.push(BridgeWitnessMember {
            owner_path: vec![],
            name: "Mode".into(),
            kind: BridgeKindId(0),
        });
        let named = BridgeKindId(u32::try_from(plan.kinds.len()).expect("fixture IDs"));
        plan.kinds.push(BridgeKind::Named {
            module: format!("$exists:{binder}"),
            name: "Mode".into(),
            origin: BridgeConstructorOrigin::Witness(BridgeWitnessId(
                u32::try_from(i).expect("fixture IDs"),
            )),
        });
        let prefix = BridgeKindId(u32::try_from(plan.kinds.len()).expect("fixture IDs"));
        plan.kinds.push(BridgeKind::Arrow(previous, named));
        previous = prefix;
        bodies.push(body);
    }
    plan.entries[0].root = push(&mut plan, 0, BridgeNode::Tuple(bodies));
    plan
}

#[test]
fn unused_branched_supports_are_valid_but_their_origins_are_checked() {
    let plan = unused_branched_supports();
    assert_eq!(plan.validate_structure(), Ok(()));
    let mut missing = plan.clone();
    missing.witnesses[0].members.clear();
    let error = missing
        .validate_structure()
        .expect_err("unused named origin still requires its registry member");
    assert_eq!(error.path, "kinds[2].origin");
    assert_eq!(error.message, "undeclared witness named kind");
    let mut reserved = plan.clone();
    if let BridgeKind::Named { origin, .. } = &mut reserved.kinds[2] {
        *origin = BridgeConstructorOrigin::Global;
    }
    let error = reserved
        .validate_structure()
        .expect_err("reserved namespace needs witness provenance even when unused");
    assert_eq!(error.path, "kinds[2].origin");
    assert_eq!(
        error.message,
        "reserved existential module requires witness origin"
    );
    let mut bad_id = plan;
    if let BridgeKind::Named { origin, .. } = &mut bad_id.kinds[2] {
        *origin = BridgeConstructorOrigin::Witness(BridgeWitnessId(u32::MAX));
    }
    let error = bad_id
        .validate_structure()
        .expect_err("unused origin is still bounds checked");
    assert_eq!(error.path, "kinds[2].origin");
    assert_eq!(error.message, "reference out of range");
    bad_id.nodes[0].kind = BridgeKindId(u32::MAX);
    let error = bad_id
        .validate_structure()
        .expect_err("structural phase precedes support construction");
    assert_eq!(error.path, "nodes[0].kind");
}
