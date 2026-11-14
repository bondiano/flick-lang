#[test]
fn validates_fifty_thousand_structural_nodes_and_kinds_without_recursion() {
    let mut plan = constant();
    let mut previous = BridgeNodeId(0);
    for _ in 0..50_000 {
        previous = push(&mut plan, 0, BridgeNode::Tuple(vec![previous]));
    }
    plan.entries[0].root = previous;
    for index in 0..50_000 {
        plan.kinds
            .push(BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(index)));
    }
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.nodes[0].node = BridgeNode::Tuple(vec![previous]);
    assert_eq!(
        plan.validate_structure().expect_err("deep back edge").path,
        "nodes[1].items[0]"
    );
}

#[test]
fn many_rigid_nodes_share_a_deep_kind_without_rewalking_the_kind_chain() {
    let mut plan = constant();
    for index in 0..50_000 {
        plan.kinds
            .push(BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(index)));
    }
    for id in 0..50_000 {
        push(&mut plan, 50_000, BridgeNode::Rigid(BridgeRigidId(id)));
    }
    plan.kinds.push(BridgeKind::CaptureRow);
    let entries = (0..50_000)
        .map(|id| {
            (
                BridgeLabel::Capture(BridgeAtom::Rigid(BridgeRigidId(id))),
                BridgeNodeId(id + 1),
            )
        })
        .collect();
    let captures = push(&mut plan, 50_001, BridgeNode::Row(row(entries)));
    let root = push(
        &mut plan,
        0,
        BridgeNode::Captured {
            ty: BridgeNodeId(0),
            captures,
            value: true,
        },
    );
    plan.entries[0].root = root;
    // Deliberately no wall-clock budget: independent before/after measurements
    // establish scaling, while this pins the shared deep-kind graph workload.
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn shared_forall_diamond_with_closed_leaves_does_not_enumerate_context_paths() {
    let mut plan = constant();
    let mut previous = BridgeNodeId(0);
    for _ in 0..12_500 {
        let left = push(
            &mut plan,
            0,
            BridgeNode::Forall {
                vars: vec![binder("a", 0)],
                constraints: vec![],
                body: previous,
            },
        );
        let right = push(
            &mut plan,
            0,
            BridgeNode::Forall {
                vars: vec![binder("a", 0)],
                constraints: vec![],
                body: previous,
            },
        );
        previous = push(&mut plan, 0, BridgeNode::Tuple(vec![left, right]));
    }
    plan.entries[0].root = previous;
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn fifty_thousand_owned_members_use_exact_indexed_identity_lookup() {
    let mut plan = single_witness();
    let mut members = vec![BridgeNodeId(3)];
    for index in 0..50_000 {
        members.push(add_member(&mut plan, 0, &format!("Item{index}"), 0));
    }
    let body = push(&mut plan, 0, BridgeNode::Tuple(members));
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body,
    };
    assert_eq!(plan.validate_structure(), Ok(()));
}
