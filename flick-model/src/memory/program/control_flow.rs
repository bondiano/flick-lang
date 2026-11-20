// Control flow, frame-owned vars, loops, and Option? unwinding.
impl Machine<'_, '_> {
    fn control_operation(&mut self, binding: Local, operation: &Operation) -> Result<bool, Error> {
        match operation {
            Operation::Label(name) => {
                let value = self.heap.allocate(Data::Label(name.clone()));
                self.store(binding, value)?;
            }
            Operation::VarMake(init) => self.var_make(binding, *init)?,
            Operation::VarGet(var) => self.var_get(binding, *var)?,
            Operation::VarSet { var, value } => self.var_set(binding, *var, *value)?,
            Operation::Loop { id, code, args } => self.loop_enter(binding, id, *code, args)?,
            Operation::LoopExit { id, value } => self.loop_exit_op(id, *value)?,
            Operation::FunctionBoundary { code, args } => {
                self.stage(args.iter().copied())?;
                self.call(*code, Destination::FunctionBoundary { binding })?;
            }
            Operation::UnwrapOption(option) => self.unwrap_option_op(binding, *option)?,
            Operation::GenerateStrict { code, args } => {
                self.stage(args.iter().copied())?;
                self.call(
                    *code,
                    Destination::GenerateStrict {
                        items: Vec::new(),
                        remaining: None,
                        binding,
                    },
                )?;
            }
            Operation::GenerateLazy {
                code,
                captures,
                limit,
                ..
            } => self.generate_lazy_op(binding, *code, captures, *limit)?,
            Operation::Yield(value) => {
                self.stage([*value])?;
                let yielded = self.scratch.pop().expect("yielded element");
                self.yield_element(binding, yielded)?;
            }
            Operation::SeqOpen(source) => {
                self.stage([*source])?;
                self.seq_open(binding)?;
            }
            Operation::SeqNext(iterator) => {
                self.stage([*iterator])?;
                self.seq_next(binding)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn var_make(&mut self, binding: Local, init: Local) -> Result<(), Error> {
        self.stage([init])?;
        let contents = self.scratch.pop().expect("var initial contents");
        let root = self.heap.allocate(Data::Var {
            execution: self.execution,
            contents: Some(contents),
        });
        let id = root.object_id().expect("var allocation");
        self.frames.last_mut().expect("var frame").vars.push(root);
        self.store(
            binding,
            Value::Var(VarHandle {
                id,
                execution: self.execution,
            }),
        )
    }

    fn var_get(&mut self, binding: Local, var: Local) -> Result<(), Error> {
        self.stage([var])?;
        let Value::Var(handle) = &self.scratch[0] else {
            return Err(Error::ExpectedVar);
        };
        let current = self
            .heap
            .var_contents(self.execution, handle)?
            .representation();
        let value = self.heap.dup(&current)?;
        self.primitive_result(binding, value)
    }

    fn var_set(&mut self, binding: Local, var: Local, value: Local) -> Result<(), Error> {
        self.stage([var, value])?;
        let Value::Var(handle) = &self.scratch[0] else {
            return Err(Error::ExpectedVar);
        };
        let handle = VarHandle {
            id: handle.id,
            execution: handle.execution,
        };
        let contents = self.heap.var_contents_mut(self.execution, &handle)?;
        let replacement = self.scratch.pop().expect("var replacement");
        let old = mem::replace(contents, replacement);
        self.scratch.push(old);
        self.primitive_result(binding, Value::Unit)
    }

    fn loop_enter(
        &mut self,
        binding: Local,
        id: &str,
        code: CodeId,
        args: &[Local],
    ) -> Result<(), Error> {
        self.stage(args.iter().copied())?;
        let mut saved = Vec::with_capacity(self.scratch.len());
        for value in &self.scratch {
            match self.heap.dup(value) {
                Ok(dup) => saved.push(dup),
                Err(err) => {
                    self.heap.drop_values(saved)?;
                    return Err(err);
                }
            }
        }
        self.call(
            code,
            Destination::Loop {
                id: id.to_owned(),
                code,
                args: saved,
                binding,
            },
        )
    }

    fn loop_exit_op(&mut self, id: &str, value: Option<Local>) -> Result<(), Error> {
        if let Some(value) = value {
            self.stage([value])?;
        }
        let target = self
            .frames
            .iter()
            .rposition(
                |frame| matches!(&frame.destination, Destination::Loop { id: target, .. } if target == id),
            )
            .ok_or_else(|| Error::MissingLoopTarget(id.to_owned()))?;
        if value.is_some() {
            let break_value = self.scratch.pop().expect("break value");
            let frames = self.frames.split_off(target);
            let Destination::Loop { binding, .. } = &frames[0].destination else {
                unreachable!("validated loop target");
            };
            let binding = *binding;
            self.discard = Some(Discard {
                frames,
                owner: None,
                next: TransferNext::DeliverValue {
                    binding,
                    value: break_value,
                },
            });
        } else {
            let mut frames = self.frames.split_off(target);
            let Destination::Loop {
                id,
                code,
                args,
                binding,
            } = mem::replace(&mut frames[0].destination, Destination::DiscardFinalizer)
            else {
                unreachable!("validated loop target");
            };
            self.discard = Some(Discard {
                frames,
                owner: None,
                next: TransferNext::LoopContinue {
                    id,
                    code,
                    args,
                    binding,
                },
            });
        }
        Ok(())
    }

    fn unwrap_option_op(&mut self, binding: Local, option: Local) -> Result<(), Error> {
        self.stage([option])?;
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedOption);
        };
        let (tag, ordinary_fields, total_fields) = match &self.heap.object(token.id)?.data {
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
            } => (tag.as_str(), *ordinary_fields, fields.len()),
            _ => return Err(Error::ExpectedOption),
        };
        match (tag, ordinary_fields, total_fields) {
            ("Prelude.Option.Some", 1, 1) => {
                let value = self.heap.project(&self.scratch[0], 0)?;
                self.primitive_result(binding, value)
            }
            ("Prelude.Option.None", 0, 0) => {
                self.release_operands()?;
                let target = self
                    .frames
                    .iter()
                    .rposition(|frame| {
                        matches!(frame.destination, Destination::FunctionBoundary { .. })
                    })
                    .ok_or(Error::MissingFunctionBoundary)?;
                let frames = self.frames.split_off(target);
                let Destination::FunctionBoundary { binding } = frames[0].destination else {
                    unreachable!("validated function boundary");
                };
                self.discard = Some(Discard {
                    frames,
                    owner: None,
                    next: TransferNext::ReturnNone { binding },
                });
                Ok(())
            }
            _ => Err(Error::ExpectedOption),
        }
    }

    fn generate_lazy_op(
        &mut self,
        binding: Local,
        code: CodeId,
        captures: &[Local],
        limit: Option<Local>,
    ) -> Result<(), Error> {
        self.stage(captures.iter().copied().chain(limit))?;
        let parsed_limit = if limit.is_some() {
            let lim_val = self.scratch.pop().expect("generator limit");
            let Value::Int(n) = lim_val else {
                self.scratch.push(lim_val);
                return Err(Error::ExpectedInt);
            };
            Some(usize::try_from(n.max(0)).unwrap_or(0))
        } else {
            None
        };
        let captures = mem::take(&mut self.scratch);
        let value = self.heap.allocate(Data::Seq(SeqData::Generator {
            execution: self.execution,
            code,
            captures,
            limit: parsed_limit,
        }));
        self.store(binding, value)
    }
}
