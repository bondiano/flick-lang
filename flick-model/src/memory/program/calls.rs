impl Machine<'_, '_> {
    fn deliver(&mut self, destination: &Destination, value: Value) -> Result<(), Error> {
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
                    .insert(*binding, vec![value]);
            }
            Destination::RecoverySelection { .. }
            | Destination::RecoveryHelper { .. }
            | Destination::RecoveryBody { .. } => {
                unreachable!("recovery result completes its boundary first")
            }
            Destination::Catch { .. } => unreachable!("catch result releases handler first"),
            Destination::LayerRun { .. }
            | Destination::Handle { .. }
            | Destination::Invocation(_) => {
                unreachable!("handler result completes its boundary first")
            }
            Destination::Initialize { .. } => {
                unreachable!("initializer results are cached before delivery")
            }
            Destination::Modify { .. } => unreachable!("modify results commit before delivery"),
            Destination::CloseScope { .. } | Destination::RestoreOwner { .. } => {
                unreachable!("owner boundaries restore before delivery")
            }
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

    fn call(&mut self, code: CodeId, destination: Destination) -> Result<(), Error> {
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
        let locals = function
            .params
            .iter()
            .copied()
            .zip(
                mem::take(&mut self.scratch)
                    .into_iter()
                    .map(|value| vec![value]),
            )
            .collect();
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
            destination,
            evidence: self.current_evidence,
        });
        Ok(())
    }

    fn apply(&mut self, destination: Destination) -> Result<(), Error> {
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedClosure);
        };
        let Data::Closure {
            execution, code, ..
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedClosure);
        };
        if *execution != self.execution {
            return Err(Error::ForeignClosure);
        }
        self.call(*code, destination)
    }

    fn await_task(&mut self, destination: Destination) -> Result<(), Error> {
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedTask);
        };
        let Data::Task {
            execution, code, ..
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedTask);
        };
        if *execution != self.execution {
            return Err(Error::ForeignTask);
        }
        self.call(*code, destination)
    }

    fn advance_main(&mut self) -> Result<(), Error> {
        let stage = self.main_stage;
        self.main_stage = if stage == MainStage::Function {
            MainStage::Task
        } else {
            MainStage::Done
        };
        let callable = if let Some(Value::Object(token)) = &self.top {
            matches!(
                (&self.heap.object(token.id)?.data, stage),
                (Data::Closure { .. }, MainStage::Function) | (Data::Task { .. }, MainStage::Task)
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
            .insert(binding, vec![value]);
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
