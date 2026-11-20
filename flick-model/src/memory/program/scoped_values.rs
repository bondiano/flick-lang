impl Machine<'_, '_> {
    fn scoped_intrinsic(&mut self, kind: Intrinsic, binding: Local) -> Result<(), Error> {
        match kind {
            Intrinsic::ScopeRun => {
                let previous = self.current_owner;
                let owner = self.owners.len();
                self.owners.push(OwnerState::default());
                self.current_owner = owner;
                self.current_evidence = Some(OwnerEvidence::from_owner(&self.owner_handle(owner)));
                self.scratch.push(Value::Owner(self.owner_handle(owner)));
                self.apply(Destination::CloseScope {
                    owner,
                    previous,
                    binding: Some(binding),
                })?;
            }
            Intrinsic::ScopeCurrent => {
                let value = Value::Owner(self.owner_handle(self.current_owner));
                self.primitive_result(binding, value)?;
            }
            Intrinsic::ArenaMake => {
                if self.owners[self.current_owner].closed {
                    return self.raise_defect(DefectSpec::arena_owner_closed());
                }
                let root = self.heap.allocate(Data::Arena {
                    execution: self.execution,
                    contents: vec![],
                });
                let id = root.object_id().expect("arena allocation");
                self.owners[self.current_owner].arenas.push(root);
                let value = Value::Arena(ArenaHandle {
                    id,
                    execution: self.execution,
                });
                self.primitive_result(binding, value)?;
            }
            Intrinsic::ArenaAlloc => {
                let Value::Arena(arena) = &self.scratch[0] else {
                    return Err(Error::ExpectedArena);
                };
                let handle = ArenaHandle {
                    id: arena.id,
                    execution: arena.execution,
                };
                let contents = match self.heap.arena_contents_mut(self.execution, &handle) {
                    Ok(contents) => contents,
                    Err(Error::ClosedArena) => {
                        return self.raise_defect(DefectSpec::arena_closed());
                    }
                    Err(error) => return Err(error),
                };
                let index = contents.len();
                contents.push(self.scratch.pop().expect("arena slot contents"));
                let value = Value::ArenaView(ArenaView {
                    arena: handle,
                    index,
                });
                self.primitive_result(binding, value)?;
            }
            Intrinsic::ArenaGet => {
                let Value::ArenaView(view) = &self.scratch[0] else {
                    return Err(Error::ExpectedArenaView);
                };
                let contents = match self.heap.arena_contents(self.execution, &view.arena) {
                    Ok(contents) => contents,
                    Err(Error::ClosedArena) => {
                        return self.raise_defect(DefectSpec::arena_closed());
                    }
                    Err(error) => return Err(error),
                };
                let current = contents
                    .get(view.index)
                    .ok_or(Error::MissingArenaSlot(view.index))?
                    .representation();
                let value = self.heap.dup(&current)?;
                self.primitive_result(binding, value)?;
            }
            _ => unreachable!("scoped intrinsic dispatch"),
        }
        Ok(())
    }

    fn modify(&mut self, binding: Local) -> Result<(), Error> {
        let Value::Cell(cell) = &self.scratch[0] else {
            return Err(Error::ExpectedCell);
        };
        let current = self
            .heap
            .cell_contents(self.execution, cell)?
            .representation();
        let old = self.heap.dup(&current)?;
        let mut operands = mem::take(&mut self.scratch).into_iter();
        let Value::Cell(cell) = operands.next().expect("validated cell") else {
            unreachable!("cell kind checked")
        };
        self.scratch.push(operands.next().expect("modify callback"));
        self.scratch.push(old);
        self.apply(Destination::Modify { cell, binding })
    }

    fn complete_modify(&mut self, cell: &CellHandle, binding: Local) -> Result<(), Error> {
        let returned = self.pending.as_ref().expect("callback outcome");
        let Value::Object(token) = returned else {
            return Err(Error::ExpectedModifyPair);
        };
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedModifyPair);
        };
        tagged_boundary(tag, *ordinary_fields, fields.len())?;
        if tag != "$tuple" || *ordinary_fields != 2 || fields.len() != 2 {
            return Err(Error::ExpectedModifyPair);
        }
        let result = self.heap.project(returned, 0)?;
        self.scratch.push(result);
        let replacement = self
            .heap
            .project(self.pending.as_ref().expect("callback pair"), 1)?;
        self.scratch.push(replacement);
        let contents = self.heap.cell_contents_mut(self.execution, cell)?;
        let replacement = self.scratch.pop().expect("new contents");
        let old = mem::replace(contents, replacement);
        self.scratch.push(old);
        self.scratch
            .push(self.pending.take().expect("callback pair"));
        // The result stays rooted while pair fields and old contents are
        // dropped. Tuple convention is (result, new contents), not reversed.
        self.pending = Some(self.scratch.remove(0));
        self.release_operands()?;
        let result = self.pending.take().expect("modify result");
        self.store(binding, result)?;
        Ok(())
    }
}
