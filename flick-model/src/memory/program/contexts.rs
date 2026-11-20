impl Machine<'_, '_> {
    fn within(
        &mut self,
        binding: Local,
        owner: Local,
        code: CodeId,
        args: &[Local],
    ) -> Result<(), Error> {
        self.stage([owner].into_iter().chain(args.iter().copied()))?;
        let selected = self.owner_index(&self.scratch[0])?;
        self.scratch.remove(0);
        let previous = self.current_owner;
        self.current_owner = selected;
        self.call(code, Destination::RestoreOwner { previous, binding })
    }

    fn capture(
        &mut self,
        binding: Local,
        code: CodeId,
        captures: &[Local],
        task: bool,
    ) -> Result<(), Error> {
        self.stage(captures.iter().copied())?;
        let execution = self.execution;
        let captures = mem::take(&mut self.scratch);
        let data = if task {
            Data::Task {
                execution,
                code,
                captures,
            }
        } else {
            Data::Closure {
                execution,
                code,
                captures,
            }
        };
        let value = self.heap.allocate(data);
        self.store(binding, value)?;
        Ok(())
    }

    fn intrinsic(&mut self, kind: Intrinsic, binding: Local) -> Result<(), Error> {
        if self.control_intrinsic(kind, binding)? {
            return Ok(());
        }
        match kind {
            Intrinsic::ArrayToList
            | Intrinsic::ListRange
            | Intrinsic::PrimitiveCompare
            | Intrinsic::PrimitiveHash
            | Intrinsic::IntUnsafeDiv
            | Intrinsic::IntUnsafeRem
            | Intrinsic::NumberToFloat
            | Intrinsic::ToBigInt
            | Intrinsic::FloatMin
            | Intrinsic::FloatMax
            | Intrinsic::StringLength
            | Intrinsic::RecordGet
            | Intrinsic::RecordExtend
            | Intrinsic::RecordRemove
            | Intrinsic::RecordProject
            | Intrinsic::RecordMergeDisjoint
            | Intrinsic::RecordLabels
            | Intrinsic::ExitIsSuccess
            | Intrinsic::ExitIsFailure
            | Intrinsic::ExitHasInterrupts => self.metered_primitive(kind, binding)?,
            Intrinsic::StringStartsWith | Intrinsic::StringIncludes => {
                let value = primitives::string_predicate(
                    self.heap,
                    kind,
                    &self.scratch[0],
                    &self.scratch[1],
                )?;
                self.primitive_result(binding, value)?;
            }
            Intrinsic::LayerSucceed | Intrinsic::LayerRun | Intrinsic::LayerMerge => {
                self.layer_intrinsic(kind, binding)?;
            }
            Intrinsic::Try => {
                self.pending_handler = self.scratch.pop();
                self.scratch.push(Value::Unit);
                // Keep handler rooted until apply has successfully installed a fresh frame.
                self.apply_fresh(Destination::Bind(binding))?;
                self.frames.last_mut().expect("try body frame").destination = Destination::Catch {
                    handler: self.pending_handler.take().expect("staged handler"),
                    binding,
                };
            }
            Intrinsic::Raise => self.raise_ordinary()?,
            Intrinsic::FromResult => self.unwrap_result(binding)?,
            Intrinsic::MatchFail => return Err(Error::NoMatch),
            Intrinsic::ScopeRun
            | Intrinsic::ScopeCurrent
            | Intrinsic::ArenaMake
            | Intrinsic::ArenaAlloc
            | Intrinsic::ArenaGet => self.scoped_intrinsic(kind, binding)?,
            Intrinsic::RefMake
            | Intrinsic::RefGet
            | Intrinsic::RefSet
            | Intrinsic::RefSame
            | Intrinsic::RefModify => self.ref_intrinsic(kind, binding)?,
            Intrinsic::PrintLine | Intrinsic::Show | Intrinsic::Host(_) => {
                let value =
                    primitives::intrinsic(self.heap, kind, &self.scratch[0], &mut self.stdout)?;
                self.primitive_result(binding, value)?;
            }
            Intrinsic::LazyMake
            | Intrinsic::LazyForce
            | Intrinsic::ScopeAddFinalizer
            | Intrinsic::ScopeAddFinalizerExit
            | Intrinsic::ScopeAddClose
            | Intrinsic::AcquireRelease
            | Intrinsic::Bracket
            | Intrinsic::Uninterruptible
            | Intrinsic::SeqOfList
            | Intrinsic::SeqToList => unreachable!("control intrinsics dispatched above"),
        }
        Ok(())
    }

    fn ref_intrinsic(&mut self, kind: Intrinsic, binding: Local) -> Result<(), Error> {
        match kind {
            Intrinsic::RefMake => {
                let contents = self.scratch.pop().expect("cell initial contents");
                let root = self.heap.allocate(Data::Cell {
                    execution: self.execution,
                    contents: Some(contents),
                });
                let id = root.object_id().expect("allocated cell root");
                self.cells.push(root);
                self.store(
                    binding,
                    Value::Cell(CellHandle {
                        id,
                        execution: self.execution,
                    }),
                )?;
            }
            Intrinsic::RefGet => {
                let Value::Cell(cell) = &self.scratch[0] else {
                    return Err(Error::ExpectedCell);
                };
                let current = self
                    .heap
                    .cell_contents(self.execution, cell)?
                    .representation();
                let value = self.heap.dup(&current)?;
                self.primitive_result(binding, value)?;
            }
            Intrinsic::RefSet => {
                let Value::Cell(cell) = &self.scratch[0] else {
                    return Err(Error::ExpectedCell);
                };
                let contents = self.heap.cell_contents_mut(self.execution, cell)?;
                let replacement = self.scratch.pop().expect("replacement contents");
                let old = mem::replace(contents, replacement);
                self.scratch.push(old);
                self.primitive_result(binding, Value::Unit)?;
            }
            Intrinsic::RefSame => {
                let (Value::Cell(left), Value::Cell(right)) = (&self.scratch[0], &self.scratch[1])
                else {
                    return Err(Error::ExpectedCell);
                };
                self.heap.cell_contents(self.execution, left)?;
                self.heap.cell_contents(self.execution, right)?;
                let same = left == right;
                self.primitive_result(binding, Value::Bool(same))?;
            }
            Intrinsic::RefModify => self.modify(binding)?,
            _ => unreachable!("non-ref intrinsic"),
        }
        Ok(())
    }

    fn metered_primitive(&mut self, kind: Intrinsic, binding: Local) -> Result<(), Error> {
        let value = primitives::metered_primitive(self.heap, kind, &self.scratch, &mut self.fuel)?;
        self.primitive_result(binding, value)
    }

    fn unwrap_result(&mut self, binding: Local) -> Result<(), Error> {
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedResult);
        };
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedResult);
        };
        if *ordinary_fields != 1
            || fields.len() != 1
            || !matches!(tag.as_str(), "Prelude.Result.Ok" | "Prelude.Result.Err")
        {
            return Err(Error::ExpectedResult);
        }
        let failed = tag == "Prelude.Result.Err";
        let value = self.heap.project(&self.scratch[0], 0)?;
        if failed {
            self.scratch.push(value);
            self.raise_ordinary()?;
        } else {
            self.primitive_result(binding, value)?;
        }
        Ok(())
    }
}
