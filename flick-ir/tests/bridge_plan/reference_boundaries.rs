fn valid_record_tail() -> BridgePlan {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Row(BridgeKindId(0)));
    plan.nodes[0].node = BridgeNode::Record(BridgeRow {
        entries: vec![],
        tail: Some(BridgeNodeId(1)),
    });
    push(&mut plan, 1, BridgeNode::Var("tail".into()));
    plan.entries[0].free = vec![binder("tail", 1)];
    plan
}

#[test]
fn row_tail_and_kind_references_are_checked_before_private_node_checks() {
    let valid = valid_record_tail();
    assert_eq!(valid.validate_structure(), Ok(()));
    for row_node in [false, true] {
        for id in [2, u32::MAX] {
            let mut bad = valid.clone();
            let row = BridgeRow {
                entries: vec![],
                tail: Some(BridgeNodeId(id)),
            };
            bad.nodes[0].node = if row_node {
                bad.nodes[0].kind = BridgeKindId(1);
                BridgeNode::Row(row)
            } else {
                BridgeNode::Record(row)
            };
            let error = bad.validate_structure().expect_err("bad tail rejects");
            assert_eq!(error.path, "nodes[0].tail");
            assert_eq!(error.message, "reference out of range");
        }
    }
    for id in [2, u32::MAX] {
        let mut bad = valid.clone();
        bad.nodes[1].kind = BridgeKindId(id);
        let error = bad.validate_structure().expect_err("bad tail kind rejects");
        assert_eq!(error.path, "nodes[1].kind");
        assert_eq!(error.message, "reference out of range");

        let mut bad = valid.clone();
        bad.kinds[1] = BridgeKind::Row(BridgeKindId(id));
        let error = bad
            .validate_structure()
            .expect_err("bad Row payload rejects");
        assert_eq!(error.path, "kinds[1].row");
        assert_eq!(error.message, "reference out of range");
    }
    let mut bad = valid.clone();
    bad.kinds.clear();
    let error = bad
        .validate_structure()
        .expect_err("empty kind table rejects");
    assert_eq!(error.path, "nodes[0].kind");

    let mut bad = valid.clone();
    bad.nodes[1].kind = BridgeKindId(0);
    let error = bad.validate_structure().expect_err("non-row tail rejects");
    assert_eq!(error.path, "nodes[0].tail");
    assert_eq!(error.message, "record tail must have Row Type kind");

    let mut bad = valid.clone();
    bad.kinds.push(BridgeKind::Symbol);
    bad.kinds[1] = BridgeKind::Row(BridgeKindId(2));
    let error = bad
        .validate_structure()
        .expect_err("wrong row payload rejects");
    assert_eq!(error.path, "nodes[0].tail");
    assert_eq!(error.message, "record tail must have Row Type kind");

    let mut bad = valid;
    let BridgeNode::Record(row) = &mut bad.nodes[0].node else {
        panic!("record fixture");
    };
    row.tail = Some(BridgeNodeId(2));
    bad.nodes[1].kind = BridgeKindId(u32::MAX);
    let error = bad
        .validate_structure()
        .expect_err("reference order is stable");
    assert_eq!(error.path, "nodes[0].tail");
}
