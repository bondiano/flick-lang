fn is_metered_primitive(kind: Intrinsic) -> bool {
    matches!(
        kind,
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
            | Intrinsic::ExitHasInterrupts
            | Intrinsic::Int(_)
            | Intrinsic::FixedWidth(_)
            | Intrinsic::Float(_)
            | Intrinsic::BigInt(_)
            | Intrinsic::String(_)
            | Intrinsic::List(_)
            | Intrinsic::Record(_)
            | Intrinsic::Debug(_)
            | Intrinsic::Bytes(_)
            | Intrinsic::Hash(_)
            | Intrinsic::Json(_)
            | Intrinsic::BigDecimal(_)
            | Intrinsic::RegExp(_)
            | Intrinsic::Duration(_)
            | Intrinsic::Outcome(_)
            | Intrinsic::Gen(_)
            | Intrinsic::Schema(_)
            | Intrinsic::JsonSchema(_)
            | Intrinsic::Path(_)
            | Intrinsic::DateTime(_)
            | Intrinsic::Cron(_)
            | Intrinsic::Graph(_)
            | Intrinsic::Crypto(_)
            | Intrinsic::Config(_)
            | Intrinsic::Arbitrary(_)
    )
}

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
        if is_metered_primitive(kind) {
            return self.metered_primitive(kind, binding);
        }
        match kind {
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
            Intrinsic::Async(op) => self.async_intrinsic(op, binding)?,
            Intrinsic::Layer(op) => self.layer_std_intrinsic(op, binding)?,
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
            Intrinsic::EffectExit
            | Intrinsic::EffectDie
            | Intrinsic::FailureRethrow
            | Intrinsic::EffectCatchDefect => self.outcome_intrinsic(kind, binding)?,
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
            Intrinsic::PrintLine | Intrinsic::Show => {
                let value =
                    primitives::intrinsic(self.heap, kind, &self.scratch[0], &mut self.stdout)?;
                self.primitive_result(binding, value)?;
            }
            Intrinsic::Host(primitive) => {
                let value = self.host_primitive(primitive)?;
                self.primitive_result(binding, value)?;
            }
            _ => unreachable!("control and metered intrinsics dispatched above"),
        }
        Ok(())
    }

    fn outcome_intrinsic(&mut self, kind: Intrinsic, binding: Local) -> Result<(), Error> {
        match kind {
            Intrinsic::EffectExit => {
                self.scratch.push(Value::Unit);
                self.await_task_fresh(Destination::CaptureExit { binding })?;
            }
            Intrinsic::EffectDie => {
                let msg = self.scratch.pop().expect("die message");
                let cause = self.heap.cause(&[super::Reason::Die(&msg)], &mut self.fuel);
                self.heap.drop_value(msg)?;
                self.failure = Some(cause?);
                self.failure_elected = false;
            }
            Intrinsic::FailureRethrow => {
                let cause_val = self.scratch.pop().expect("rethrow cause");
                let token = CauseToken::from_cause_value(self.heap, cause_val)?;
                self.failure = Some(token);
                self.failure_elected = true;
            }
            Intrinsic::EffectCatchDefect => {
                let task = self.scratch.pop().expect("catch_defect task");
                let handler = self.scratch.pop().expect("catch_defect handler");
                self.pending_handler = Some(handler);
                self.scratch.extend([task, Value::Unit]);
                self.await_task_fresh(Destination::Bind(binding))?;
                self.frames
                    .last_mut()
                    .expect("catch_defect body frame")
                    .destination = Destination::CatchDefect {
                    handler: self.pending_handler.take().expect("staged handler"),
                    binding,
                };
            }
            _ => unreachable!("non-outcome intrinsic"),
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

    fn host_primitive(&mut self, primitive: HostPrimitive) -> Result<Value, Error> {
        match primitive {
            HostPrimitive::CurrentTimeMillis => {
                #[expect(clippy::cast_precision_loss, reason = "virtual ms fits f64")]
                let ms = (self.virtual_now_ns / 1_000_000) as f64;
                return Ok(Value::Float(ms));
            }
            HostPrimitive::CurrentTimeNanos | HostPrimitive::MonotonicTimeNanos => {
                return self
                    .heap
                    .literal(&Lit::BigInt(self.virtual_now_ns.to_string()));
            }
            HostPrimitive::RandomNext => {
                if let Some(rng_id) = self.seeded_random
                    && let Some(rng) = self.isaac_rngs.get_mut(&rng_id)
                {
                    return Ok(Value::Float(rng.next_f64()));
                }
            }
            _ => {}
        }
        primitives::intrinsic_with_host(
            self.heap,
            self.host_mode,
            Intrinsic::Host(primitive),
            &self.scratch[0],
            &mut self.stdout,
        )
    }

    fn unwrap_result(&mut self, binding: Local) -> Result<(), Error> {
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedResult);
        };
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
            ..
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
