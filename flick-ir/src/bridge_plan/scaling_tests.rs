use super::*;

fn chain(links: usize) -> BridgePlan {
    let mut plan = BridgePlan {
        kinds: vec![BridgeKind::Type],
        witnesses: vec![],
        constructors: vec![],
        nodes: vec![BridgeType {
            kind: BridgeKindId(0),
            node: BridgeNode::Tuple(vec![]),
        }],
        aliases: vec![],
        entries: vec![],
    };
    let mut previous = BridgeNodeId(0);
    let mut free = Vec::new();
    for i in 0..links {
        let name = format!("x{i:05}");
        free.push(BridgeBinder {
            name: name.clone(),
            kind: BridgeKindId(0),
        });
        let var = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
        plan.nodes.push(BridgeType {
            kind: BridgeKindId(0),
            node: BridgeNode::Var(name),
        });
        let next = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
        plan.nodes.push(BridgeType {
            kind: BridgeKindId(0),
            node: BridgeNode::Tuple(vec![previous, var]),
        });
        previous = next;
    }
    plan.entries.push(BridgeEntry {
        root: previous,
        free,
        select: BridgeSelect::Value,
    });
    plan
}
fn measure(links: usize) -> metrics::Counts {
    let plan = chain(links);
    metrics::reset();
    assert_eq!(plan.validate_structure(), Ok(()));
    metrics::counts()
}
#[test]
fn open_chain_work_and_retained_storage_grow_subquadratically() {
    let small = measure(512);
    let large = measure(1024);
    eprintln!("open-chain 512={small:?}; 1024={large:?}");
    assert!(
        large.merge <= 3 * small.merge,
        "merge work: {small:?} -> {large:?}"
    );
    assert!(
        large.retained <= 3 * small.retained,
        "retained cells: {small:?} -> {large:?}"
    );
    assert!(
        large.lookup <= 3 * small.lookup,
        "lookup work: {small:?} -> {large:?}"
    );
}

#[test]
fn fifty_thousand_open_variables_retain_only_path_copied_indexes() {
    let links = 50_000;
    let measured = measure(links);
    let bits = usize::BITS - (links - 1).leading_zeros();
    eprintln!(
        "open-chain links={links} nodes={} bits={bits} counts={measured:?}",
        2 * links + 1
    );
    // Each variable's summary allocates at most h+1 nodes, its parent union
    // at most h, and its ordinary frame binding at most h+1.
    assert!(measured.retained <= links * (4 * bits as usize + 3));
    assert!(measured.merge <= links * (4 * bits as usize + 4));
    assert!(measured.lookup <= links * (bits as usize + 2));
}

fn unused_named_kind_prefixes(count: usize) -> BridgePlan {
    let mut plan = chain(0);
    plan.kinds.push(BridgeKind::Module);
    plan.nodes.push(BridgeType {
        kind: BridgeKindId(1),
        node: BridgeNode::Rigid(BridgeRigidId(17)),
    });
    let mut previous = BridgeKindId(0);
    let mut bodies = vec![];
    for i in 0..count {
        let id = BridgeWitnessId(u32::try_from(i).expect("fixture IDs"));
        let binder = format!("K{i}");
        plan.witnesses.push(BridgeWitness {
            scope: BridgeWitnessScope::Root,
            binder: binder.clone(),
            contract: BridgeNodeId(1),
            members: vec![BridgeWitnessMember {
                owner_path: vec![],
                name: "Mode".into(),
                kind: BridgeKindId(0),
            }],
        });
        let named = BridgeKindId(u32::try_from(plan.kinds.len()).expect("fixture IDs"));
        plan.kinds.push(BridgeKind::Named {
            module: format!("$exists:{binder}"),
            name: "Mode".into(),
            origin: BridgeConstructorOrigin::Witness(id),
        });
        let arrow = BridgeKindId(u32::try_from(plan.kinds.len()).expect("fixture IDs"));
        plan.kinds.push(BridgeKind::Arrow(previous, named));
        previous = arrow;
        let body = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
        plan.nodes.push(BridgeType {
            kind: BridgeKindId(0),
            node: BridgeNode::Exists {
                witness: id,
                body: BridgeNodeId(0),
            },
        });
        bodies.push(body);
    }
    let root = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
    plan.nodes.push(BridgeType {
        kind: BridgeKindId(0),
        node: BridgeNode::Tuple(bodies),
    });
    plan.entries[0].root = root;
    plan
}

#[test]
fn kind_support_prefix_storage_grows_subquadratically() {
    let measure = |count| {
        let plan = unused_named_kind_prefixes(count);
        assert_eq!(plan.validate_structure(), Ok(()));
        plan.measured_kind_support_cells()
            .expect("actual support summaries")
    };
    let small = measure(512);
    let large = measure(1024);
    eprintln!("actual kind-support cells: 512={small}, 1024={large}");
    assert!(
        large <= 3 * small,
        "kind-support prefix copies: {small} -> {large}"
    );
}

fn nested_kind_members(count: usize) -> BridgePlan {
    let mut plan = unused_named_kind_prefixes(count);
    for i in 0..count {
        plan.witnesses[i].scope = if i == 0 {
            BridgeWitnessScope::Root
        } else {
            BridgeWitnessScope::Within(BridgeWitnessId(u32::try_from(i - 1).expect("fixture IDs")))
        };
        plan.witnesses[i].members.push(BridgeWitnessMember {
            owner_path: vec![],
            name: "Fn".into(),
            kind: BridgeKindId(u32::try_from(3 + 2 * i).expect("fixture IDs")),
        });
        let BridgeNode::Exists { body, .. } = &mut plan.nodes[i + 2].node else {
            panic!("owner fixture")
        };
        *body = if i + 1 == count {
            BridgeNodeId(0)
        } else {
            BridgeNodeId(u32::try_from(i + 3).expect("fixture IDs"))
        };
    }
    plan.nodes.pop();
    plan.entries[0].root = BridgeNodeId(2);
    plan
}

#[test]
fn nested_named_metadata_access_grows_subquadratically() {
    let measure = |count| {
        let plan = nested_kind_members(count);
        metrics::reset();
        assert_eq!(plan.validate_structure(), Ok(()));
        metrics::counts().ancestor
    };
    let small = measure(128);
    let large = measure(256);
    eprintln!("actual nested named-kind ancestor steps:128={small},256={large}");
    assert!(
        large <= 3 * small,
        "nested ancestry walks: {small} -> {large}"
    );
}

fn measure_named(count: usize, nested: bool, rows: bool) -> metrics::Counts {
    let mut plan = if nested {
        nested_kind_members(count)
    } else {
        unused_named_kind_prefixes(count)
    };
    if rows {
        for i in 0..count {
            let prefix = BridgeKindId(u32::try_from(3 + 2 * i).expect("fixture IDs"));
            let row = BridgeKindId(u32::try_from(plan.kinds.len()).expect("fixture IDs"));
            plan.kinds.push(BridgeKind::Row(prefix));
            if nested {
                plan.witnesses[i].members.push(BridgeWitnessMember {
                    owner_path: vec![],
                    name: "Again".into(),
                    kind: row,
                });
            }
        }
    }
    metrics::reset();
    assert_eq!(plan.validate_structure(), Ok(()));
    metrics::counts()
}
#[test]
fn fifty_thousand_unused_kind_prefixes_share_support_storage() {
    let counts = measure_named(50_000, false, true);
    eprintln!("50k unused named kinds + Row versions: {counts:?}");
    assert!(counts.retained <= 50_000 * (4 * 17 + 3));
    assert!(counts.merge <= 50_000 * (6 * 17 + 16));
    assert_eq!(counts.kind_enumerated, 0);
}
#[test]
fn fifty_thousand_nested_kind_prefixes_check_ancestry_without_enumeration() {
    let counts = measure_named(50_000, true, true);
    eprintln!("50k nested named Fn + repeated Row prefixes: {counts:?}");
    assert!(counts.retained <= 50_000 * (4 * 17 + 3));
    assert!(counts.ancestor <= 50_000 * (6 * 17 + 16));
    assert!(counts.kind_checks <= 5 * 50_000 + 4);
    assert_eq!(
        counts.kind_enumerated, 0,
        "successful support checks never scan whole prefixes"
    );
}
#[test]
fn direct_witness_checks_skip_unrelated_variable_namespaces() {
    let measure = |n| {
        let mut plan = chain(n);
        plan.kinds.push(BridgeKind::Module);
        let contract = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
        plan.nodes.push(BridgeType {
            kind: BridgeKindId(1),
            node: BridgeNode::Rigid(BridgeRigidId(19)),
        });
        let mut body = plan.entries[0].root;
        for i in (0..n).rev() {
            let witness = BridgeWitnessId(u32::try_from(i).expect("fixture IDs"));
            // IDs are assigned in lexical order, independently of node creation.
            if plan.witnesses.is_empty() {
                plan.witnesses = (0..n)
                    .map(|j| BridgeWitness {
                        scope: if j == 0 {
                            BridgeWitnessScope::Root
                        } else {
                            BridgeWitnessScope::Within(BridgeWitnessId(
                                u32::try_from(j - 1).expect("fixture IDs"),
                            ))
                        },
                        binder: format!("K{j}"),
                        contract,
                        members: vec![],
                    })
                    .collect();
            }
            let owner = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
            plan.nodes.push(BridgeType {
                kind: BridgeKindId(0),
                node: BridgeNode::Exists { witness, body },
            });
            body = owner;
        }
        plan.entries[0].root = body;
        metrics::reset();
        assert_eq!(plan.validate_structure(), Ok(()));
        metrics::counts().range
    };
    let small = measure(512);
    let large = measure(1024);
    eprintln!("namespace range visits512={small},1024={large}");
    assert!(large <= 3 * small);
}

fn equal_open_dag(links: usize, raw_distinct: bool) -> BridgePlan {
    let mut plan = chain(links);
    if raw_distinct {
        plan.kinds.push(BridgeKind::Type);
    }
    let left = plan.entries[0].root;
    let mut right = BridgeNodeId(0);
    for i in (0..links).rev() {
        let var = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
        plan.nodes.push(BridgeType {
            kind: BridgeKindId(u32::from(raw_distinct)),
            node: BridgeNode::Var(format!("x{i:05}")),
        });
        let next = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
        plan.nodes.push(BridgeType {
            kind: BridgeKindId(0),
            node: BridgeNode::Tuple(vec![right, var]),
        });
        right = next;
    }
    let mut repeated = Vec::new();
    for _ in 0..links {
        let id = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
        plan.nodes.push(BridgeType {
            kind: BridgeKindId(0),
            node: BridgeNode::Tuple(vec![left, right]),
        });
        repeated.push(id);
    }
    let root = BridgeNodeId(u32::try_from(plan.nodes.len()).expect("fixture IDs"));
    plan.nodes.push(BridgeType {
        kind: BridgeKindId(0),
        node: BridgeNode::Tuple(repeated),
    });
    plan.entries[0].root = root;
    plan
}

#[test]
fn independently_built_equal_open_dag_unions_grow_subquadratically() {
    let mut measured = Vec::new();
    for raw_distinct in [false, true] {
        let measure = |n| {
            let plan = equal_open_dag(n, raw_distinct);
            metrics::reset();
            assert_eq!(plan.validate_structure(), Ok(()));
            metrics::counts().merge
        };
        let small = measure(512);
        let large = measure(1024);
        eprintln!("equal open DAG raw-distinct={raw_distinct}:512={small},1024={large}");
        measured.push((small, large));
    }
    for (small, large) in measured {
        assert!(large <= 3 * small, "repeated union work: {small}->{large}");
    }
}

#[test]
fn fifty_thousand_equal_open_dag_joins_reuse_policy_local_successes() {
    for raw_distinct in [false, true] {
        let links = 50_000;
        let plan = equal_open_dag(links, raw_distinct);
        assert_eq!(plan.nodes.len(), 5 * links + 2);
        metrics::reset();
        assert_eq!(plan.validate_structure(), Ok(()));
        let counts = metrics::counts();
        let bits = usize::BITS - (links - 1).leading_zeros();
        eprintln!("50k equal DAG raw-distinct={raw_distinct} counts={counts:?}");
        // Two independently built chains, ordinary-frame paths and cached
        // nontrivial subtree pairs; no per-parent full-map merge or copy.
        assert!(counts.retained <= links * (7 * bits as usize + 8));
        assert!(counts.merge <= links * (12 * bits as usize + 32));
        assert!(counts.memo_entries <= links * (4 * bits as usize + 8));
        assert!(counts.memo_lookups <= counts.merge);
    }
}
