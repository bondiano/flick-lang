fn open_prefix_graph(links: u32, diamonds: bool) -> (BridgePlan, Vec<BridgeNodeId>) {
    let mut plan = constant();
    let mut previous = BridgeNodeId(0);
    let mut prefixes = vec![];
    for i in 0..links {
        let name = format!("x{i:05}");
        plan.entries[0].free.push(binder(&name, 0));
        let var = push(&mut plan, 0, BridgeNode::Var(name));
        previous = push(&mut plan, 0, BridgeNode::Tuple(vec![previous, var]));
        if diamonds {
            let left = push(
                &mut plan,
                0,
                BridgeNode::Forall {
                    vars: vec![binder("unused", 0)],
                    constraints: vec![],
                    body: previous,
                },
            );
            let right = push(&mut plan, 0, BridgeNode::Tuple(vec![previous, previous]));
            previous = push(&mut plan, 0, BridgeNode::Tuple(vec![left, right]));
        }
        prefixes.push(previous);
    }
    plan.entries[0].root = previous;
    (plan, prefixes)
}

#[test]
fn growing_open_diamonds_share_versions_without_dropping_repeated_edges() {
    let (plan, _) = open_prefix_graph(12_500, true);
    assert_eq!(plan.nodes.len(), 62_501);
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn forall_masks_do_not_mutate_shared_children_or_other_roots() {
    let mut plan = constant();
    let x = push(&mut plan, 0, BridgeNode::Var("x".into()));
    let y = push(&mut plan, 0, BridgeNode::Var("y".into()));
    let body = push(&mut plan, 0, BridgeNode::Tuple(vec![x, y]));
    let mask_x = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("x", 0)],
            constraints: vec![],
            body,
        },
    );
    let mask_y = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("y", 0)],
            constraints: vec![],
            body,
        },
    );
    let root = push(
        &mut plan,
        0,
        BridgeNode::Tuple(vec![mask_x, mask_y, body, body]),
    );
    plan.entries[0].root = root;
    plan.entries[0].free = vec![binder("x", 0), binder("y", 0)];
    plan.entries.push(BridgeEntry {
        root: body,
        free: plan.entries[0].free.clone(),
        select: BridgeSelect::Value,
    });
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.entries[0].free.pop();
    let error = plan
        .validate_structure()
        .expect_err("both masks retain the other variable");
    assert_eq!(error.path, "entries[0].root");
    assert_eq!(error.message, "unbound variable \"y\"");
}

#[test]
fn alias_and_entry_prefix_roots_keep_independent_late_summaries() {
    let (mut plan, prefixes) = open_prefix_graph(64, false);
    for (alias, prefix) in [31, 63].into_iter().enumerate() {
        let constructor =
            BridgeConstructorId(u32::try_from(plan.constructors.len()).expect("fixture IDs"));
        plan.constructors.push(BridgeConstructor {
            identity: TypeName {
                module: "Probe".into(),
                name: format!("Prefix{alias}"),
            },
            origin: BridgeConstructorOrigin::Global,
            kind: BridgeKindId(0),
            meaning: BridgeMeaning::Alias(BridgeAliasId(
                u32::try_from(alias).expect("fixture IDs"),
            )),
            service_impl: None,
            dictionary_role: BridgeDictionaryRole::Passed,
        });
        plan.aliases.push(BridgeAlias {
            constructor,
            params: vec![],
            body: prefixes[prefix],
            free: plan.entries[0].free[..=prefix].to_vec(),
        });
    }
    plan.entries.push(BridgeEntry {
        root: prefixes[31],
        free: plan.entries[0].free[..32].to_vec(),
        select: BridgeSelect::Value,
    });
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.aliases[1].free.pop();
    plan.entries[0].free.clear();
    let error = plan
        .validate_structure()
        .expect_err("alias capture is checked before entry capture");
    assert_eq!(error.path, "aliases[1].body");
    assert_eq!(error.message, "unbound variable \"x00063\"");
}

#[test]
fn witness_contract_summaries_survive_parent_discharge_and_repeated_use() {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Module);
    let contract = push(&mut plan, 1, BridgeNode::Var("M".into()));
    let a = exists(&mut plan, "A", contract, BridgeNodeId(0));
    let b = exists(&mut plan, "B", contract, BridgeNodeId(0));
    let body = push(&mut plan, 0, BridgeNode::Tuple(vec![a, b]));
    let root = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("M", 1)],
            constraints: vec![],
            body,
        },
    );
    plan.entries[0].root = root;
    assert_eq!(plan.validate_structure(), Ok(()));
    let BridgeNode::Forall { vars, .. } = &mut plan.nodes[root.0 as usize].node else {
        panic!("forall fixture")
    };
    vars[0].kind = BridgeKindId(0);
    let error = plan
        .validate_structure()
        .expect_err("contract binder kind still matters");
    assert_eq!(error.path, format!("nodes[{}].vars", root.0));
    assert_eq!(error.message, "kind mismatch");
}

#[test]
fn requirement_kind_errors_still_precede_late_entry_reference_errors() {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Symbol);
    let body = push(&mut plan, 0, BridgeNode::Var("x".into()));
    let quantified = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("x", 1)],
            constraints: vec![],
            body,
        },
    );
    plan.entries[0].root = BridgeNodeId(u32::MAX);
    let error = plan
        .validate_structure()
        .expect_err("scopes precede entry references");
    assert_eq!(error.path, format!("nodes[{}].vars", quantified.0));
    assert_eq!(error.message, "kind mismatch");
}
