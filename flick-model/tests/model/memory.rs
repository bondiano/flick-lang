//! Executes the shared memory annotations, not the source-corpus evaluator.

use flick_core::ir::Lit;
use flick_core::ir::memory::{Block, Exit, Function, Instruction, Operation};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, Value, execute};

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn function(params: Vec<u32>, instructions: Vec<Instruction>, result: u32) -> Function {
    Function {
        params,
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    }
}

#[test]
fn shared_fields_have_real_counts_and_the_caller_releases_the_result() {
    let mut heap = Heap::new();
    let child = heap.literal(&Lit::String("shared".into())).expect("string");
    let f = function(
        vec![0],
        vec![
            Instruction::Dup(0),
            bind(
                1,
                Operation::Construct {
                    tag: "Pair".into(),
                    fields: vec![0, 0],
                },
            ),
        ],
        1,
    );
    let result = execute(&mut heap, &f, vec![child]).expect("balanced execution");
    assert_eq!(heap.tag(&result), Ok("Pair"));
    assert_eq!(heap.stats().live_objects, 2);
    assert_eq!(
        heap.reference_count(heap.field(&result, 0).expect("field")),
        Ok(2)
    );
    let projected = heap.project(&result, 0).expect("owned field");
    assert_eq!(heap.string(&projected), Ok("shared"));
    assert_eq!(heap.reference_count(&projected), Ok(3));
    heap.drop_value(projected).expect("projected token");
    assert_eq!(heap.finish(), Err(Error::LiveObjects(2)));
    heap.drop_value(result).expect("caller releases result");
    let stats = heap.finish().expect("no sweep is needed");
    assert_eq!(stats.allocations, 2);
    assert_eq!(stats.frees, 2);
    assert_eq!(stats.dups, 2);
    assert_eq!(stats.drops, 4);
}

#[test]
fn both_branches_transfer_only_the_selected_tokens() {
    for yes in [true, false] {
        let mut heap = Heap::new();
        let left = heap.literal(&Lit::String("left".into())).expect("left");
        let right = heap.literal(&Lit::String("right".into())).expect("right");
        let f = Function {
            params: vec![0, 1, 2],
            body: Block {
                instructions: vec![],
                exit: Exit::Branch {
                    condition: 0,
                    then_: Box::new(Block {
                        instructions: vec![Instruction::Drop(2)],
                        exit: Exit::Return(1),
                    }),
                    else_: Box::new(Block {
                        instructions: vec![Instruction::Drop(1)],
                        exit: Exit::Return(2),
                    }),
                },
            },
        };
        let result =
            execute(&mut heap, &f, vec![Value::Bool(yes), left, right]).expect("selected path");
        assert_eq!(heap.string(&result), Ok(if yes { "left" } else { "right" }));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(result).expect("result");
        assert_eq!(heap.finish().expect("balanced").frees, 2);
    }
}

#[test]
fn unique_same_capacity_reuse_keeps_the_allocation() {
    let mut heap = Heap::new();
    let old = heap.construct("Old", vec![Value::Int(1)]).expect("object");
    let old_id = old.object_id().expect("object id");
    let f = function(
        vec![0],
        vec![
            bind(1, Operation::Literal(Lit::Int(2))),
            bind(
                2,
                Operation::Reuse {
                    object: 0,
                    tag: "New".into(),
                    fields: vec![1],
                },
            ),
        ],
        2,
    );
    let result = execute(&mut heap, &f, vec![old]).expect("reuse");
    assert_eq!(result.object_id(), Some(old_id));
    assert_eq!(heap.tag(&result), Ok("New"));
    assert_eq!(heap.field(&result, 0), Ok(&Value::Int(2)));
    assert_eq!(heap.stats().allocations, 1);
    assert_eq!(heap.stats().reuses, 1);
    heap.drop_value(result).expect("result");
    assert_eq!(heap.finish().expect("balanced").frees, 1);
}

#[test]
fn aliased_candidate_falls_back_without_mutating_the_alias() {
    let mut heap = Heap::new();
    let old = heap.construct("Old", vec![]).expect("object");
    let alias = heap.dup(&old).expect("alias");
    let old_id = old.object_id().expect("old id");
    let f = function(
        vec![0],
        vec![bind(
            1,
            Operation::Reuse {
                object: 0,
                tag: "New".into(),
                fields: vec![],
            },
        )],
        1,
    );
    let result = execute(&mut heap, &f, vec![old]).expect("fallback");
    assert_ne!(result.object_id(), Some(old_id));
    assert_eq!(heap.tag(&alias), Ok("Old"));
    assert_eq!(heap.tag(&result), Ok("New"));
    assert_eq!(heap.stats().allocations, 2);
    assert_eq!(heap.stats().reuses, 0);
    heap.drop_value(alias).expect("alias");
    heap.drop_value(result).expect("result");
    assert_eq!(heap.finish().expect("balanced").frees, 2);
}

#[test]
fn incompatible_capacity_and_string_candidates_fall_back() {
    for string in [false, true] {
        let mut heap = Heap::new();
        let old = if string {
            heap.literal(&Lit::String("old".into())).expect("string")
        } else {
            heap.construct("Old", vec![]).expect("object")
        };
        let old_id = old.object_id();
        let result = heap.reuse(old, "New", vec![Value::Unit]).expect("fallback");
        assert_ne!(result.object_id(), old_id);
        assert_eq!(heap.stats().allocations, 2);
        assert_eq!(heap.stats().frees, 1);
        assert_eq!(heap.stats().reuses, 0);
        heap.drop_value(result).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn reuse_releases_old_children_but_keeps_explicitly_shared_fields() {
    let mut heap = Heap::new();
    let child = heap.literal(&Lit::String("kept".into())).expect("child");
    let parent = heap.construct("Old", vec![child]).expect("parent");
    let projected = heap.project(&parent, 0).expect("owned child");
    let result = heap.reuse(parent, "New", vec![projected]).expect("reuse");
    assert_eq!(
        heap.reference_count(heap.field(&result, 0).expect("field")),
        Ok(1)
    );
    assert_eq!(heap.stats().live_objects, 2);
    assert_eq!(heap.stats().allocations, 2);
    assert_eq!(heap.stats().reuses, 1);
    heap.drop_value(result).expect("result");
    assert_eq!(heap.finish().expect("balanced").frees, 2);
}

#[test]
fn deep_destruction_uses_an_explicit_worklist() {
    let mut heap = Heap::new();
    let mut value = Value::Unit;
    for _ in 0..100_000 {
        value = heap.construct("Node", vec![value]).expect("node");
    }
    assert_eq!(heap.stats().live_objects, 100_000);
    heap.drop_value(value).expect("iterative destruction");
    let stats = heap.finish().expect("balanced");
    assert_eq!(stats.frees, 100_000);
    assert_eq!(stats.peak_live_objects, 100_000);
}

#[test]
fn stale_and_foreign_handles_never_alias_a_new_allocation() {
    let mut heap = Heap::new();
    let old = heap.construct("Old", vec![]).expect("old");
    let old_id = old.object_id().expect("id");
    heap.drop_value(old).expect("release");
    let current = heap.construct("Current", vec![]).expect("current");
    assert_ne!(current.object_id(), Some(old_id));
    assert!(!heap.is_live(old_id));
    assert!(heap.is_live(current.object_id().expect("current identity")));
    let mut other_heap = Heap::new();
    assert!(matches!(
        other_heap.dup(&current),
        Err(Error::InvalidObject(_))
    ));
    heap.drop_value(current).expect("current");
    assert!(heap.finish().is_ok());
    assert!(other_heap.finish().is_ok());
}

#[test]
fn malformed_annotations_are_rejected_before_execution() {
    let mut heap = Heap::new();
    let f = function(
        vec![],
        vec![
            bind(0, Operation::Literal(Lit::String("not allocated".into()))),
            Instruction::Dup(0),
        ],
        0,
    );
    assert!(matches!(
        execute(&mut heap, &f, vec![]),
        Err(Error::InvalidFunction(_))
    ));
    assert_eq!(heap.stats().allocations, 0);
    assert!(heap.finish().is_ok());
}

#[test]
fn invalid_dynamic_shapes_do_not_masquerade_as_success() {
    let mut heap = Heap::new();
    assert!(matches!(
        heap.project(&Value::Unit, 0),
        Err(Error::ExpectedObject)
    ));
    let object = heap.construct("Empty", vec![]).expect("object");
    assert!(matches!(
        heap.project(&object, 0),
        Err(Error::MissingField { .. })
    ));
    heap.drop_value(object).expect("object");
    assert!(heap.finish().is_ok());
}

#[test]
fn inserted_last_project_reuse_runs_without_an_extra_allocation() {
    let mut heap = Heap::new();
    let child = heap.literal(&Lit::String("kept".into())).expect("child");
    let old = heap.construct("Old", vec![child]).expect("old");
    let old_id = old.object_id();
    let f = function(
        vec![0],
        vec![
            bind(
                1,
                Operation::Project {
                    object: 0,
                    field: 0,
                },
            ),
            bind(
                2,
                Operation::Construct {
                    tag: "New".into(),
                    fields: vec![1],
                },
            ),
        ],
        2,
    );
    let annotated = perceus::insert(&f).expect("insert ownership");
    let result = execute(&mut heap, &annotated, vec![old]).expect("execute annotations");
    assert_eq!(result.object_id(), old_id);
    assert_eq!(heap.tag(&result), Ok("New"));
    assert_eq!(
        heap.string(heap.field(&result, 0).expect("field")),
        Ok("kept")
    );
    assert_eq!(heap.stats().allocations, 2);
    assert_eq!(heap.stats().reuses, 1);
    heap.drop_value(result).expect("result");
    assert_eq!(heap.finish().expect("balanced").frees, 2);
}

#[test]
fn inserted_reuse_with_aliased_parameters_preserves_the_old_value() {
    let mut heap = Heap::new();
    let child = heap.literal(&Lit::String("kept".into())).expect("child");
    let old = heap
        .construct("Old", vec![child, Value::Unit])
        .expect("old");
    let alias = heap.dup(&old).expect("aliased parameter");
    let old_id = old.object_id();
    let f = function(
        vec![0, 3],
        vec![
            bind(
                1,
                Operation::Project {
                    object: 0,
                    field: 0,
                },
            ),
            bind(
                2,
                Operation::Construct {
                    tag: "New".into(),
                    fields: vec![1, 3],
                },
            ),
        ],
        2,
    );
    let annotated = perceus::insert(&f).expect("insert ownership");
    let result = execute(&mut heap, &annotated, vec![old, alias]).expect("execute annotations");
    assert_ne!(result.object_id(), old_id);
    let retained = heap.field(&result, 1).expect("old alias retained");
    assert_eq!(retained.object_id(), old_id);
    assert_eq!(heap.tag(retained), Ok("Old"));
    assert_eq!(heap.field(retained, 1), Ok(&Value::Unit));
    assert_eq!(
        heap.string(heap.field(retained, 0).expect("old child")),
        Ok("kept")
    );
    assert_eq!(heap.stats().allocations, 3);
    assert_eq!(heap.stats().reuses, 0);
    heap.drop_value(result).expect("result");
    assert_eq!(heap.finish().expect("balanced").frees, 3);
}

#[test]
fn inserted_nested_branches_and_repeated_fields_balance_on_all_paths() {
    let returned = |local| {
        Box::new(Block {
            instructions: vec![],
            exit: Exit::Return(local),
        })
    };
    let f = Function {
        params: vec![0, 1, 2, 3],
        body: Block {
            instructions: vec![bind(
                4,
                Operation::Construct {
                    tag: "Pair".into(),
                    fields: vec![2, 2],
                },
            )],
            exit: Exit::Branch {
                condition: 0,
                then_: Box::new(Block {
                    instructions: vec![],
                    exit: Exit::Branch {
                        condition: 1,
                        then_: returned(4),
                        else_: returned(3),
                    },
                }),
                else_: returned(3),
            },
        },
    };
    let annotated = perceus::insert(&f).expect("insert ownership");
    for outer in [false, true] {
        for inner in [false, true] {
            let mut heap = Heap::new();
            let child = heap.literal(&Lit::String("shared".into())).expect("child");
            let other = heap.literal(&Lit::String("other".into())).expect("other");
            let result = execute(
                &mut heap,
                &annotated,
                vec![Value::Bool(outer), Value::Bool(inner), child, other],
            )
            .expect("path executes");
            if outer && inner {
                assert_eq!(heap.tag(&result), Ok("Pair"));
                assert_eq!(
                    heap.string(heap.field(&result, 0).expect("field")),
                    Ok("shared")
                );
                assert_eq!(
                    heap.reference_count(heap.field(&result, 1).expect("field")),
                    Ok(2)
                );
            } else {
                assert_eq!(heap.string(&result), Ok("other"));
            }
            heap.drop_value(result).expect("result");
            let stats = heap.finish().expect("every path releases all objects");
            assert_eq!(stats.allocations, 3);
            assert_eq!(stats.frees, 3);
        }
    }
}

#[test]
fn reused_candidate_as_a_field_uses_the_alias_fallback_not_a_self_cycle() {
    let mut heap = Heap::new();
    let old = heap.construct("Old", vec![Value::Unit]).expect("old");
    let old_id = old.object_id();
    let alias = heap.dup(&old).expect("owned field alias");
    let result = heap.reuse(old, "New", vec![alias]).expect("fallback");
    assert_ne!(result.object_id(), old_id);
    assert_eq!(
        heap.field(&result, 0).expect("old value").object_id(),
        old_id
    );
    assert_eq!(heap.stats().reuses, 0);
    heap.drop_value(result).expect("result");
    assert!(heap.finish().is_ok());
}

#[test]
fn invalid_dynamic_condition_preserves_evidence_of_unreleased_objects() {
    let mut heap = Heap::new();
    let condition = heap.construct("NotBool", vec![]).expect("bad condition");
    let f = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![],
            exit: Exit::Branch {
                condition: 0,
                then_: Box::new(Block {
                    instructions: vec![],
                    exit: Exit::Return(1),
                }),
                else_: Box::new(Block {
                    instructions: vec![],
                    exit: Exit::Return(1),
                }),
            },
        },
    };
    assert_eq!(
        execute(&mut heap, &f, vec![condition, Value::Unit]),
        Err(Error::ExpectedBoolean)
    );
    assert_eq!(heap.finish(), Err(Error::LiveObjects(1)));
    assert_eq!(heap.stats().frees, 0);
}

#[test]
fn bigint_payloads_are_counted_without_narrowing_their_literal_spelling() {
    let mut heap = Heap::new();
    let digits = "12345678901234567890123456789012345678901234567890";
    let value = heap.literal(&Lit::BigInt(digits.into())).expect("BigInt");
    assert_eq!(heap.bigint(&value), Ok(digits));
    let alias = heap.dup(&value).expect("alias");
    assert_eq!(heap.reference_count(&value), Ok(2));
    heap.drop_value(value).expect("first token");
    assert_eq!(heap.bigint(&alias), Ok(digits));
    assert_eq!(heap.stats().live_objects, 1);
    heap.drop_value(alias).expect("last token");
    let stats = heap.finish().expect("balanced");
    assert_eq!(stats.allocations, 1);
    assert_eq!(stats.frees, 1);
}

#[test]
fn arguments_require_the_declared_arity() {
    let mut heap = Heap::new();
    let identity = function(vec![0], vec![], 0);
    assert_eq!(
        execute(&mut heap, &identity, vec![]),
        Err(Error::ArgumentCount {
            expected: 1,
            actual: 0
        })
    );
    assert!(heap.finish().is_ok());
}

#[test]
fn an_unselected_malformed_branch_is_still_rejected_before_running() {
    let mut heap = Heap::new();
    let f = Function {
        params: vec![0, 1],
        body: Block {
            instructions: vec![],
            exit: Exit::Branch {
                condition: 0,
                then_: Box::new(Block {
                    instructions: vec![],
                    exit: Exit::Return(1),
                }),
                else_: Box::new(Block {
                    instructions: vec![Instruction::Dup(1)],
                    exit: Exit::Return(1),
                }),
            },
        },
    };
    assert!(matches!(
        execute(&mut heap, &f, vec![Value::Bool(true), Value::Unit]),
        Err(Error::InvalidFunction(_))
    ));
    assert_eq!(heap.stats().allocations, 0);
    assert!(heap.finish().is_ok());
}
