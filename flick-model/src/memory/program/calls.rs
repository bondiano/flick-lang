enum AwaitTaskKind {
    Code(CodeId),
    Masked { target_mask: usize, task_rep: Value },
    CachedRun(usize),
    CachedInvalidate(usize),
    ExitOk(Value),
    ExitErr(Value),
}

impl<'p> Machine<'p, '_> {
    fn deliver(&mut self, destination: &Destination<'p>, value: Value) -> Result<(), Error> {
        self.scratch.push(value);
        self.heap
            .admit_ordinary(self.scratch.last().expect("delivery root"))?;
        let value = self.scratch.pop().expect("ordinary delivery");
        match destination {
            Destination::Top => self.top = Some(value),
            Destination::Bind(binding) => {
                self.frames
                    .last_mut()
                    .expect("suspended caller")
                    .locals
                    .insert_one(*binding, value);
            }
            _ => unreachable!("specialized boundaries complete before delivery"),
        }
        Ok(())
    }

    fn global(&mut self, name: &Name, binding: Option<Local>) -> Result<(), Error> {
        match self.globals.get(name) {
            Some(Some(value)) => {
                self.heap.admit_ordinary(value)?;
                let value = self.heap.dup(value)?;
                self.deliver(&binding.map_or(Destination::Top, Destination::Bind), value)?;
            }
            Some(None) => return Err(Error::GlobalCycle(name.clone())),
            None => {
                let code = self
                    .program
                    .globals
                    .iter()
                    .find(|global| &global.name == name)
                    .ok_or_else(|| Error::UnknownGlobal(name.clone()))?
                    .initializer;
                self.globals.insert(name.clone(), None);
                self.call(
                    code,
                    Destination::Initialize {
                        name: name.clone(),
                        binding,
                    },
                )?;
            }
        }
        Ok(())
    }

    fn stage(&mut self, operands: impl IntoIterator<Item = Local>) -> Result<(), Error> {
        let locals = &mut self.frames.last_mut().expect("operand caller").locals;
        for local in operands {
            self.scratch.push(take(locals, local)?);
        }
        Ok(())
    }

    fn call(&mut self, code: CodeId, destination: Destination<'p>) -> Result<(), Error> {
        self.call_inner(code, destination, true)
    }

    fn call_inner(
        &mut self,
        code: CodeId,
        destination: Destination<'p>,
        allow_tco: bool,
    ) -> Result<(), Error> {
        let function = self
            .program
            .functions
            .get(&code)
            .ok_or(Error::InvalidProgram(memory::Error::UnknownCode(code)))?;
        if function.params.len() != self.scratch.len() {
            return Err(Error::ArgumentCount {
                expected: function.params.len(),
                actual: self.scratch.len(),
            });
        }
        if allow_tco
            && let Destination::Bind(binding) = &destination
            && let Some(top) = self.frames.last_mut()
            && top.at == top.block.instructions.len()
            && matches!(top.block.exit, Exit::Return(ret) if ret == *binding)
            && top.locals.is_empty()
            && top.vars.is_empty()
            && top.iterators.is_empty()
            && top.recovery_id.is_none()
            && !matches!(
                top.destination,
                Destination::RecoverySelection { .. } | Destination::RecoveryHelper { .. }
            )
        {
            top.block = &function.body;
            top.at = 0;
            for (&param, value) in function.params.iter().zip(self.scratch.drain(..)) {
                top.locals.insert_one(param, value);
            }
            top.evidence = self.current_evidence;
            return Ok(());
        }
        let mut locals = Locals::new();
        for (&param, value) in function.params.iter().zip(self.scratch.drain(..)) {
            locals.insert_one(param, value);
        }
        let recovery_id = match &destination {
            Destination::RecoverySelection { id } | Destination::RecoveryHelper { id, .. } => {
                Some(*id)
            }
            _ => None,
        };
        self.frames.push(Frame {
            recovery_id,
            block: &function.body,
            at: 0,
            locals,
            vars: Vec::new(),
            iterators: Vec::new(),
            destination,
            evidence: self.current_evidence,
        });
        Ok(())
    }

    fn apply_fresh(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        self.apply_inner(destination, false)
    }

    fn apply(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        self.apply_inner(destination, true)
    }

    fn apply_inner(&mut self, destination: Destination<'p>, allow_tco: bool) -> Result<(), Error> {
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedClosure);
        };
        match &self.heap.object(token.id)?.data {
            Data::Closure {
                execution, code, ..
            } => {
                if *execution != self.execution {
                    return Err(Error::ForeignClosure);
                }
                let code = *code;
                self.call_inner(code, destination, allow_tco)
            }
            Data::MaskRestore {
                execution,
                outer_mask,
            } => {
                if *execution != self.execution {
                    return Err(Error::ForeignClosure);
                }
                let target_mask = *outer_mask;
                let task = self.scratch.pop().expect("restore task");
                self.release_operands()?;
                let masked = self.heap.allocate(Data::MaskedTask { target_mask, task });
                self.pending = Some(masked);
                self.returned_frame(destination)
            }
            Data::Continuation { execution, id } => {
                if *execution != self.execution {
                    return Err(Error::ForeignContinuation);
                }
                let id = *id;
                let Destination::Bind(binding) = destination else {
                    return Err(Error::UnsupportedOperation(
                        "non-call continuation application",
                    ));
                };
                let arg = self.scratch.pop().expect("resumed argument");
                self.pending = Some(arg);
                self.release_operands()?;
                let Some(mut captured) = self.continuations.get_mut(&id).and_then(Option::take)
                else {
                    if let Some(arg) = self.pending.take() {
                        self.heap.drop_value(arg)?;
                    }
                    return Err(Error::ResumeTwice);
                };
                if let Err(err) = self.transfer_fuel(captured.frames.len()) {
                    self.continuations.insert(id, Some(captured));
                    return Err(err);
                }
                let target_binding = if let Some(top) = self.frames.last()
                    && top.at == top.block.instructions.len()
                    && matches!(top.block.exit, Exit::Return(ret) if ret == binding)
                    && top.locals.is_empty()
                    && top.vars.is_empty()
                    && top.iterators.is_empty()
                    && top.recovery_id.is_none()
                    && (matches!(top.destination, Destination::Bind(_))
                        || matches!(top.destination, Destination::Discontinue { cont_id, .. } if cont_id == id))
                {
                    let popped = self.frames.pop().expect("tail-resume frame");
                    match popped.destination {
                        Destination::Bind(outer)
                        | Destination::Discontinue { binding: outer, .. } => outer,
                        _ => unreachable!("matched tail-resume destination"),
                    }
                } else {
                    binding
                };
                if let Some(bottom) = captured.frames.first_mut()
                    && let Destination::Handle {
                        binding: handle_binding,
                        ..
                    } = &mut bottom.destination
                {
                    *handle_binding = target_binding;
                }
                self.current_owner = captured.owner;
                self.frames.extend(captured.frames);
                self.restore_evidence();
                let arg = self.pending.take().expect("resumed argument");
                self.store(captured.binding, arg)?;
                Ok(())
            }
            _ => Err(Error::ExpectedClosure),
        }
    }

    fn await_task(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        self.await_task_inner(destination, true)
    }

    fn await_task_fresh(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        self.await_task_inner(destination, false)
    }

    fn classify_task(&self, obj_id: ObjectId) -> Result<AwaitTaskKind, Error> {
        match &self.heap.object(obj_id)?.data {
            Data::Task {
                execution, code, ..
            } => {
                if *execution != self.execution {
                    return Err(Error::ForeignTask);
                }
                Ok(AwaitTaskKind::Code(*code))
            }
            Data::MaskedTask { target_mask, task } => Ok(AwaitTaskKind::Masked {
                target_mask: *target_mask,
                task_rep: task.representation(),
            }),
            Data::CachedRunTask { execution, id } => {
                if *execution != self.execution {
                    return Err(Error::ForeignTask);
                }
                Ok(AwaitTaskKind::CachedRun(*id))
            }
            Data::CachedInvalidateTask { execution, id } => {
                if *execution != self.execution {
                    return Err(Error::ForeignTask);
                }
                Ok(AwaitTaskKind::CachedInvalidate(*id))
            }
            Data::ExitTask(ExitData::Success(v)) => Ok(AwaitTaskKind::ExitOk(v.representation())),
            Data::ExitTask(ExitData::Failure(c)) => Ok(AwaitTaskKind::ExitErr(c.representation())),
            _ => Err(Error::ExpectedTask),
        }
    }

    fn await_cached_run(
        &mut self,
        cell_id: usize,
        destination: Destination<'p>,
    ) -> Result<(), Error> {
        self.release_operands()?;
        let cached_rep = self.cached_cells.get(&cell_id).and_then(|cell| {
            if let CachedEntry::Ready { value, expires_ns } = &cell.entry
                && expires_ns.is_none_or(|exp| self.virtual_now_ns < exp)
            {
                Some(value.representation())
            } else {
                None
            }
        });
        if let Some(rep) = cached_rep {
            let val = self.heap.dup(&rep)?;
            self.pending = Some(val);
            return self.returned_frame(destination);
        }
        let task_rep = self.cached_cells[&cell_id].task.representation();
        let task = self.heap.dup(&task_rep)?;
        self.scratch.extend([task, Value::Unit]);
        self.await_task_fresh(Destination::CachedComplete {
            cell_id,
            next: Box::new(destination),
        })
    }

    fn await_task_inner(
        &mut self,
        destination: Destination<'p>,
        allow_tco: bool,
    ) -> Result<(), Error> {
        if self.check_active_interrupt()? {
            self.release_operands()?;
            return self.fail_destination(destination);
        }
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedTask);
        };
        match self.classify_task(token.id)? {
            AwaitTaskKind::Code(code) => self.call_inner(code, destination, allow_tco),
            AwaitTaskKind::Masked {
                target_mask,
                task_rep,
            } => {
                let inner_task = self.heap.dup(&task_rep)?;
                self.release_operands()?;
                let saved_mask = self.mask_depth;
                self.mask_depth = target_mask;
                self.scratch.extend([inner_task, Value::Unit]);
                self.await_task_fresh(Destination::RestoreMask {
                    saved_mask,
                    check_on_exit: true,
                    next: Box::new(destination),
                })
            }
            AwaitTaskKind::CachedRun(cell_id) => self.await_cached_run(cell_id, destination),
            AwaitTaskKind::CachedInvalidate(cell_id) => {
                self.release_operands()?;
                if let Some(cell) = self.cached_cells.get_mut(&cell_id) {
                    let old = mem::replace(&mut cell.entry, CachedEntry::Empty);
                    if let CachedEntry::Ready { value, .. } = old {
                        self.heap.drop_value(value)?;
                    }
                }
                self.pending = Some(Value::Unit);
                self.returned_frame(destination)
            }
            AwaitTaskKind::ExitOk(rep) => {
                let val = self.heap.dup(&rep)?;
                self.release_operands()?;
                self.pending = Some(val);
                self.returned_frame(destination)
            }
            AwaitTaskKind::ExitErr(rep) => {
                let cval = self.heap.dup(&rep)?;
                self.release_operands()?;
                let cause = CauseToken::from_cause_value(self.heap, cval)?;
                self.failure = Some(cause);
                self.failure_elected = true;
                self.fail_destination(destination)
            }
        }
    }

    fn advance_main(&mut self) -> Result<(), Error> {
        if let MainStage::Root(main) = &self.main_stage {
            let main = main.clone();
            self.install_root()?;
            self.main_stage = MainStage::Function;
            return self.global(&main, None);
        }
        let stage = self.main_stage.clone();
        self.main_stage = if stage == MainStage::Function {
            MainStage::Task
        } else {
            MainStage::Done
        };
        let callable = if let Some(Value::Object(token)) = &self.top {
            matches!(
                (&self.heap.object(token.id)?.data, &stage),
                (Data::Closure { .. }, MainStage::Function)
                    | (
                        Data::Task { .. }
                            | Data::MaskedTask { .. }
                            | Data::CachedRunTask { .. }
                            | Data::CachedInvalidateTask { .. }
                            | Data::ExitTask(_),
                        MainStage::Task,
                    )
            )
        } else {
            false
        };
        if callable {
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            self.scratch.push(self.top.take().expect("main callable"));
            self.scratch.push(Value::Unit);
            if stage == MainStage::Function {
                self.apply(Destination::Top)?;
            } else {
                self.await_task(Destination::Top)?;
            }
        }
        Ok(())
    }

    fn store(&mut self, binding: Local, value: Value) -> Result<(), Error> {
        self.scratch.push(value);
        self.heap
            .admit_ordinary(self.scratch.last().expect("ordinary result root"))?;
        let value = self.scratch.pop().expect("ordinary admitted result");
        self.store_raw(binding, value);
        Ok(())
    }

    fn store_raw(&mut self, binding: Local, value: Value) {
        self.frames
            .last_mut()
            .expect("result caller")
            .locals
            .insert_one(binding, value);
    }

    fn reportable_failure(&self) -> Result<bool, Error> {
        Ok(self
            .heap
            .ordinary_failure(self.failure.as_ref().expect("failure root"))?
            .is_some())
    }

    fn take_failure_payload(&mut self) -> Result<Value, Error> {
        charge(&mut self.fuel, 1)?;
        let value = self
            .heap
            .ordinary_failure(self.failure.as_ref().expect("failure root"))?
            .expect("singleton ordinary Fail")
            .representation();
        self.pending = Some(self.heap.dup(&value)?);
        self.heap
            .drop_cause(self.failure.take().expect("failure root"))?;
        Ok(self.pending.take().expect("retained typed payload"))
    }

    fn raise_ordinary(&mut self) -> Result<(), Error> {
        let payload = self.scratch.last().expect("staged typed payload");
        let cause = self.heap.cause(&[Reason::Fail(payload)], &mut self.fuel)?;
        self.failure = Some(cause);
        self.failure_elected = false;
        self.release_operands()
    }

    fn raise_defect(&mut self, spec: DefectSpec) -> Result<(), Error> {
        charge(&mut self.fuel, 1)?;
        self.scratch.push(self.heap.allocate(Data::Defect(spec)));
        let payload = self.scratch.last().expect("rooted fresh defect");
        let cause = self.heap.cause(&[Reason::Die(payload)], &mut self.fuel)?;
        self.failure = Some(cause);
        self.failure_elected = false;
        self.release_operands()
    }

    fn release_operands(&mut self) -> Result<(), Error> {
        self.heap.drop_values(mem::take(&mut self.scratch))
    }

    fn primitive_result(&mut self, binding: Local, result: Value) -> Result<(), Error> {
        self.pending = Some(result);
        self.release_operands()?;
        let value = self.pending.take().expect("computed result");
        self.store(binding, value)?;
        Ok(())
    }
}
