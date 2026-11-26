mod aggregate_tests {
    use std::collections::BTreeMap;

    use flick_core::ir::memory::{
        Block, CellLayout, Exit, Function, Global, Instruction, Name, Operation, Program, ValueRep,
    };
    use flick_core::ir::{BinOp, Lit, Pat, Rep};
    use flick_core::types::Constructor;

    use super::{Error, Heap, ProgramOutcome, Value, execute, execute_program_outcome, patterns, primitives};

    fn bind(binding: u32, value: Operation) -> Instruction {
        Instruction::Let { binding, value }
    }

    fn layout(capacity: usize, align: u32, ownership: &str) -> CellLayout {
        CellLayout {
            capacity,
            align,
            ownership: ownership.into(),
        }
    }

    fn con(module: &str, ty: &str, name: &str, arity: usize) -> Constructor {
        Constructor {
            module: module.into(),
            ty: ty.into(),
            name: name.into(),
            arity,
            dicts: 0,
            rep: Rep::Tagged,
            fields: vec![],
            type_args: vec![],
        }
    }

    #[test]
    fn immediate_aggregate_uses_zero_allocations_dups_and_drops() {
        let mut heap = Heap::new();
        let inner = heap
            .construct_aggregate("Main.Inner.Inner", vec![Value::Int(10), Value::Bool(true)])
            .expect("inner immediate aggregate");
        let outer = heap
            .construct_aggregate("Main.Outer.Outer", vec![inner, Value::Int(20)])
            .expect("outer immediate aggregate");
        assert!(outer.is_aggregate());
        assert_eq!(outer.object_id(), None);
        assert_eq!(heap.tag(&outer), Ok("Main.Outer.Outer"));

        let copy = heap.dup(&outer).expect("dup immediate aggregate");
        let projected = heap.project(&outer, 0).expect("project nested aggregate");
        let leaf = heap.project(&projected, 0).expect("project int field");
        assert_eq!(leaf, Value::Int(10));
        assert_eq!(
            primitives::binary(&mut heap, BinOp::Eq, &outer, &copy),
            Ok(Value::Bool(true))
        );

        heap.drop_values(vec![leaf, projected, copy, outer])
            .expect("drop immediate aggregates");
        let stats = heap.finish().expect("zero leaks");
        assert_eq!(stats.allocations, 0);
        assert_eq!(stats.frees, 0);
        assert_eq!(stats.dups, 0);
        assert_eq!(stats.drops, 0);
    }

    #[test]
    fn nested_aggregate_retains_and_releases_only_boxed_descendants() {
        let mut heap = Heap::new();
        let text = heap.literal(&Lit::String("payload".into())).expect("string");
        let inner = heap
            .construct_aggregate("Main.Inner.Inner", vec![text, Value::Int(42)])
            .expect("inner aggregate");
        let outer = heap
            .construct_aggregate("Main.Outer.Outer", vec![inner, Value::Int(7)])
            .expect("outer aggregate");
        assert_eq!(heap.stats().allocations, 1);
        assert_eq!(heap.stats().dups, 0);

        let duped = heap.dup(&outer).expect("dup outer");
        assert_eq!(heap.stats().dups, 1);

        let projected_inner = heap.project(&outer, 0).expect("project inner");
        assert_eq!(heap.stats().dups, 2);

        // Drop both outer values before reading the boxed descendant through the projection.
        heap.drop_values(vec![outer, duped]).expect("drop parents");
        assert_eq!(heap.stats().live_objects, 1);

        let projected_text = heap.project(&projected_inner, 0).expect("project string");
        assert_eq!(heap.string(&projected_text), Ok("payload"));
        assert_eq!(heap.reference_count(&projected_text), Ok(2));

        heap.drop_values(vec![projected_text, projected_inner])
            .expect("drop remaining tokens");
        let stats = heap.finish().expect("zero live objects");
        assert_eq!(stats.allocations, 1);
        assert_eq!(stats.frees, 1);
        assert_eq!(stats.live_objects, 0);
    }

    #[test]
    fn pattern_matching_nested_aggregates_borrows_without_boxing() {
        let mut heap = Heap::new();
        let text = heap.literal(&Lit::String("matched".into())).expect("string");
        let inner = heap
            .construct_aggregate("Main.Inner.Inner", vec![text, Value::Int(5)])
            .expect("inner");
        let outer = heap
            .construct_aggregate("Main.Outer.Outer", vec![inner, Value::Int(9)])
            .expect("outer");
        let pat = Pat::Con(
            con("Main", "Outer", "Outer", 2),
            vec![
                Pat::Con(
                    con("Main", "Inner", "Inner", 2),
                    vec![Pat::Bind("s".into()), Pat::Lit(Lit::Int(5))],
                ),
                Pat::Bind("n".into()),
            ],
        );
        let mut scratch = Vec::new();
        let before = heap.stats();
        let matched = patterns::execute(&mut heap, &outer, &pat, &mut 1000, &mut scratch)
            .expect("match executes");
        assert!(scratch.is_empty());
        // Only the `$match` result cell allocated; `s` retained once.
        assert_eq!(heap.stats().allocations, before.allocations + 1);
        assert_eq!(heap.stats().dups, before.dups + 1);
        assert_eq!(heap.field(&matched, 0), Ok(&Value::Bool(true)));
        assert_eq!(heap.field(&matched, 1), Ok(&Value::Int(9)));
        let bound_s = heap.project(&matched, 2).expect("bound string");
        assert_eq!(heap.string(&bound_s), Ok("matched"));

        heap.drop_values(vec![bound_s, matched, outer])
            .expect("release");
        heap.finish().expect("zero leaks");
    }

    #[test]
    fn aggregate_reuse_candidates_are_rejected_without_leaking_operands() {
        let mut heap = Heap::new();
        let s1 = heap.literal(&Lit::String("a".into())).expect("s1");
        let agg1 = heap
            .construct_aggregate("Main.Box.Box", vec![s1])
            .expect("agg1");
        assert_eq!(heap.drop_reuse(agg1), Err(Error::AggregateReuseCandidate));
        heap.finish().expect("drop_reuse releases rejected aggregate");

        let s2 = heap.literal(&Lit::String("b".into())).expect("s2");
        let s3 = heap.literal(&Lit::String("c".into())).expect("s3");
        let agg2 = heap
            .construct_aggregate("Main.Box.Box", vec![s2])
            .expect("agg2");
        assert_eq!(
            heap.reuse(agg2, "Main.Box.Box", vec![s3]),
            Err(Error::AggregateReuseCandidate)
        );
        heap.finish().expect("reuse releases candidate and fields");

        let s4 = heap.literal(&Lit::String("d".into())).expect("s4");
        let s5 = heap.literal(&Lit::String("e".into())).expect("s5");
        let agg3 = heap
            .construct_aggregate("Main.Box.Box", vec![s4])
            .expect("agg3");
        assert_eq!(
            heap.reuse_record(agg3, vec![("x".into(), s5)]),
            Err(Error::AggregateReuseCandidate)
        );
        heap.finish()
            .expect("reuse_record releases candidate and fields");
    }

    #[test]
    fn box_aggregate_unbox_borrow_and_unique_vs_shared_unbox_consume() {
        let mut heap = Heap::new();
        let text = heap.literal(&Lit::String("boxed".into())).expect("string");
        let agg = heap
            .construct_aggregate("Main.Pair.Pair", vec![text, Value::Int(11)])
            .expect("agg");
        let lay = layout(2, 8, "boxed,imm");

        // BoxAggregate transfers descendant reference with zero extra dups.
        let boxed = heap
            .box_aggregate(agg, Some(lay.clone()))
            .expect("box aggregate");
        assert_eq!(heap.cell_layout(&boxed), Ok(Some(&lay)));
        assert_eq!(heap.stats().allocations, 2);
        assert_eq!(heap.stats().dups, 0);

        // UnboxBorrow retains boxed descendant without releasing the cell.
        let borrowed = heap.unbox_borrow(&boxed).expect("unbox borrow");
        assert!(borrowed.is_aggregate());
        assert_eq!(heap.stats().dups, 1);
        assert_eq!(heap.reference_count(&boxed), Ok(1));
        heap.drop_value(borrowed).expect("drop borrowed aggregate");
        assert_eq!(heap.stats().live_objects, 2);

        // Shared UnboxConsume retains descendant first, then decrements cell refcount.
        let alias = heap.dup(&boxed).expect("external alias");
        assert_eq!(heap.reference_count(&boxed), Ok(2));
        let dups_before_shared = heap.stats().dups;
        let frees_before_shared = heap.stats().frees;
        let shared_unboxed = heap.unbox_consume(boxed).expect("shared unbox consume");
        assert_eq!(heap.stats().dups, dups_before_shared + 1);
        assert_eq!(heap.stats().frees, frees_before_shared);
        assert_eq!(heap.reference_count(&alias), Ok(1));
        let alias_check = heap.unbox_borrow(&alias).expect("alias stays intact");
        assert_eq!(
            heap.string(heap.field(&alias_check, 0).expect("field")),
            Ok("boxed")
        );
        heap.drop_values(vec![alias_check, shared_unboxed])
            .expect("drop unboxed copies");

        // Unique UnboxConsume frees the cell and moves aggregate out with zero extra descendant dups/drops.
        let stats_before_unique = heap.stats();
        let moved = heap.unbox_consume(alias).expect("unique unbox consume");
        assert_eq!(heap.stats().dups, stats_before_unique.dups);
        assert_eq!(heap.stats().drops, stats_before_unique.drops + 1);
        assert_eq!(heap.stats().frees, stats_before_unique.frees + 1);
        assert_eq!(heap.stats().live_objects, 1);
        assert_eq!(
            heap.string(heap.field(&moved, 0).expect("moved field")),
            Ok("boxed")
        );
        heap.drop_value(moved).expect("drop moved aggregate");
        heap.finish().expect("zero live objects");
    }

    #[test]
    fn typed_cell_reuse_checks_cell_layout_compatibility() {
        let layout_a = layout(2, 8, "boxed,imm");
        let layout_b = layout(2, 16, "boxed,boxed");

        // Incompatible layouts with the exact same field count fall back to drop + alloc.
        let mut heap = Heap::new();
        let s1 = heap.literal(&Lit::String("old".into())).expect("s1");
        let cell_a = heap
            .construct_with_layout("Main.A.A", vec![s1, Value::Int(1)], Some(layout_a.clone()))
            .expect("cell a");
        let tok_a = heap.drop_reuse(cell_a).expect("detach reuse token");
        assert_eq!(heap.cell_layout(&tok_a), Ok(Some(&layout_a)));
        let s2 = heap.literal(&Lit::String("new1".into())).expect("s2");
        let s3 = heap.literal(&Lit::String("new2".into())).expect("s3");
        let replaced = heap
            .reuse_with_layout(tok_a, "Main.B.B", vec![s2, s3], Some(layout_b.clone()))
            .expect("fallback reuse");
        assert_eq!(heap.stats().reuses, 0);
        assert_eq!(heap.stats().reuse_fallbacks, 1);
        assert_eq!(heap.cell_layout(&replaced), Ok(Some(&layout_b)));
        heap.drop_value(replaced).expect("drop replaced");
        heap.finish().expect("zero leaks on incompatible reuse");

        // Compatible layouts with the same field count reuse in place.
        let mut heap = Heap::new();
        let s1 = heap.literal(&Lit::String("old".into())).expect("s1");
        let cell_a = heap
            .construct_with_layout("Main.A.A", vec![s1, Value::Int(1)], Some(layout_a.clone()))
            .expect("cell a");
        let old_id = cell_a.object_id().expect("cell id");
        let tok_a = heap.drop_reuse(cell_a).expect("detach reuse token");
        let s2 = heap.literal(&Lit::String("reused".into())).expect("s2");
        let reused = heap
            .reuse_with_layout(tok_a, "Main.A2.A2", vec![s2, Value::Int(2)], Some(layout_a))
            .expect("in-place reuse");
        assert_eq!(reused.object_id(), Some(old_id));
        assert_eq!(heap.stats().reuses, 1);
        assert_eq!(heap.stats().reuse_fallbacks, 0);
        heap.drop_value(reused).expect("drop reused");
        heap.finish().expect("zero leaks on compatible reuse");
    }

    #[test]
    fn standalone_executes_box_unbox_ops() {
        let lay_a = layout(2, 8, "imm,imm");
        let func = Function {
            params: vec![0],
            body: Block {
                instructions: vec![
                    bind(1, Operation::Literal(Lit::Int(9))),
                    bind(
                        2,
                        Operation::Construct {
                            tag: "Main.Pair.Pair".into(),
                            fields: vec![0, 1],
                            rep: ValueRep::Aggregate,
                            layout: None,
                        },
                    ),
                    bind(
                        3,
                        Operation::BoxAggregate {
                            value: 2,
                            layout: Some(lay_a),
                        },
                    ),
                    bind(4, Operation::UnboxBorrow { object: 3 }),
                    Instruction::Drop(4),
                    bind(5, Operation::UnboxConsume { object: 3 }),
                    bind(
                        6,
                        Operation::Project {
                            object: 5,
                            field: 0,
                            rep: ValueRep::Aggregate,
                        },
                    ),
                    Instruction::Drop(5),
                ],
                exit: Exit::Return(6),
            },
        };
        let mut heap = Heap::new();
        let arg = heap.literal(&Lit::String("roundtrip".into())).expect("arg");
        let res = execute(&mut heap, &func, vec![arg]).expect("standalone box/unbox");
        assert_eq!(heap.string(&res), Ok("roundtrip"));
        heap.drop_value(res).expect("drop result");
        heap.finish().expect("zero leaks in standalone");
    }

    #[test]
    fn program_executes_incompatible_layout_reuse_fallback() {
        let lay_a = layout(2, 8, "imm,imm");
        let lay_b = layout(2, 16, "imm,imm");
        let entry = Name::new("Main", "main");
        let program = Program {
            globals: vec![Global {
                name: entry.clone(),
                initializer: 0,
            }],
            functions: BTreeMap::from([(
                0,
                Function {
                    params: vec![],
                    body: Block {
                        instructions: vec![
                            bind(0, Operation::Literal(Lit::Int(1))),
                            bind(1, Operation::Literal(Lit::Int(2))),
                            bind(
                                2,
                                Operation::Construct {
                                    tag: "Main.A.A".into(),
                                    fields: vec![0, 1],
                                    rep: ValueRep::Cell,
                                    layout: Some(lay_a),
                                },
                            ),
                            bind(3, Operation::DropReuse(2)),
                            bind(4, Operation::Literal(Lit::Int(3))),
                            bind(5, Operation::Literal(Lit::Int(4))),
                            bind(
                                6,
                                Operation::Reuse {
                                    object: 3,
                                    tag: "Main.B.B".into(),
                                    fields: vec![4, 5],
                                    rep: ValueRep::Cell,
                                    layout: Some(lay_b),
                                },
                            ),
                            bind(
                                7,
                                Operation::Project {
                                    object: 6,
                                    field: 0,
                                    rep: ValueRep::Cell,
                                },
                            ),
                            Instruction::Drop(6),
                        ],
                        exit: Exit::Return(7),
                    },
                },
            )]),
        };
        let mut heap = Heap::new();
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![], 10_000);
        assert!(matches!(report.outcome, ProgramOutcome::Returned(Value::Int(3))));
        report.outcome.release(&mut heap).expect("release");
        let stats = heap.finish().expect("zero leaks in program");
        assert_eq!(stats.reuses, 0);
        assert_eq!(stats.reuse_fallbacks, 1);
    }
}
