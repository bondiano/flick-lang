#[test]
fn capture_row_members_can_be_regions_and_labels_are_structured() {
    let mut plan = constant();
    plan.kinds
        .extend([BridgeKind::Region, BridgeKind::CaptureRow]);
    let region = push(&mut plan, 1, BridgeNode::Var("r".to_owned()));
    let captures = push(
        &mut plan,
        2,
        BridgeNode::Row(row(vec![(
            BridgeLabel::Capture(BridgeAtom::Var("r".to_owned())),
            region,
        )])),
    );
    let qualified = push(
        &mut plan,
        0,
        BridgeNode::Captured {
            ty: BridgeNodeId(0),
            captures,
            value: true,
        },
    );
    plan.entries[0].root = qualified;
    plan.entries[0].free.push(binder("r", 1));
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.entries[0].free.clear();
    assert!(plan.validate_structure().is_err());
}

#[test]
fn service_row_live_and_provided_members_use_their_actual_core_kinds() {
    let mut plan = constant();
    plan.kinds.extend([
        BridgeKind::ServiceRow,
        BridgeKind::CaptureRow,
        BridgeKind::Constraint,
    ]);
    let captures = push(&mut plan, 2, BridgeNode::Row(row(vec![])));
    let obligation = push(&mut plan, 3, BridgeNode::Var("provided".to_owned()));
    let services = push(
        &mut plan,
        1,
        BridgeNode::Row(row(vec![
            (BridgeLabel::Live, captures),
            (
                BridgeLabel::Provided(Box::new(BridgeLabel::Named("App.Db".to_owned()))),
                obligation,
            ),
        ])),
    );
    plan.kinds.push(BridgeKind::ErrorRow);
    let errors = push(&mut plan, 4, BridgeNode::Row(row(vec![])));
    let arrow = push(
        &mut plan,
        0,
        BridgeNode::Arrow {
            param: BridgeNodeId(0),
            effects: Some(BridgeEffects {
                services,
                errors,
                sync: false,
                native_direct: false,
            }),
            result: BridgeNodeId(0),
        },
    );
    plan.entries[0].root = arrow;
    plan.entries[0].free.push(binder("provided", 3));
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn frozen_error_label_is_not_a_kind_requirement_on_same_spelled_variable() {
    let plan = labeled_arrow(
        BridgeLabel::Error(BridgeAtom::Var("a".to_owned())),
        BridgeKind::Symbol,
        false,
    );
    // Accepted projected Clock evidence proves labels are frozen literals,
    // not references requiring invented historical binder/kind contexts.
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn frozen_rigid_label_does_not_constrain_a_real_rigid_node() {
    let plan = labeled_arrow(
        BridgeLabel::Error(BridgeAtom::Rigid(BridgeRigidId(17))),
        BridgeKind::Symbol,
        true,
    );
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn capture_members_reject_symbol_but_allow_the_core_capture_kind_set() {
    let mut plan = constant();
    plan.kinds
        .extend([BridgeKind::CaptureRow, BridgeKind::Symbol]);
    let member = push(&mut plan, 2, BridgeNode::Symbol("not-a-capture".to_owned()));
    let captures = push(
        &mut plan,
        1,
        BridgeNode::Row(row(vec![(
            BridgeLabel::Capture(BridgeAtom::Rigid(BridgeRigidId(17))),
            member,
        )])),
    );
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
    assert_eq!(
        plan.validate_structure()
            .expect_err("Symbol member is invalid")
            .path,
        "nodes[2].entries[0].type"
    );
}

#[test]
fn error_labels_allow_higher_kinded_heads_and_survive_member_substitution() {
    let mut plan = labeled_arrow(
        BridgeLabel::Error(BridgeAtom::Var("a".to_owned())),
        BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(0)),
        false,
    );
    let head = push(&mut plan, 3, BridgeNode::Var("a".to_owned()));
    let member = push(
        &mut plan,
        0,
        BridgeNode::App {
            head,
            arg: BridgeNodeId(0),
        },
    );
    if let BridgeNode::Row(row) = &mut plan.nodes[1].node {
        row.entries[0].1 = member;
    }
    assert_eq!(plan.validate_structure(), Ok(()));
    // Core Type::map_children/substitute retains the raw old label. Its
    // original free kind context is explicitly retained, not re-inferred from
    // the substituted member (which is no longer headed by a).
    if let BridgeNode::Row(row) = &mut plan.nodes[1].node {
        row.entries[0].1 = BridgeNodeId(0);
    }
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn frozen_service_labels_do_not_reinfer_kind_from_current_members() {
    let mut plan = labeled_arrow(
        BridgeLabel::Named("error".to_owned()),
        BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(4)),
        false,
    );
    plan.kinds.push(BridgeKind::Service);
    let head = push(&mut plan, 3, BridgeNode::Var("a".to_owned()));
    let member = push(
        &mut plan,
        4,
        BridgeNode::App {
            head,
            arg: BridgeNodeId(0),
        },
    );
    if let BridgeNode::Row(row) = &mut plan.nodes[1].node {
        row.entries.clear();
    }
    if let BridgeNode::Row(row) = &mut plan.nodes[2].node {
        row.entries.push((
            BridgeLabel::Service(BridgeAtom::Var("a".to_owned())),
            member,
        ));
    }
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.kinds.push(BridgeKind::Symbol);
    plan.entries[0].free.push(binder("bad", 5));
    if let BridgeNode::Row(row) = &mut plan.nodes[2].node {
        row.entries[0].0 = BridgeLabel::Service(BridgeAtom::Var("bad".to_owned()));
    }
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn capture_rows_and_atom_labels_accept_all_core_capture_kinds() {
    let allowed = [
        BridgeKind::Type,
        BridgeKind::Region,
        BridgeKind::CaptureRow,
        BridgeKind::ErrorRow,
        BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(0)),
    ];
    for kind in allowed {
        let mut plan = constant();
        plan.kinds.extend([BridgeKind::CaptureRow, kind]);
        let member = push(&mut plan, 2, BridgeNode::Rigid(BridgeRigidId(17)));
        let captures = push(
            &mut plan,
            1,
            BridgeNode::Row(row(vec![(
                BridgeLabel::Capture(BridgeAtom::Rigid(BridgeRigidId(17))),
                member,
            )])),
        );
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
        assert_eq!(plan.validate_structure(), Ok(()));
    }
}

#[test]
fn frozen_rigid_labels_can_survive_in_different_row_families() {
    let mut plan = labeled_arrow(
        BridgeLabel::Error(BridgeAtom::Rigid(BridgeRigidId(17))),
        BridgeKind::Type,
        false,
    );
    plan.entries[0].free.clear();
    plan.kinds.push(BridgeKind::Service);
    let member = push(&mut plan, 4, BridgeNode::Rigid(BridgeRigidId(18)));
    if let BridgeNode::Row(row) = &mut plan.nodes[2].node {
        row.entries.push((
            BridgeLabel::Service(BridgeAtom::Rigid(BridgeRigidId(17))),
            member,
        ));
    }
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn accepted_projected_clock_label_does_not_invent_a_clock_variable() {
    let plan = projected_clock();
    assert_eq!(plan.validate_structure(), Ok(()));
    assert!(plan.entries[0].free.is_empty());
    let mut bad_member = plan.clone();
    if let BridgeNode::Row(row) = &mut bad_member.nodes[2].node {
        row.entries[0].1 = BridgeNodeId(0);
    }
    assert_eq!(
        bad_member
            .validate_structure()
            .expect_err("actual service member remains strict")
            .path,
        "nodes[2].entries[0].type"
    );
    let mut real_variable = plan;
    real_variable.nodes[1].node = BridgeNode::Var("Clock".to_owned());
    assert_eq!(
        real_variable
            .validate_structure()
            .expect_err("actual Clock variable needs a binder")
            .path,
        "entries[0].root"
    );
}

#[test]
fn frozen_provided_and_without_labels_preserve_literal_names_and_rigids() {
    let mut plan = constant();
    plan.kinds
        .extend([BridgeKind::ServiceRow, BridgeKind::Constraint]);
    let obligation = push(&mut plan, 2, BridgeNode::Rigid(BridgeRigidId(42)));
    let frozen = BridgeLabel::Provided(Box::new(BridgeLabel::Provided(Box::new(
        BridgeLabel::Service(BridgeAtom::Var("Clock".to_owned())),
    ))));
    let services = push(
        &mut plan,
        1,
        BridgeNode::Row(row(vec![(frozen.clone(), obligation)])),
    );
    let gone = vec![
        frozen.clone(),
        BridgeLabel::Capture(BridgeAtom::Rigid(BridgeRigidId(42))),
    ];
    let removed = push(
        &mut plan,
        1,
        BridgeNode::Without {
            row: services,
            gone: gone.clone(),
        },
    );
    let duplicate = push(
        &mut plan,
        1,
        BridgeNode::Without {
            row: services,
            gone,
        },
    );
    plan.kinds.push(BridgeKind::ErrorRow);
    let errors = push(&mut plan, 3, BridgeNode::Row(row(vec![])));
    let root = push(
        &mut plan,
        0,
        BridgeNode::Arrow {
            param: BridgeNodeId(0),
            effects: Some(BridgeEffects {
                services: removed,
                errors,
                sync: false,
                native_direct: false,
            }),
            result: BridgeNodeId(0),
        },
    );
    plan.entries[0].root = root;
    assert_eq!(plan.validate_structure(), Ok(()));
    assert!(
        plan.raw_type_eq(removed, duplicate)
            .expect("literal frozen label equality")
    );
    if let BridgeNode::Without { gone, .. } = &mut plan.nodes[duplicate.0 as usize].node {
        gone.reverse();
    }
    assert!(
        !plan
            .raw_type_eq(removed, duplicate)
            .expect("label order remains raw structure")
    );
    assert_eq!(plan.entries[0].free, vec![]);
}
