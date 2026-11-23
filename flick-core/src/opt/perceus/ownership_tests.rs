#[cfg(test)]
mod tests {
    use crate::ir::memory::{self, Block, Exit, Function, Instruction, Operation};
    use crate::ir::{Lit, Pat};

    use super::{Error, copies, insert, insert_fbip, insert_without_reuse, optimize_reuse};

    fn let_(binding: u32, value: Operation) -> Instruction {
        Instruction::Let { binding, value }
    }

    fn construct(fields: &[u32]) -> Operation {
        Operation::Construct {
            tag: "Node".into(),
            fields: fields.to_vec(),
        }
    }

    fn function(params: &[u32], instructions: Vec<Instruction>, result: u32) -> Function {
        Function {
            params: params.to_vec(),
            body: Block {
                instructions,
                exit: Exit::Return(result),
            },
        }
    }

    #[test]
    fn duplicates_repeated_fields_and_drops_unused_parameters() {
        let input = function(&[0, 1], vec![let_(2, construct(&[0, 0]))], 2);
        let output = insert(&input).expect("ownership insertion");
        assert_eq!(
            output.body.instructions,
            [
                Instruction::Drop(1),
                Instruction::Dup(0),
                let_(2, construct(&[0, 0]))
            ]
        );
        assert_eq!(memory::validate(&output), Ok(()));
    }

    #[test]
    fn releases_a_borrowed_receiver_only_after_projecting_its_field() {
        let project = Operation::Project {
            object: 0,
            field: 0,
        };
        let input = function(&[0], vec![let_(1, project.clone())], 1);
        let output = insert(&input).expect("ownership insertion");
        assert_eq!(
            output.body.instructions,
            [let_(1, project), Instruction::Drop(0)]
        );
        assert_eq!(memory::validate(&output), Ok(()));
    }

    #[test]
    fn reuses_a_last_projected_receiver_in_the_next_constructor() {
        let project = Operation::Project {
            object: 0,
            field: 0,
        };
        let input = function(
            &[0],
            vec![let_(1, project.clone()), let_(2, construct(&[1]))],
            2,
        );
        let output = insert(&input).expect("ownership insertion");
        assert_eq!(
            output.body.instructions,
            [
                let_(1, project),
                let_(
                    2,
                    Operation::Reuse {
                        object: 0,
                        tag: "Node".into(),
                        fields: vec![1]
                    }
                ),
            ]
        );
        assert_eq!(memory::validate(&output), Ok(()));
    }

    #[test]
    fn branch_specific_uses_do_not_copy_both_successors() {
        let input = Function {
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
                        exit: Exit::Return(0),
                    }),
                },
            },
        };
        let output = insert(&input).expect("ownership insertion");
        assert_eq!(output.body.instructions, [Instruction::Dup(0)]);
        let Exit::Branch { then_, else_, .. } = &output.body.exit else {
            panic!("branch")
        };
        assert_eq!(then_.instructions, [Instruction::Drop(0)]);
        assert_eq!(else_.instructions, [Instruction::Drop(1)]);
        assert_eq!(memory::validate(&output), Ok(()));
    }

    #[test]
    fn does_not_reuse_a_receiver_that_remains_live() {
        let input = function(
            &[0],
            vec![
                let_(
                    1,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                let_(2, construct(&[1])),
                let_(3, construct(&[0, 2])),
            ],
            3,
        );
        let output = insert(&input).expect("ownership insertion");
        assert!(!output.body.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::Reuse { .. },
                ..
            }
        )));
        assert_eq!(memory::validate(&output), Ok(()));
    }

    #[test]
    fn does_not_defer_drop_across_unrelated_work() {
        let input = function(
            &[0],
            vec![
                let_(
                    1,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                let_(2, Operation::Literal(Lit::Unit)),
                let_(3, construct(&[1])),
            ],
            3,
        );
        let output = insert(&input).expect("ownership insertion");
        assert_eq!(output.body.instructions[1], Instruction::Drop(0));
        assert_eq!(memory::validate(&output), Ok(()));
    }

    #[test]
    fn rejects_unbound_and_rebound_locals_and_existing_annotations() {
        assert_eq!(insert(&function(&[], vec![], 0)), Err(Error::Unbound(0)));
        assert_eq!(
            insert(&function(&[0, 0], vec![], 0)),
            Err(Error::DuplicateBinding(0))
        );
        assert_eq!(
            insert(&function(&[0], vec![let_(0, construct(&[]))], 0)),
            Err(Error::DuplicateBinding(0))
        );
        assert_eq!(
            insert(&function(&[0], vec![Instruction::Dup(0)], 0)),
            Err(Error::Annotated)
        );
        assert_eq!(
            insert(&function(
                &[0],
                vec![let_(
                    1,
                    Operation::Reuse {
                        object: 0,
                        tag: "Node".into(),
                        fields: vec![]
                    }
                )],
                1
            )),
            Err(Error::Annotated)
        );
    }

    #[test]
    fn all_local_use_counts_obey_the_lean_preparation_equation() {
        for consumed in 0..64 {
            for keep in [false, true] {
                let discarded = usize::from(consumed == 0 && !keep);
                assert_eq!(
                    1 + copies(consumed, keep),
                    consumed + usize::from(keep) + discarded
                );
            }
            let fields = vec![0; consumed];
            let output = insert(&function(&[0], vec![let_(1, construct(&fields))], 1))
                .expect("ownership insertion");
            assert_eq!(memory::validate(&output), Ok(()));
        }
    }
    #[test]
    fn match_uses_the_generic_borrowed_receiver_reuse_boundary() {
        let pattern = Pat::Wild;
        let output = insert(&function(
            &[0],
            vec![
                let_(
                    1,
                    Operation::Match {
                        object: 0,
                        pattern: Box::new(pattern.clone()),
                    },
                ),
                let_(2, construct(&[1])),
            ],
            2,
        ))
        .expect("borrowed match insertion");
        assert_eq!(
            output.body.instructions,
            [
                let_(
                    1,
                    Operation::Match {
                        object: 0,
                        pattern: Box::new(pattern),
                    }
                ),
                let_(
                    2,
                    Operation::Reuse {
                        object: 0,
                        tag: "Node".into(),
                        fields: vec![1]
                    }
                ),
            ]
        );
        assert_eq!(memory::validate(&output), Ok(()));
    }

    #[test]
    fn a_dead_match_result_drops_before_reusing_its_candidate() {
        let pattern = Pat::Bind("whole".into());
        let output = insert(&function(
            &[0],
            vec![
                let_(
                    1,
                    Operation::Match {
                        object: 0,
                        pattern: Box::new(pattern.clone()),
                    },
                ),
                let_(2, construct(&[])),
            ],
            2,
        ))
        .expect("dead match result insertion");
        assert_eq!(
            output.body.instructions,
            [
                let_(
                    1,
                    Operation::Match {
                        object: 0,
                        pattern: Box::new(pattern),
                    }
                ),
                Instruction::Drop(1),
                let_(
                    2,
                    Operation::Reuse {
                        object: 0,
                        tag: "Node".into(),
                        fields: vec![]
                    }
                ),
            ]
        );
        assert_eq!(memory::validate(&output), Ok(()));
    }
    #[test]
    fn handler_and_operation_roots_are_counted_with_multiplicity() {
        let operations = [
            Operation::Handle {
                body: 0,
                effects: vec![memory::HandlerEffect {
                    effect: "App.E".into(),
                    operations: vec![memory::HandlerOperation {
                        name: "op".into(),
                        class: flick_ir::OpClass::Tail,
                        dicts: 0,
                        arity: 1,
                        function: 0,
                    }],
                }],
                ret: Some(0),
                errors: Some(0),
            },
            Operation::Perform {
                effect: "App.E".into(),
                op: "op".into(),
                dicts: vec![0],
                args: vec![0, 0],
            },
        ];
        for operation in operations {
            let count = operation.consumed().len();
            let output = insert(&function(&[0], vec![let_(1, operation.clone())], 1))
                .expect("every handler/operation root gets an owned token");
            let mut expected = vec![Instruction::Dup(0); count - 1];
            expected.push(let_(1, operation));
            assert_eq!(output.body.instructions, expected);
            assert_eq!(memory::validate(&output), Ok(()));
        }
    }

    #[test]
    fn ownership_counts_evidence_and_defers_to_evidence_construction() {
        let output = insert(&function(
            &[0],
            vec![let_(
                1,
                Operation::ConstructWithDicts {
                    tag: "Node".into(),
                    fields: vec![0, 0],
                    dicts: vec![0],
                },
            )],
            1,
        ))
        .expect("all ordinary and evidence occurrences count");
        assert_eq!(
            output.body.instructions,
            [
                Instruction::Dup(0),
                Instruction::Dup(0),
                let_(
                    1,
                    Operation::ConstructWithDicts {
                        tag: "Node".into(),
                        fields: vec![0, 0],
                        dicts: vec![0]
                    }
                )
            ]
        );
        let output = insert(&function(
            &[0, 1],
            vec![
                let_(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                let_(
                    3,
                    Operation::ConstructWithDicts {
                        tag: "Node".into(),
                        fields: vec![2],
                        dicts: vec![1],
                    },
                ),
            ],
            3,
        ))
        .expect("last borrowed receiver defers to evidence construction");
        assert!(
            matches!(output.body.instructions.last(), Some(Instruction::Let { value: Operation::ReuseWithDicts { object: 0, fields, dicts, .. }, .. }) if fields == &[2] && dicts == &[1])
        );
        assert_eq!(memory::validate(&output), Ok(()));
        assert_eq!(
            insert(&function(
                &[0, 1, 2],
                vec![let_(
                    3,
                    Operation::ReuseWithDicts {
                        object: 0,
                        tag: "Node".into(),
                        fields: vec![1],
                        dicts: vec![2]
                    }
                )],
                3
            )),
            Err(Error::Annotated)
        );
        assert_eq!(
            insert(&function(&[0], vec![let_(1, Operation::DropReuse(0))], 1)),
            Err(Error::Annotated)
        );
        assert_eq!(
            insert(&function(
                &[0, 1],
                vec![let_(
                    2,
                    Operation::ReuseRecord {
                        object: 0,
                        fields: vec![("x".into(), 1)]
                    }
                )],
                2
            )),
            Err(Error::Annotated)
        );
    }

    #[test]
    fn insert_without_reuse_emits_zero_reuse_operations() {
        let input = function(
            &[0],
            vec![
                let_(
                    1,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                let_(2, construct(&[1])),
            ],
            2,
        );
        let baseline = insert_without_reuse(&input).expect("baseline insertion");
        assert_eq!(
            baseline.body.instructions,
            [
                let_(
                    1,
                    Operation::Project {
                        object: 0,
                        field: 0
                    }
                ),
                Instruction::Drop(0),
                let_(2, construct(&[1])),
            ]
        );
        assert_eq!(memory::validate(&baseline), Ok(()));
    }

    #[test]
    fn fbip_reuse_pairs_drop_reuse_across_intervening_work_and_records() {
        // Intervening work between Project and Construct uses DropReuse + Reuse.
        let across_work = function(
            &[0],
            vec![
                let_(
                    1,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                let_(2, Operation::Literal(Lit::Int(1))),
                let_(3, construct(&[1])),
            ],
            3,
        );
        let fbip = insert_fbip(&across_work).expect("fbip reuse across work");
        assert!(fbip.body.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::DropReuse(0),
                ..
            }
        )));
        assert!(fbip.body.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::Reuse { tag, fields, .. },
                ..
            } if tag == "Node" && fields == &[1]
        )));
        assert_eq!(memory::validate(&fbip), Ok(()));

        // Record destructuring + rebuild emits DropReuse + ReuseRecord.
        let rec_rebuild = function(
            &[0],
            vec![
                let_(
                    1,
                    Operation::Field {
                        object: 0,
                        field: "x".into(),
                    },
                ),
                let_(2, Operation::Literal(Lit::Int(1))),
                let_(
                    3,
                    Operation::Record {
                        fields: vec![("x".into(), 1), ("y".into(), 2)],
                    },
                ),
            ],
            3,
        );
        let fbip_rec = insert_fbip(&rec_rebuild).expect("fbip record rebuild");
        assert!(fbip_rec.body.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::DropReuse(0),
                ..
            }
        )));
        assert!(fbip_rec.body.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::ReuseRecord { fields, .. },
                ..
            } if fields.len() == 2
        )));
        assert_eq!(memory::validate(&fbip_rec), Ok(()));
    }

    #[test]
    fn fbip_reuse_pairs_drop_reuse_across_partial_branches() {
        // Partial branch reuse: then_ reuses via Reuse, else_ drops the reuse token immediately.
        let partial_branch = Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![let_(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                )],
                exit: Exit::Branch {
                    condition: 1,
                    then_: Box::new(Block {
                        instructions: vec![let_(3, construct(&[2]))],
                        exit: Exit::Return(3),
                    }),
                    else_: Box::new(Block {
                        instructions: vec![],
                        exit: Exit::Return(2),
                    }),
                },
            },
        };
        let inserted = insert(&partial_branch).expect("stage 1");
        let fbip_branch = optimize_reuse(&inserted).expect("fbip partial branch");
        let token_local = fbip_branch
            .body
            .instructions
            .iter()
            .find_map(|i| match i {
                Instruction::Let {
                    binding,
                    value: Operation::DropReuse(0),
                } => Some(*binding),
                _ => None,
            })
            .expect("DropReuse emitted before branch");
        let Exit::Branch { then_, else_, .. } = &fbip_branch.body.exit else {
            panic!("branch")
        };
        assert!(then_.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::Reuse { object, .. },
                ..
            } if *object == token_local
        )));
        assert_eq!(else_.instructions.first(), Some(&Instruction::Drop(token_local)));
        assert_eq!(memory::validate(&fbip_branch), Ok(()));
    }
}
