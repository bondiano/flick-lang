#[test]
fn reuse_replaces_semantic_partition_for_unique_and_shared_equal_capacity_objects() {
    for shared in [false, true] {
        let mut heap = Heap::new();
        let original = heap
            .construct("T.Box", vec![Value::Int(1), Value::Int(2)])
            .expect("ordinary fields");
        let old_id = original.object_id();
        let external = if shared {
            Some(heap.dup(&original).expect("shared candidate"))
        } else {
            None
        };
        let changed = heap
            .reuse_with_dicts(original, "T.Box", vec![Value::Int(1)], vec![Value::Int(99)])
            .expect("ordinary to evidence");
        assert_eq!(changed.object_id() == old_id, !shared);
        let alias = heap.dup(&changed).expect("comparison alias");
        let expected = heap
            .construct_with_dicts(
                "T.Box",
                vec![Value::Int(1)],
                vec![Value::Int(77), Value::Int(88)],
            )
            .expect("same semantic payload");
        compare_values(&mut heap, BinOp::Eq, alias, expected, true);
        let changed_id = changed.object_id();
        let old_partition = if shared {
            Some(heap.dup(&changed).expect("shared evidence candidate"))
        } else {
            None
        };
        let reset = heap
            .reuse(changed, "T.Box", vec![Value::Int(1), Value::Int(2)])
            .expect("evidence to ordinary");
        assert_eq!(reset.object_id() == changed_id, !shared);
        let expected = heap
            .construct("T.Box", vec![Value::Int(1), Value::Int(3)])
            .expect("ordinary second field differs");
        compare_values(&mut heap, BinOp::Eq, reset, expected, false);
        if let Some(external) = external {
            heap.drop_value(external)
                .expect("old ordinary candidate alias");
        }
        if let Some(old_partition) = old_partition {
            heap.drop_value(old_partition)
                .expect("old evidence candidate alias");
        }
        assert_eq!(heap.stats().reuses, if shared { 0 } else { 2 });
        heap.finish()
            .expect("old and new partitions free all physical fields");
    }
}
#[test]
fn matching_constructor_requires_the_partition_not_just_total_physical_count() {
    let mut heap = Heap::new();
    let input = heap
        .construct("M.Box.Box", vec![Value::Int(42), Value::Int(7)])
        .expect("wrong all-ordinary partition");
    let p = matcher(Pat::Con(constructor(1, 1), vec![Pat::Wild, Pat::Wild]));
    assert_eq!(
        run(&mut heap, &p, input, 1000).expect_err("same total but wrong evidence boundary"),
        Error::PatternEvidenceLayout {
            expected_fields: 1,
            actual_fields: 2,
            expected_dicts: 1,
            actual_dicts: 0
        }
    );
    let input = heap
        .construct_with_dicts("M.Box.Box", vec![Value::Int(42)], vec![Value::Int(7)])
        .expect("correct partition");
    let result = run(&mut heap, &p, input, 1000).expect("matching payload and evidence counts");
    assert!(flag(&heap, &result));
    heap.drop_value(result).expect("result release");
    heap.finish()
        .expect("both layouts released all physical values");
}
#[test]
fn reserved_evidence_rejection_releases_valid_public_helper_owned_inputs() {
    for tag in ["$tuple", "$list", "$match"] {
        let mut heap = Heap::new();
        let payload = heap
            .literal(&Lit::String("external payload".into()))
            .expect("payload");
        let external = heap.dup(&payload).expect("external alias");
        let dict = heap
            .literal(&Lit::String("dictionary".into()))
            .expect("dictionary");
        assert_eq!(
            heap.construct_with_dicts(tag, vec![payload], vec![dict])
                .expect_err("reserved evidence"),
            Error::ReservedConstructorEvidence(tag.into())
        );
        assert_eq!(heap.stats().live_objects, 1);
        let candidate_field = heap
            .literal(&Lit::String("old candidate field".into()))
            .expect("old contents");
        let candidate = heap
            .construct("Old", vec![candidate_field])
            .expect("candidate");
        let payload = heap.dup(&external).expect("new payload root");
        let dict = heap
            .literal(&Lit::String("new dictionary".into()))
            .expect("new dictionary");
        assert_eq!(
            heap.reuse_with_dicts(candidate, tag, vec![payload], vec![dict])
                .expect_err("reserved reuse evidence"),
            Error::ReservedConstructorEvidence(tag.into())
        );
        assert_eq!(
            heap.stats().live_objects,
            1,
            "rejection frees candidate, its old contents and both new vectors"
        );
        assert_eq!(heap.reference_count(&external), Ok(1));
        heap.drop_value(external).expect("external release");
        heap.finish()
            .expect("rejected consumed inputs were explicitly dropped");
    }
}

#[test]
fn both_constructor_vectors_and_reuse_candidate_preserve_operand_multiplicities() {
    let mut heap = Heap::new();
    let input = heap
        .literal(&Lit::String("shared field and evidence".into()))
        .expect("input");
    let p = program(function(
        &[0, 1],
        vec![bind(
            2,
            Operation::ConstructWithDicts {
                tag: "T.Many".into(),
                fields: vec![1, 1],
                dicts: vec![1],
                rep: ValueRep::Cell,
                layout: None,
            },
        )],
        2,
    ));
    let result =
        run(&mut heap, &p, input, 1000).expect("all repeated constructor operands own a token");
    assert_eq!(
        heap.reference_count(heap.field(&result, 0).expect("ordinary field")),
        Ok(3)
    );
    assert_eq!(
        heap.field(&result, 0).expect("field").object_id(),
        heap.field(&result, 2).expect("evidence").object_id()
    );
    heap.drop_value(result)
        .expect("three physical aliases release");
    heap.finish()
        .expect("all constructor operand occurrences balanced");

    let body = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![
                Instruction::Drop(0),
                Instruction::Dup(1),
                Instruction::Dup(1),
                bind(
                    2,
                    Operation::ReuseWithDicts {
                        object: 1,
                        tag: "T.New".into(),
                        fields: vec![1],
                        dicts: vec![1],
                        rep: ValueRep::Cell,
                        layout: None,
                    },
                ),
            ],
            exit: Exit::Return(2),
        },
    };
    let first = heap
        .literal(&Lit::String("first old child".into()))
        .expect("child");
    let second = heap
        .literal(&Lit::String("second old child".into()))
        .expect("child");
    let candidate = heap
        .construct("T.Old", vec![first, second])
        .expect("same-capacity candidate");
    let old = candidate.object_id();
    let result = run(&mut heap, &program(body), candidate, 1000)
        .expect("three candidate/field/evidence operands");
    assert_ne!(
        result.object_id(),
        old,
        "new self-alias operands prohibit destructive reuse"
    );
    assert_eq!(
        heap.field(&result, 0)
            .expect("ordinary old object")
            .object_id(),
        old
    );
    assert_eq!(
        heap.field(&result, 1)
            .expect("evidence old object")
            .object_id(),
        old
    );
    heap.drop_value(result).expect("fallback graph release");
    heap.finish()
        .expect("candidate and all descendant tokens balanced");
}
#[test]
fn reuse_drops_every_old_evidence_field_or_preserves_shared_old_storage() {
    for shared in [false, true] {
        let mut heap = Heap::new();
        let old_dict = heap
            .literal(&Lit::String("old owned evidence".into()))
            .expect("old evidence");
        let old_dict_id = old_dict.object_id().expect("evidence identity");
        let candidate = heap
            .construct_with_dicts("T.Old", vec![Value::Int(1)], vec![old_dict])
            .expect("candidate evidence");
        let external = if shared {
            Some(heap.dup(&candidate).expect("shared candidate"))
        } else {
            None
        };
        let new = heap
            .reuse(candidate, "T.New", vec![Value::Int(2), Value::Int(3)])
            .expect("replace partition and fields");
        assert_eq!(
            heap.is_live(old_dict_id),
            shared,
            "unique replacement drops hidden old evidence too"
        );
        if let Some(external) = external {
            assert_eq!(
                heap.string(
                    heap.field(&external, 1)
                        .expect("old physical dictionary field")
                ),
                Ok("old owned evidence")
            );
            heap.drop_value(external)
                .expect("shared old storage release");
        }
        assert!(!heap.is_live(old_dict_id));
        heap.drop_value(new).expect("new storage release");
        heap.finish()
            .expect("ordinary/evidence storage released exactly once");
    }
}
