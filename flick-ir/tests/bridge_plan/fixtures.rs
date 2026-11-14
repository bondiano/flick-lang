fn constant() -> BridgePlan {
    BridgePlan {
        kinds: vec![BridgeKind::Type],
        witnesses: vec![],
        constructors: vec![],
        nodes: vec![BridgeType {
            kind: BridgeKindId(0),
            node: BridgeNode::Tuple(vec![]),
        }],
        aliases: vec![],
        entries: vec![BridgeEntry {
            root: BridgeNodeId(0),
            free: vec![],
            select: BridgeSelect::Value,
        }],
    }
}

fn push(plan: &mut BridgePlan, kind: u32, node: BridgeNode) -> BridgeNodeId {
    let id = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("small fixture"));
    plan.nodes.push(BridgeType {
        kind: BridgeKindId(kind),
        node,
    });
    id
}

fn exists(
    plan: &mut BridgePlan,
    binder: &str,
    contract: BridgeNodeId,
    body: BridgeNodeId,
) -> BridgeNodeId {
    let witness = BridgeWitnessId(u32::try_from(plan.witnesses.len()).expect("small fixture"));
    plan.witnesses.push(BridgeWitness {
        scope: BridgeWitnessScope::Root,
        binder: binder.to_owned(),
        contract,
        members: vec![],
    });
    push(plan, 0, BridgeNode::Exists { witness, body })
}

fn binder(name: &str, kind: u32) -> BridgeBinder {
    BridgeBinder {
        name: name.to_owned(),
        kind: BridgeKindId(kind),
    }
}

fn row(entries: Vec<(BridgeLabel, BridgeNodeId)>) -> BridgeRow {
    BridgeRow {
        entries,
        tail: None,
    }
}

fn recursive_alias() -> BridgePlan {
    let mut plan = constant();
    plan.kinds
        .push(BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(0)));
    plan.constructors.push(BridgeConstructor {
        origin: BridgeConstructorOrigin::Global,
        identity: TypeName {
            module: "App".to_owned(),
            name: "Loop".to_owned(),
        },
        kind: BridgeKindId(1),
        meaning: BridgeMeaning::Alias(BridgeAliasId(0)),
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    let con = push(&mut plan, 1, BridgeNode::Con(BridgeConstructorId(0)));
    let var = push(&mut plan, 0, BridgeNode::Var("a".to_owned()));
    let app = push(
        &mut plan,
        0,
        BridgeNode::App {
            head: con,
            arg: var,
        },
    );
    plan.aliases.push(BridgeAlias {
        constructor: BridgeConstructorId(0),
        params: vec![binder("a", 0)],
        free: vec![],
        body: app,
    });
    plan
}

fn rich_graph() -> BridgePlan {
    let mut plan = recursive_alias();
    plan.kinds.extend([
        BridgeKind::ServiceRow,
        BridgeKind::ErrorRow,
        BridgeKind::CaptureRow,
        BridgeKind::Region,
        BridgeKind::Symbol,
        BridgeKind::Constraint,
        BridgeKind::Module,
        BridgeKind::Service,
        BridgeKind::Row(BridgeKindId(0)),
        BridgeKind::Named {
            origin: BridgeConstructorOrigin::Global,
            module: "App".to_owned(),
            name: "Finite".to_owned(),
        },
    ]);
    let services = push(&mut plan, 2, BridgeNode::Row(row(vec![])));
    let errors = push(&mut plan, 3, BridgeNode::Row(row(vec![])));
    let rigid = push(&mut plan, 5, BridgeNode::Rigid(BridgeRigidId(1234)));
    let labels = vec![
        BridgeLabel::Capture(BridgeAtom::Rigid(BridgeRigidId(1234))),
        BridgeLabel::Provided(Box::new(BridgeLabel::Live)),
    ];
    let captures = push(
        &mut plan,
        4,
        BridgeNode::Row(row(labels
            .iter()
            .cloned()
            .map(|label| (label, rigid))
            .collect())),
    );
    let bounded = push(
        &mut plan,
        0,
        BridgeNode::Bounded {
            ty: BridgeNodeId(0),
            captures,
        },
    );
    let qualified = push(
        &mut plan,
        0,
        BridgeNode::Captured {
            ty: bounded,
            captures,
            value: true,
        },
    );
    let arrow = push(
        &mut plan,
        0,
        BridgeNode::Arrow {
            param: qualified,
            effects: Some(BridgeEffects {
                services,
                errors,
                sync: false,
                native_direct: true,
            }),
            result: BridgeNodeId(0),
        },
    );
    let symbol = push(&mut plan, 6, BridgeNode::Symbol("field".to_owned()));
    let fields = push(
        &mut plan,
        10,
        BridgeNode::Row(row(vec![(BridgeLabel::Named("field".to_owned()), arrow)])),
    );
    let union = push(&mut plan, 4, BridgeNode::Union(captures, captures));
    push(
        &mut plan,
        4,
        BridgeNode::Without {
            row: union,
            gone: labels,
        },
    );
    let record = push(
        &mut plan,
        0,
        BridgeNode::Record(BridgeRow {
            entries: vec![],
            tail: Some(fields),
        }),
    );
    let contract = push(&mut plan, 8, BridgeNode::Rigid(BridgeRigidId(777)));
    let exists = exists(&mut plan, "K", contract, record);
    // Keep symbol and the recursive alias declaration in the same raw graph;
    // node storage is not a normalized conversion shape.
    assert_eq!(plan.nodes[symbol.0 as usize].kind, BridgeKindId(6));
    plan.entries[0].root = exists;
    plan.entries[0].select = BridgeSelect::ResultAfter(0);
    plan
}

fn labeled_arrow(label: BridgeLabel, atom_kind: BridgeKind, rigid: bool) -> BridgePlan {
    let mut plan = constant();
    plan.kinds
        .extend([BridgeKind::ServiceRow, BridgeKind::ErrorRow, atom_kind]);
    let errors = push(
        &mut plan,
        2,
        BridgeNode::Row(row(vec![(label, BridgeNodeId(0))])),
    );
    let services = push(&mut plan, 1, BridgeNode::Row(row(vec![])));
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
    if rigid {
        push(&mut plan, 3, BridgeNode::Rigid(BridgeRigidId(17)));
    } else {
        plan.entries[0].free.push(binder("a", 3));
    }
    plan
}

#[cfg(feature = "cbor")]
fn remove_cbor_field(value: &mut Value, field: &str) {
    match value {
        Value::Map(entries) => {
            entries.retain(|(key, _)| key.as_text() != Some(field));
            for (_, child) in entries {
                remove_cbor_field(child, field);
            }
        }
        Value::Array(values) => {
            for child in values {
                remove_cbor_field(child, field);
            }
        }
        _ => {}
    }
}
