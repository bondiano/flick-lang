#[cfg(test)]
mod aggregate_tests {
    use crate::ir::Lit;
    use crate::ir::memory::{self, Block, CellLayout, Exit, Function, Instruction, Operation, ValueRep};

    use super::{insert, insert_fbip};

    fn let_(binding: u32, value: Operation) -> Instruction {
        Instruction::Let { binding, value }
    }

    fn construct(fields: &[u32]) -> Operation {
        Operation::Construct {
            tag: "Node".into(),
            fields: fields.to_vec(),
            rep: ValueRep::Cell,
            layout: None,
        }
    }

    fn construct_agg(tag: &str, fields: &[u32]) -> Operation {
        Operation::Construct {
            tag: tag.into(),
            fields: fields.to_vec(),
            rep: ValueRep::Aggregate,
            layout: None,
        }
    }

    fn project(object: u32, field: usize) -> Operation {
        Operation::Project {
            object,
            field,
            rep: ValueRep::Cell,
        }
    }

    fn project_agg(object: u32, field: usize) -> Operation {
        Operation::Project {
            object,
            field,
            rep: ValueRep::Aggregate,
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
    fn aggregate_receivers_and_constructors_never_defer_or_pair_for_reuse() {
        let agg_then_cell = function(
            &[0],
            vec![let_(1, project_agg(0, 0)), let_(2, construct(&[1]))],
            2,
        );
        let stage1 = insert(&agg_then_cell).expect("stage 1 aggregate");
        assert_eq!(
            stage1.body.instructions,
            [
                let_(1, project_agg(0, 0)),
                Instruction::Drop(0),
                let_(2, construct(&[1])),
            ]
        );
        let fbip = insert_fbip(&agg_then_cell).expect("fbip aggregate");
        assert!(!fbip.body.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::DropReuse(_) | Operation::Reuse { .. },
                ..
            }
        )));
        assert_eq!(memory::validate(&fbip), Ok(()));

        let cell_then_agg = function(
            &[0],
            vec![
                let_(1, project(0, 0)),
                let_(2, construct_agg("App.Agg.Agg", &[1])),
            ],
            2,
        );
        let fbip_cell_agg = insert_fbip(&cell_then_agg).expect("fbip cell then agg");
        assert_eq!(
            fbip_cell_agg.body.instructions,
            [
                let_(1, project(0, 0)),
                Instruction::Drop(0),
                let_(2, construct_agg("App.Agg.Agg", &[1])),
            ]
        );
        assert_eq!(memory::validate(&fbip_cell_agg), Ok(()));

        let constructed_agg_then_cell = function(
            &[0],
            vec![
                let_(1, construct_agg("App.Agg.Agg", &[0])),
                let_(2, project_agg(1, 0)),
                let_(3, Operation::Literal(Lit::Int(1))),
                let_(4, construct(&[2])),
            ],
            4,
        );
        let fbip_no_reuse = insert_fbip(&constructed_agg_then_cell).expect("fbip no agg reuse");
        assert!(!fbip_no_reuse.body.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::DropReuse(_) | Operation::Reuse { .. },
                ..
            }
        )));
        assert_eq!(memory::validate(&fbip_no_reuse), Ok(()));
    }

    #[test]
    fn box_aggregate_and_unbox_operations_insert_balanced_tokens_without_reuse() {
        let layout = Some(CellLayout {
            capacity: 1,
            align: 8,
            ownership: "boxed".into(),
        });
        let input = function(
            &[0],
            vec![
                let_(1, construct_agg("App.Wrap.Wrap", &[0])),
                let_(
                    2,
                    Operation::BoxAggregate {
                        value: 1,
                        layout: layout.clone(),
                    },
                ),
                let_(3, Operation::UnboxBorrow { object: 2 }),
                let_(4, Operation::UnboxConsume { object: 2 }),
                let_(5, construct(&[3, 4])),
            ],
            5,
        );
        let output = insert_fbip(&input).expect("box/unbox insertion");
        assert!(!output.body.instructions.iter().any(|i| matches!(
            i,
            Instruction::Let {
                value: Operation::DropReuse(_) | Operation::Reuse { .. },
                ..
            }
        )));
        assert_eq!(memory::validate(&output), Ok(()));

        let unbox_then_cell = function(
            &[0],
            vec![
                let_(1, Operation::UnboxBorrow { object: 0 }),
                let_(2, construct(&[1])),
            ],
            2,
        );
        let out_unbox = insert_fbip(&unbox_then_cell).expect("unbox borrow then cell");
        assert_eq!(
            out_unbox.body.instructions,
            [
                let_(1, Operation::UnboxBorrow { object: 0 }),
                Instruction::Drop(0),
                let_(2, construct(&[1])),
            ]
        );
        assert_eq!(memory::validate(&out_unbox), Ok(()));
    }
}
