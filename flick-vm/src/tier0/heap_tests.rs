#[cfg(test)]
mod tests {
    use super::{CellHandle, Data, Error, Heap, Lit, Value, execute};
    use flick_core::ir::memory::{Block, Exit, Function, Instruction, Operation};

    #[test]
    fn a_cached_zero_cannot_hide_an_occupied_allocation() {
        let mut heap = Heap::new();
        let value = heap.construct("Live", vec![]).expect("object");
        heap.stats.live_objects = 0;
        assert_eq!(
            heap.finish(),
            Err(Error::LiveCountMismatch {
                tracked: 0,
                actual: 1
            })
        );
        heap.stats.live_objects = 1;
        heap.drop_value(value).expect("explicit release");
        assert!(heap.finish().is_ok());
    }

    #[test]
    fn a_nonowning_cell_handle_does_not_hide_or_release_arena_storage() {
        let mut heap = Heap::new();
        let value = heap
            .literal(&Lit::String("contents".into()))
            .expect("value");
        let root = heap.allocate(Data::Cell {
            execution: 0,
            contents: Some(value),
        });
        let id = root.object_id().expect("root identity");
        let handle = Value::Cell(CellHandle { id, execution: 0 });
        assert_eq!(heap.finish(), Err(Error::LiveObjects(2)));
        heap.drop_value(handle).expect("nonowning handle release");
        assert_eq!(heap.finish(), Err(Error::LiveObjects(2)));
        let Data::Cell { contents, .. } = &mut heap.object_mut(id).expect("cell allocation").data
        else {
            panic!("cell root");
        };
        let detached = contents.take().expect("strong contents root");
        heap.drop_value(detached).expect("detached value");
        assert_eq!(heap.finish(), Err(Error::LiveObjects(1)));
        heap.drop_value(root).expect("arena owner release");
        let stats = heap.finish().expect("no live storage");
        assert_eq!(stats.cells_allocated, 1);
        assert_eq!(stats.cells_freed, 1);
    }
    #[test]
    fn private_stale_handle_injection_is_rejected() {
        let mut heap = Heap::new();
        let old = heap.construct("Old", vec![]).expect("old");
        // Only private model internals can manufacture this uncounted copy.
        let stale = old.representation();
        let old_id = old.object_id().expect("old id");
        heap.drop_value(old).expect("release old");
        let current = heap.construct("Current", vec![]).expect("new allocation");
        assert_ne!(current.object_id(), Some(old_id));
        assert_eq!(heap.dup(&stale), Err(Error::InvalidObject(old_id)));
        heap.drop_value(current).expect("release current");
        assert!(heap.finish().is_ok());
    }

    #[test]
    fn private_argument_token_forgery_does_not_pass_reference_validation() {
        let mut heap = Heap::new();
        let pair = Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 2,
                    value: Operation::Construct {
                        tag: "Pair".into(),
                        fields: vec![0, 1],
                    },
                }],
                exit: Exit::Return(2),
            },
        };
        let value = heap.construct("OneToken", vec![]).expect("object");
        let id = value.object_id().expect("id");
        // Test the defensive dynamic check by deliberately bypassing the
        // public API's unforgeable-token boundary inside this private module.
        let forged = value.representation();
        let original = value.representation();
        assert_eq!(
            execute(&mut heap, &pair, vec![value, forged]),
            Err(Error::InsufficientReferences {
                object: id,
                required: 2,
                actual: 1
            })
        );
        assert_eq!(heap.stats().allocations, 1);
        assert_eq!(heap.stats().frees, 0);
        heap.drop_value(original)
            .expect("explicitly recover original token");
        assert!(heap.finish().is_ok());
    }

    #[test]
    fn moving_an_owned_token_out_and_back_does_not_duplicate_it() {
        let mut heap = Heap::new();
        let Value::Object(token) = heap.construct("Value", vec![]).expect("object") else {
            panic!("construct returns an object")
        };
        let value = Value::Object(token);
        assert_eq!(heap.reference_count(&value), Ok(1));
        heap.drop_value(value).expect("original owned token");
        assert!(heap.finish().is_ok());
    }
    #[test]
    fn invalid_internal_boundaries_return_errors_without_slicing_or_losing_fields() {
        use super::{patterns, primitives};
        use flick_core::ir::{BinOp, Pat};
        let mut heap = Heap::new();
        let input = heap.allocate(Data::Tagged {
            tag: "T.Box".into(),
            ordinary_fields: 2,
            fields: vec![Value::Int(1)],
        });
        let expected = || Error::InvalidTaggedBoundary {
            ordinary: 2,
            physical: 1,
        };
        for op in [BinOp::Eq, BinOp::NotEq, BinOp::Same, BinOp::NotSame] {
            assert_eq!(
                primitives::binary(&mut heap, op, &input, &input),
                Err(expected())
            );
        }
        assert_eq!(heap.field(&input, 0), Err(expected()));
        assert_eq!(heap.project_named(&input, "_tag"), Err(expected()));
        heap.drop_value(input)
            .expect("invalid boundary still drops all physical fields");
        for tag in ["$tuple", "$list", "$match"] {
            let evidence = heap
                .literal(&Lit::String("owned hidden value".into()))
                .expect("evidence");
            let input = heap.allocate(Data::Tagged {
                tag: tag.into(),
                ordinary_fields: 1,
                fields: vec![Value::Int(1), evidence],
            });
            let expected = || Error::ReservedConstructorEvidence(tag.into());
            assert_eq!(heap.field(&input, 0), Err(expected()));
            let pattern = if tag == "$list" {
                Pat::List(vec![Pat::Wild, Pat::Wild], None)
            } else {
                Pat::Tuple(vec![Pat::Wild, Pat::Wild])
            };
            let mut scratch = vec![];
            assert_eq!(
                patterns::execute(&mut heap, &input, &pattern, &mut 1000, &mut scratch),
                Err(expected())
            );
            assert!(scratch.is_empty());
            if tag == "$list" {
                assert_eq!(
                    primitives::binary(&mut heap, BinOp::ListConcat, &input, &input),
                    Err(expected())
                );
            }
            heap.drop_value(input)
                .expect("corrupted metadata does not truncate owned evidence on drop");
            heap.finish()
                .expect("all physical fields released even for rejected internal layout");
        }
    }
}

#[cfg(test)]
mod service_tag_tests {
    use super::*;
    #[test]
    fn weak_service_index_retires_exact_last_identity() {
        let mut heap = Heap::new();
        let tag = heap.service_tag("key").expect("tag");
        let id = tag.object_id().expect("identity");
        let alias = heap.dup(&tag).expect("alias");
        assert_eq!(heap.service_tags.get("key"), Some(&id));
        heap.drop_value(tag).expect("release");
        assert_eq!(heap.service_tags.get("key"), Some(&id));
        heap.drop_value(alias).expect("last release");
        assert!(
            heap.service_tags.is_empty(),
            "weak entries are removed, not swept"
        );
        heap.finish().expect("no hidden tag owner");
    }
}
