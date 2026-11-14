fn projected_clock() -> BridgePlan {
    let mut plan = constant();
    plan.kinds.extend([
        BridgeKind::Service,
        BridgeKind::ServiceRow,
        BridgeKind::ErrorRow,
    ]);
    plan.constructors.push(BridgeConstructor {
        origin: BridgeConstructorOrigin::Global,
        identity: TypeName {
            module: "Witness".to_owned(),
            name: "Clock".to_owned(),
        },
        kind: BridgeKindId(1),
        meaning: BridgeMeaning::Nominal,
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    let clock = push(&mut plan, 1, BridgeNode::Con(BridgeConstructorId(0)));
    let services = push(
        &mut plan,
        2,
        BridgeNode::Row(row(vec![(
            BridgeLabel::Service(BridgeAtom::Var("Clock".to_owned())),
            clock,
        )])),
    );
    let errors = push(&mut plan, 3, BridgeNode::Row(row(vec![])));
    // Exact accepted projected Callback: Unit ->{Witness.Clock} Int, retaining
    // the frozen Service(Var("Clock")) label from static-module projection.
    for name in ["Unit", "Int"] {
        plan.constructors.push(BridgeConstructor {
            origin: BridgeConstructorOrigin::Global,
            identity: TypeName {
                module: "Prelude".to_owned(),
                name: name.to_owned(),
            },
            kind: BridgeKindId(0),
            meaning: BridgeMeaning::Nominal,
            service_impl: None,
            dictionary_role: BridgeDictionaryRole::Passed,
        });
    }
    let unit = push(&mut plan, 0, BridgeNode::Con(BridgeConstructorId(1)));
    let int = push(&mut plan, 0, BridgeNode::Con(BridgeConstructorId(2)));
    let root = push(
        &mut plan,
        0,
        BridgeNode::Arrow {
            param: unit,
            effects: Some(BridgeEffects {
                services,
                errors,
                sync: false,
                native_direct: false,
            }),
            result: int,
        },
    );
    plan.entries[0].root = root;
    plan
}

fn both_exists() -> BridgePlan {
    let mut plan = constant();
    plan.kinds.extend([
        BridgeKind::Module,
        BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(0)),
    ]);
    for name in ["Plain", "Higher"] {
        plan.constructors.push(BridgeConstructor {
            origin: BridgeConstructorOrigin::Global,
            identity: TypeName {
                module: "Probe".to_owned(),
                name: name.to_owned(),
            },
            kind: BridgeKindId(1),
            meaning: BridgeMeaning::Nominal,
            service_impl: None,
            dictionary_role: BridgeDictionaryRole::Passed,
        });
    }
    let plain_contract = push(&mut plan, 1, BridgeNode::Con(BridgeConstructorId(0)));
    let higher_contract = push(&mut plan, 1, BridgeNode::Con(BridgeConstructorId(1)));
    for (index, kind) in [0, 2].into_iter().enumerate() {
        let witness = BridgeWitnessId(u32::try_from(index).expect("two witnesses"));
        plan.witnesses.push(BridgeWitness {
            scope: BridgeWitnessScope::Root,
            binder: "K".to_owned(),
            contract: if index == 0 {
                plain_contract
            } else {
                higher_contract
            },
            members: vec![BridgeWitnessMember {
                owner_path: vec![],
                name: "Item".to_owned(),
                kind: BridgeKindId(kind),
            }],
        });
        plan.constructors.push(BridgeConstructor {
            origin: BridgeConstructorOrigin::Witness(witness),
            identity: TypeName {
                module: "$exists:K".to_owned(),
                name: "Item".to_owned(),
            },
            kind: BridgeKindId(kind),
            meaning: BridgeMeaning::Nominal,
            service_impl: None,
            dictionary_role: BridgeDictionaryRole::Passed,
        });
    }
    let first_member = push(&mut plan, 0, BridgeNode::Con(BridgeConstructorId(2)));
    let higher = push(&mut plan, 2, BridgeNode::Con(BridgeConstructorId(3)));
    plan.constructors.push(BridgeConstructor {
        origin: BridgeConstructorOrigin::Global,
        identity: TypeName {
            module: "Prelude".to_owned(),
            name: "Int".to_owned(),
        },
        kind: BridgeKindId(0),
        meaning: BridgeMeaning::Nominal,
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    let int = push(&mut plan, 0, BridgeNode::Con(BridgeConstructorId(4)));
    let applied = push(
        &mut plan,
        0,
        BridgeNode::App {
            head: higher,
            arg: int,
        },
    );
    let left = push(
        &mut plan,
        0,
        BridgeNode::Exists {
            witness: BridgeWitnessId(0),
            body: first_member,
        },
    );
    let right = push(
        &mut plan,
        0,
        BridgeNode::Exists {
            witness: BridgeWitnessId(1),
            body: applied,
        },
    );
    let root = push(&mut plan, 0, BridgeNode::Tuple(vec![left, right]));
    plan.entries[0].root = root;
    plan
}

fn single_witness() -> BridgePlan {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Module);
    let contract = push(&mut plan, 1, BridgeNode::Rigid(BridgeRigidId(17)));
    let owner = exists(&mut plan, "K", contract, BridgeNodeId(0));
    plan.witnesses[0].members.push(BridgeWitnessMember {
        owner_path: vec![],
        name: "Item".to_owned(),
        kind: BridgeKindId(0),
    });
    plan.constructors.push(BridgeConstructor {
        origin: BridgeConstructorOrigin::Witness(BridgeWitnessId(0)),
        identity: TypeName {
            module: "$exists:K".to_owned(),
            name: "Item".to_owned(),
        },
        kind: BridgeKindId(0),
        meaning: BridgeMeaning::Nominal,
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    let member = push(&mut plan, 0, BridgeNode::Con(BridgeConstructorId(0)));
    plan.nodes[owner.0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: member,
    };
    plan.entries[0].root = owner;
    plan
}

fn add_member(plan: &mut BridgePlan, witness: u32, name: &str, kind: u32) -> BridgeNodeId {
    plan.witnesses[witness as usize]
        .members
        .push(BridgeWitnessMember {
            owner_path: vec![],
            name: name.to_owned(),
            kind: BridgeKindId(kind),
        });
    let ctor = BridgeConstructorId(u32::try_from(plan.constructors.len()).expect("small fixture"));
    plan.constructors.push(BridgeConstructor {
        origin: BridgeConstructorOrigin::Witness(BridgeWitnessId(witness)),
        identity: TypeName {
            module: format!("$exists:{}", plan.witnesses[witness as usize].binder),
            name: name.to_owned(),
        },
        kind: BridgeKindId(kind),
        meaning: BridgeMeaning::Nominal,
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    push(plan, kind, BridgeNode::Con(ctor))
}

fn add_global_alias(plan: &mut BridgePlan, body: BridgeNodeId) -> BridgeNodeId {
    let constructor =
        BridgeConstructorId(u32::try_from(plan.constructors.len()).expect("small fixture"));
    let alias = BridgeAliasId(u32::try_from(plan.aliases.len()).expect("small fixture"));
    plan.constructors.push(BridgeConstructor {
        origin: BridgeConstructorOrigin::Global,
        identity: TypeName {
            module: "App".to_owned(),
            name: "Alias".to_owned(),
        },
        kind: BridgeKindId(0),
        meaning: BridgeMeaning::Alias(alias),
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    plan.aliases.push(BridgeAlias {
        constructor,
        params: vec![],
        free: vec![],
        body,
    });
    push(plan, 0, BridgeNode::Con(constructor))
}

fn witness_manifest_alias() -> BridgePlan {
    let mut plan = single_witness();
    let arrow = BridgeKindId(u32::try_from(plan.kinds.len()).expect("small kind table"));
    plan.kinds
        .push(BridgeKind::Arrow(BridgeKindId(0), BridgeKindId(0)));
    "Box".clone_into(&mut plan.witnesses[0].members[0].name);
    plan.witnesses[0].members[0].kind = arrow;
    "Box".clone_into(&mut plan.constructors[0].identity.name);
    plan.constructors[0].kind = arrow;
    plan.constructors[0].meaning = BridgeMeaning::Alias(BridgeAliasId(0));
    plan.constructors.push(BridgeConstructor {
        identity: TypeName {
            module: "Prelude".to_owned(),
            name: "List".to_owned(),
        },
        origin: BridgeConstructorOrigin::Global,
        kind: arrow,
        meaning: BridgeMeaning::Builtin(BridgeBuiltin::List),
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    let list = push(&mut plan, arrow.0, BridgeNode::Con(BridgeConstructorId(1)));
    let a = push(&mut plan, 0, BridgeNode::Var("a".to_owned()));
    let body = push(&mut plan, 0, BridgeNode::App { head: list, arg: a });
    plan.aliases.push(BridgeAlias {
        constructor: BridgeConstructorId(0),
        params: vec![binder("a", 0)],
        free: vec![],
        body,
    });
    plan.nodes[3].kind = arrow;
    let applied = push(
        &mut plan,
        0,
        BridgeNode::App {
            head: BridgeNodeId(3),
            arg: BridgeNodeId(0),
        },
    );
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: applied,
    };
    plan
}

fn manifest(
    plan: &mut BridgePlan,
    witness: u32,
    name: &str,
    kind: u32,
    params: Vec<BridgeBinder>,
    free: Vec<BridgeBinder>,
    body: BridgeNodeId,
) -> BridgeNodeId {
    let head = add_member(plan, witness, name, kind);
    let BridgeNode::Con(constructor) = plan.nodes[head.0 as usize].node else {
        unreachable!()
    };
    let alias = BridgeAliasId(u32::try_from(plan.aliases.len()).expect("small alias table"));
    plan.constructors[constructor.0 as usize].meaning = BridgeMeaning::Alias(alias);
    plan.aliases.push(BridgeAlias {
        constructor,
        params,
        free,
        body,
    });
    head
}

fn captured_alias() -> BridgePlan {
    let mut plan = single_witness();
    let variable = push(&mut plan, 0, BridgeNode::Var("a".to_owned()));
    let alias = manifest(
        &mut plan,
        0,
        "Captured",
        0,
        vec![],
        vec![binder("a", 0)],
        variable,
    );
    plan.kinds.push(BridgeKind::Symbol);
    let caller = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("a", 2)],
            constraints: vec![],
            body: alias,
        },
    );
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: caller,
    };
    let root = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("a", 0)],
            constraints: vec![],
            body: BridgeNodeId(2),
        },
    );
    plan.entries[0].root = root;
    plan
}

fn member_at(plan: &mut BridgePlan, path: &[&str], name: &str) -> BridgeNodeId {
    let node = add_member(plan, 0, name, 0);
    let member = plan.witnesses[0].members.last_mut().expect("member");
    member.owner_path = path.iter().map(|p| (*p).to_owned()).collect();
    let BridgeNode::Con(id) = plan.nodes[node.0 as usize].node else {
        unreachable!()
    };
    for part in path {
        plan.constructors[id.0 as usize].identity.module.push('.');
        plan.constructors[id.0 as usize]
            .identity
            .module
            .push_str(part);
    }
    node
}

fn named_witness_kind(plan: &mut BridgePlan, witness: u32) -> u32 {
    let _ = add_member(plan, witness, "Mode", 0);
    let kind = u32::try_from(plan.kinds.len()).expect("small kinds");
    plan.kinds.push(BridgeKind::Named {
        module: format!("$exists:{}", plan.witnesses[witness as usize].binder),
        name: "Mode".to_owned(),
        origin: BridgeConstructorOrigin::Witness(BridgeWitnessId(witness)),
    });
    kind
}
