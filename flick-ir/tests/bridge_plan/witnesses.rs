#[test]
fn existential_witnesses_do_not_bind_literal_variables_with_the_same_spelling() {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Module);
    let contract = push(&mut plan, 1, BridgeNode::Rigid(BridgeRigidId(17)));
    let body = push(&mut plan, 0, BridgeNode::Var("$exists:M".to_owned()));
    let root = exists(&mut plan, "M", contract, body);
    plan.entries[0].root = root;
    assert!(
        plan.validate_structure().is_err(),
        "module witness cannot mask a literal raw Var"
    );
    plan.entries[0].free.push(binder("$exists:M", 0));
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn literal_forall_binders_do_not_bind_existential_module_witnesses() {
    let mut plan = constant();
    plan.kinds.push(BridgeKind::Module);
    let contract = push(&mut plan, 1, BridgeNode::Rigid(BridgeRigidId(17)));
    let owner = exists(&mut plan, "M", contract, BridgeNodeId(0));
    plan.witnesses[0].members.push(BridgeWitnessMember {
        owner_path: vec![],
        name: "Member".to_owned(),
        kind: BridgeKindId(0),
    });
    plan.constructors.push(BridgeConstructor {
        origin: BridgeConstructorOrigin::Witness(BridgeWitnessId(0)),
        identity: TypeName {
            module: "$exists:M".to_owned(),
            name: "Member".to_owned(),
        },
        kind: BridgeKindId(0),
        meaning: BridgeMeaning::Nominal,
        service_impl: None,
        dictionary_role: BridgeDictionaryRole::Passed,
    });
    let body = push(&mut plan, 0, BridgeNode::Con(BridgeConstructorId(0)));
    let root = push(
        &mut plan,
        0,
        BridgeNode::Forall {
            vars: vec![binder("$exists:M", 0)],
            constraints: vec![],
            body,
        },
    );
    plan.entries[0].root = root;
    assert!(
        plan.validate_structure().is_err(),
        "ordinary forall cannot mask a witness ID"
    );
    plan.nodes[owner.0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: root,
    };
    plan.entries[0].root = owner;
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn accepted_plain_and_higher_existentials_share_literal_member_names() {
    // Exact accepted source: type alias Both =
    // ((exists K : Plain. K.Item), (exists K : Higher. K.Item Int)).
    // The raw literal Item identity is unchanged; provenance is lexical.
    let plan = both_exists();
    assert_eq!(plan.validate_structure(), Ok(()));
    assert!(
        plan.raw_type_eq(BridgeNodeId(3), BridgeNodeId(4))
            .expect("literal Con equality ignores implicit kind and origin")
    );
    assert!(
        !plan
            .raw_type_eq(BridgeNodeId(7), BridgeNodeId(8))
            .expect("different literal contracts remain unequal")
    );
}

#[test]
fn witness_reference_and_registry_errors_name_the_actual_field() {
    type Mutation = (&'static str, fn(&mut BridgePlan));
    let cases: [Mutation; 9] = [
        ("witnesses[0].contract", |p| {
            p.witnesses[0].contract = BridgeNodeId(99);
        }),
        ("witnesses[0].members[0].kind", |p| {
            p.witnesses[0].members[0].kind = BridgeKindId(99);
        }),
        ("witnesses[0].members[1].name", |p| {
            let duplicate = p.witnesses[0].members[0].clone();
            p.witnesses[0].members.push(duplicate);
        }),
        ("nodes[2].witness", |p| {
            p.nodes[2].node = BridgeNode::Exists {
                witness: BridgeWitnessId(99),
                body: BridgeNodeId(3),
            }
        }),
        ("constructors[0].origin", |p| {
            p.constructors[0].origin = BridgeConstructorOrigin::Witness(BridgeWitnessId(99));
        }),
        ("constructors[0].kind", |p| {
            p.constructors[0].kind = BridgeKindId(1);
        }),
        ("constructors[0].identity.module", |p| {
            p.constructors[0].identity.module = "$exists:Other".to_owned();
        }),
        ("constructors[0].identity.name", |p| {
            p.constructors[0].identity.name = "Unknown".to_owned();
        }),
        ("constructors[0].origin", |p| {
            p.constructors[0].origin = BridgeConstructorOrigin::Global;
        }),
    ];
    for (path, mutate) in cases {
        let mut plan = single_witness();
        mutate(&mut plan);
        assert_eq!(plan.validate_structure().expect_err(path).path, path);
    }
}

#[test]
fn witnesses_cannot_claim_alias_builtin_dictionary_or_service_authority() {
    let cases = [
        (
            BridgeMeaning::Alias(BridgeAliasId(0)),
            BridgeDictionaryRole::Passed,
            None,
            "constructors[0].meaning",
        ),
        (
            BridgeMeaning::Builtin(BridgeBuiltin::Eff),
            BridgeDictionaryRole::Passed,
            None,
            "constructors[0].meaning",
        ),
        (
            BridgeMeaning::Nominal,
            BridgeDictionaryRole::Erased,
            None,
            "constructors[0].dictionary_role",
        ),
        (
            BridgeMeaning::Nominal,
            BridgeDictionaryRole::Held,
            None,
            "constructors[0].dictionary_role",
        ),
        (
            BridgeMeaning::Nominal,
            BridgeDictionaryRole::Passed,
            Some(BridgeAliasId(0)),
            "constructors[0].service_impl",
        ),
    ];
    for (meaning, dictionary_role, service_impl, path) in cases {
        let mut plan = single_witness();
        plan.constructors[0].meaning = meaning;
        plan.constructors[0].dictionary_role = dictionary_role;
        plan.constructors[0].service_impl = service_impl;
        assert_eq!(plan.validate_structure().expect_err(path).path, path);
    }
}

#[test]
fn every_witness_has_exactly_one_owner_even_when_the_owner_is_unused() {
    let mut plan = single_witness();
    plan.entries[0].root = BridgeNodeId(0);
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.nodes[2].node = BridgeNode::Tuple(vec![]);
    assert_eq!(
        plan.validate_structure().expect_err("missing owner").path,
        "witnesses[0]"
    );
    let mut plan = single_witness();
    push(
        &mut plan,
        0,
        BridgeNode::Exists {
            witness: BridgeWitnessId(0),
            body: BridgeNodeId(0),
        },
    );
    assert_eq!(
        plan.validate_structure().expect_err("duplicate owner").path,
        "nodes[4].witness"
    );
}

#[test]
fn contract_and_body_edges_are_structural_and_have_strict_kinds() {
    let mut plan = single_witness();
    plan.witnesses[0].contract = BridgeNodeId(2);
    assert_eq!(
        plan.validate_structure().expect_err("contract cycle").path,
        "witnesses[0].contract"
    );
    let mut plan = single_witness();
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: BridgeNodeId(2),
    };
    assert_eq!(
        plan.validate_structure().expect_err("body cycle").path,
        "nodes[2].body"
    );
    let mut plan = single_witness();
    plan.witnesses[0].contract = BridgeNodeId(0);
    assert_eq!(
        plan.validate_structure()
            .expect_err("contract is not Module")
            .path,
        "witnesses[0].contract"
    );
    let mut plan = single_witness();
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: BridgeNodeId(1),
    };
    assert_eq!(
        plan.validate_structure()
            .expect_err("body is not Type")
            .path,
        "nodes[2].body"
    );
    let mut plan = single_witness();
    plan.nodes[2].kind = BridgeKindId(1);
    assert_eq!(
        plan.validate_structure()
            .expect_err("Exists is not Type")
            .path,
        "nodes[2].kind"
    );
}

#[test]
fn constructor_identity_uniqueness_includes_lexical_origin() {
    let mut plan = single_witness();
    plan.constructors.push(plan.constructors[0].clone());
    assert_eq!(
        plan.validate_structure()
            .expect_err("same identity and origin")
            .path,
        "constructors[1].identity"
    );
    assert_eq!(both_exists().validate_structure(), Ok(()));
    let mut plan = single_witness();
    plan.constructors[0].identity.module = "$module:Ordinary".to_owned();
    plan.constructors[0].origin = BridgeConstructorOrigin::Global;
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn witness_references_cannot_escape_or_be_bound_by_ordinary_free_names() {
    let mut plan = single_witness();
    plan.entries[0].root = BridgeNodeId(3);
    plan.entries[0]
        .free
        .extend([binder("K", 1), binder("$exists:K", 1)]);
    assert_eq!(
        plan.validate_structure().expect_err("escaped witness").path,
        "entries[0].root"
    );
}

#[test]
fn witness_does_not_scope_its_own_contract() {
    let mut plan = single_witness();
    let contract = add_member(&mut plan, 0, "Signature", 1);
    plan.witnesses[0].contract = contract;
    assert_eq!(
        plan.validate_structure()
            .expect_err("self witness contract")
            .path,
        "entries[0].root"
    );
}

#[test]
fn nested_contract_uses_outer_context_but_body_obeys_same_name_shadow_barrier() {
    let mut plan = single_witness();
    let contract = add_member(&mut plan, 0, "Signature", 1);
    let inner = exists(&mut plan, "K", contract, BridgeNodeId(0));
    plan.witnesses[1].scope = BridgeWitnessScope::Within(BridgeWitnessId(0));
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: inner,
    };
    // Same spelling is legal in the inner contract, before the new scope.
    assert_eq!(plan.validate_structure(), Ok(()));
    plan.nodes[inner.0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(1),
        body: BridgeNodeId(3),
    };
    assert_eq!(
        plan.validate_structure()
            .expect_err("outer K cannot cross inner K body")
            .path,
        "nodes[5].body"
    );
    plan.witnesses[1].binder = "Inner".to_owned();
    assert_eq!(plan.validate_structure(), Ok(()));
}

#[test]
fn inner_witness_is_not_available_after_its_own_exists_body() {
    let mut plan = single_witness();
    let inner = exists(&mut plan, "Inner", BridgeNodeId(1), BridgeNodeId(0));
    plan.witnesses[1].scope = BridgeWitnessScope::Within(BridgeWitnessId(0));
    let member = add_member(&mut plan, 1, "Item", 0);
    plan.nodes[inner.0 as usize].node = BridgeNode::Exists {
        witness: BridgeWitnessId(1),
        body: member,
    };
    let body = push(&mut plan, 0, BridgeNode::Tuple(vec![inner, member]));
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body,
    };
    assert_eq!(
        plan.validate_structure()
            .expect_err("inner witness escapes")
            .path,
        "entries[0].root"
    );
}

#[test]
fn aliases_cannot_capture_callers_witnesses_but_can_own_exists_internally() {
    let mut plan = single_witness();
    let alias = add_global_alias(&mut plan, BridgeNodeId(3));
    plan.nodes[2].node = BridgeNode::Exists {
        witness: BridgeWitnessId(0),
        body: alias,
    };
    assert_eq!(
        plan.validate_structure()
            .expect_err("alias captures caller witness")
            .path,
        "aliases[0].body"
    );
    let mut plan = single_witness();
    let alias = add_global_alias(&mut plan, BridgeNodeId(2));
    plan.entries[0].root = alias;
    assert_eq!(plan.validate_structure(), Ok(()));
}
