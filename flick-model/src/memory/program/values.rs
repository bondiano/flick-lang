impl Machine<'_, '_> {
    fn operation(&mut self, binding: Local, operation: &Operation) -> Result<(), Error> {
        self.admit_operands(operation)?;
        if self.dictionary_operation(binding, operation)?
            || self.evidence_operation(binding, operation)?
            || self.handler_operation(binding, operation)?
            || self.control_operation(binding, operation)?
        {
            return Ok(());
        }
        match operation {
            Operation::RecoveryResult { selected } => self.recovery_result(binding, *selected)?,
            Operation::Handle { .. }
            | Operation::Perform { .. }
            | Operation::ServiceTag(_)
            | Operation::VarMake(_)
            | Operation::VarGet(_)
            | Operation::VarSet { .. }
            | Operation::Loop { .. }
            | Operation::LoopExit { .. }
            | Operation::FunctionBoundary { .. }
            | Operation::UnwrapOption(_)
            | Operation::GenerateStrict { .. }
            | Operation::GenerateLazy { .. }
            | Operation::Yield(_)
            | Operation::SeqOpen(_)
            | Operation::SeqNext(_)
            | Operation::Label(_)
            | Operation::Dictionary { .. }
            | Operation::Method { .. }
            | Operation::Structure { .. }
            | Operation::ReadOwnerEvidence
            | Operation::WithOwnerEvidence { .. } => {
                unreachable!("specialized operations dispatched above")
            }
            Operation::Global(name) => self.global(name, Some(binding))?,
            Operation::Call { code, args } => self.direct_call(binding, *code, args)?,
            Operation::Within { owner, code, args } => {
                self.within(binding, *owner, *code, args)?;
            }
            Operation::Apply { function, argument } => {
                self.stage([*function, *argument])?;
                self.apply(Destination::Bind(binding))?;
            }
            Operation::Await(task) => {
                self.stage([*task])?;
                self.scratch.push(Value::Unit);
                self.await_task(Destination::Bind(binding))?;
            }
            Operation::Project { object, field } => self.project_value(binding, *object, *field)?,
            Operation::Match { object, pattern } => {
                self.match_value(binding, *object, pattern)?;
            }
            Operation::Update { object, fields } => {
                self.stage(iter::once(*object).chain(fields.iter().map(|(_, local)| *local)))?;
                let value = patterns::update(self.heap, &mut self.scratch, fields, &mut self.fuel)?;
                self.primitive_result(binding, value)?;
            }
            Operation::Field { object, field } => {
                self.field_operation(binding, *object, field)?;
            }
            Operation::ForeignCall { spec, args } => {
                self.foreign_call(binding, spec, args)?;
            }
            Operation::Literal(literal) => {
                let value = self.heap.literal(literal)?;
                self.store(binding, value)?;
            }
            Operation::Closure { code, captures } | Operation::Task { code, captures } => {
                self.capture(
                    binding,
                    *code,
                    captures,
                    matches!(operation, Operation::Task { .. }),
                )?;
            }
            Operation::Record { fields } => self.record(binding, fields)?,
            Operation::Construct { tag, fields } => self.construct(binding, tag, fields, &[])?,
            Operation::ConstructWithDicts { tag, fields, dicts } => {
                self.construct(binding, tag, fields, dicts)?;
            }
            Operation::DropReuse(object) => self.drop_reuse(binding, *object)?,
            Operation::Reuse {
                object,
                tag,
                fields,
            } => self.reuse(binding, *object, tag, fields, &[])?,
            Operation::ReuseWithDicts {
                object,
                tag,
                fields,
                dicts,
            } => self.reuse(binding, *object, tag, fields, dicts)?,
            Operation::ReuseRecord { object, fields } => {
                self.reuse_record(binding, *object, fields)?;
            }
            Operation::Binary { .. }
            | Operation::Not(_)
            | Operation::Neg { .. }
            | Operation::Intrinsic { .. } => self.primitive_operation(binding, operation)?,
        }
        Ok(())
    }

    fn field_operation(&mut self, binding: Local, object: Local, field: &str) -> Result<(), Error> {
        let receiver = peek(&self.frames.last().expect("caller").locals, object)?;
        if let Value::Object(tok) = receiver
            && let Data::ForeignHandle { execution, host_id } = self.heap.object(tok.id)?.data
        {
            return self.foreign_get_field(binding, execution, host_id, field);
        }
        let value = self.heap.project_named(receiver, field)?;
        self.store(binding, value)
    }

    fn primitive_operation(&mut self, binding: Local, operation: &Operation) -> Result<(), Error> {
        match operation {
            Operation::Binary { op, left, right } => {
                self.stage([*left, *right])?;
                let result = if self.foreign.bridge_opened
                    && matches!(
                        op,
                        BinOp::Eq | BinOp::NotEq | BinOp::Same | BinOp::NotSame
                    )
                    && let Some(eq) = self.try_foreign_handles_equal()?
                {
                    Value::Bool(eq == matches!(op, BinOp::Eq | BinOp::Same))
                } else {
                    primitives::binary_metered(self.heap, *op, &self.scratch, &mut self.fuel)?
                };
                self.primitive_result(binding, result)?;
            }
            Operation::Not(local) => {
                self.stage([*local])?;
                let result = primitives::not(&self.scratch[0])?;
                self.primitive_result(binding, result)?;
            }
            Operation::Neg { num, value } => {
                self.stage([*value])?;
                let result = primitives::neg(self.heap, *num, &self.scratch[0])?;
                self.primitive_result(binding, result)?;
            }
            Operation::Intrinsic { kind, args } => {
                self.stage(args.iter().copied())?;
                self.intrinsic(*kind, binding)?;
            }
            _ => unreachable!("non-primitive operation"),
        }
        Ok(())
    }

    fn admit_operands(&self, operation: &Operation) -> Result<(), Error> {
        let locals = &self.frames.last().expect("operand caller").locals;
        match operation {
            Operation::Reuse { object, fields, .. } => {
                self.heap.admit_reuse_candidate(peek(locals, *object)?)?;
                for local in fields {
                    self.heap.admit_ordinary(peek(locals, *local)?)?;
                }
            }
            Operation::ReuseWithDicts {
                object,
                fields,
                dicts,
                ..
            } => {
                self.heap.admit_reuse_candidate(peek(locals, *object)?)?;
                for local in fields.iter().chain(dicts) {
                    self.heap.admit_ordinary(peek(locals, *local)?)?;
                }
            }
            Operation::ReuseRecord { object, fields } => {
                self.heap.admit_reuse_candidate(peek(locals, *object)?)?;
                for (_, local) in fields {
                    self.heap.admit_ordinary(peek(locals, *local)?)?;
                }
            }
            _ => {
                for local in operation.consumed().into_iter().chain(operation.borrowed()) {
                    self.heap.admit_ordinary(peek(locals, local)?)?;
                }
            }
        }
        Ok(())
    }

    fn direct_call(&mut self, binding: Local, code: CodeId, args: &[Local]) -> Result<(), Error> {
        self.stage(args.iter().copied())?;
        let destination = self
            .frames
            .last()
            .and_then(|frame| frame.recovery_id)
            .map_or(Destination::Bind(binding), |id| {
                Destination::RecoveryHelper { id, binding }
            });
        self.call(code, destination)
    }

    fn project_value(&mut self, binding: Local, object: Local, field: usize) -> Result<(), Error> {
        let receiver = peek(
            &self.frames.last().expect("projection caller").locals,
            object,
        )?;
        let value = self.heap.project(receiver, field)?;
        self.store(binding, value)?;
        Ok(())
    }

    fn match_value(&mut self, binding: Local, object: Local, pattern: &Pat) -> Result<(), Error> {
        let receiver = peek(&self.frames.last().expect("pattern caller").locals, object)?;
        let value = patterns::execute(
            self.heap,
            receiver,
            pattern,
            &mut self.fuel,
            &mut self.scratch,
        )?;
        self.store(binding, value)?;
        Ok(())
    }

    fn construct(
        &mut self,
        binding: Local,
        tag: &str,
        fields: &[Local],
        dicts: &[Local],
    ) -> Result<(), Error> {
        self.stage(fields.iter().chain(dicts).copied())?;
        let mut fields_owned = mem::take(&mut self.scratch);
        let dicts_owned = fields_owned.split_off(fields.len());
        let value = self
            .heap
            .construct_with_dicts(tag, fields_owned, dicts_owned)?;
        self.store(binding, value)?;
        Ok(())
    }

    fn drop_reuse(&mut self, binding: Local, object: Local) -> Result<(), Error> {
        self.stage([object])?;
        let candidate = self.scratch.remove(0);
        let token = self.heap.drop_reuse(candidate)?;
        self.store_raw(binding, token);
        Ok(())
    }

    fn reuse(
        &mut self,
        binding: Local,
        object: Local,
        tag: &str,
        fields: &[Local],
        dicts: &[Local],
    ) -> Result<(), Error> {
        self.stage(iter::once(object).chain(fields.iter().chain(dicts).copied()))?;
        let candidate = self.scratch.remove(0);
        let mut fields_owned = mem::take(&mut self.scratch);
        let dicts_owned = fields_owned.split_off(fields.len());
        let value = self
            .heap
            .reuse_with_dicts(candidate, tag, fields_owned, dicts_owned)?;
        self.store(binding, value)?;
        Ok(())
    }

    fn reuse_record(
        &mut self,
        binding: Local,
        object: Local,
        fields: &[(String, Local)],
    ) -> Result<(), Error> {
        self.stage(iter::once(object).chain(fields.iter().map(|(_, local)| *local)))?;
        let candidate = self.scratch.remove(0);
        let fields_owned = fields
            .iter()
            .map(|(name, _)| name.clone())
            .zip(mem::take(&mut self.scratch))
            .collect();
        let value = self.heap.reuse_record(candidate, fields_owned)?;
        self.store(binding, value)?;
        Ok(())
    }

    fn record(&mut self, binding: Local, fields: &[(String, Local)]) -> Result<(), Error> {
        let mut labels = BTreeSet::new();
        for (name, _) in fields {
            if !labels.insert(name) {
                return Err(Error::DuplicateField(name.clone()));
            }
        }
        self.stage(fields.iter().map(|(_, local)| *local))?;
        let fields = fields
            .iter()
            .map(|(name, _)| name.clone())
            .zip(mem::take(&mut self.scratch))
            .collect();
        let value = self.heap.allocate(Data::Record(fields));
        self.store(binding, value)?;
        Ok(())
    }

    fn restore_evidence(&mut self) {
        self.current_evidence = self.frames.last().and_then(|frame| frame.evidence);
    }
}
