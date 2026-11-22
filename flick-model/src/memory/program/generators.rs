// Strict and lazy generator / sequence iteration stepping.
impl Machine<'_, '_> {
    fn yield_element(&mut self, binding: Local, yielded: Value) -> Result<(), Error> {
        let Some(at) = self.frames.iter().rposition(|frame| {
            matches!(
                frame.destination,
                Destination::GenerateStrict { .. } | Destination::GeneratorStep { .. }
            )
        }) else {
            self.scratch.push(yielded);
            return Err(Error::MissingGeneratorBoundary);
        };
        match &mut self.frames[at].destination {
            Destination::GenerateStrict {
                items,
                remaining,
                binding: strict_binding,
            } => {
                if let Some(rem) = remaining {
                    if *rem == 0 {
                        let strict_binding = *strict_binding;
                        let collected = mem::take(items);
                        self.scratch.push(yielded);
                        self.release_operands()?;
                        let frames = self.frames.split_off(at);
                        self.discard = Some(Discard {
                            frames,
                            owner: None,
                            next: TransferNext::FinishGenerateStrict {
                                binding: strict_binding,
                                items: collected,
                            },
                            aborted: false,
                        });
                        return Ok(());
                    }
                    *rem -= 1;
                }
                items.push(yielded);
                self.store(binding, Value::Unit)?;
                Ok(())
            }
            Destination::GeneratorStep {
                gen_id,
                binding: caller_binding,
            } => {
                let gen_id = *gen_id;
                let caller_binding = *caller_binding;
                let old_owner = self.current_owner;
                let frames = self.frames.split_off(at);
                self.restore_evidence();
                let exhausted = if let Some(slot) = self.generators.get_mut(&gen_id) {
                    if let Some(rem) = &mut slot.remaining {
                        if *rem == 0 {
                            slot.run = GeneratorRun::Done;
                            true
                        } else {
                            *rem -= 1;
                            slot.run = GeneratorRun::Suspended {
                                frames: Vec::new(),
                                owner: old_owner,
                                yield_binding: binding,
                            };
                            false
                        }
                    } else {
                        slot.run = GeneratorRun::Suspended {
                            frames: Vec::new(),
                            owner: old_owner,
                            yield_binding: binding,
                        };
                        false
                    }
                } else {
                    false
                };
                if exhausted {
                    self.scratch.push(yielded);
                    self.release_operands()?;
                    self.discard = Some(Discard {
                        frames,
                        owner: Some(self.current_owner),
                        next: TransferNext::DeliverNone {
                            binding: caller_binding,
                        },
                        aborted: false,
                    });
                    return Ok(());
                }
                if let Some(slot) = self.generators.get_mut(&gen_id)
                    && let GeneratorRun::Suspended {
                        frames: saved_frames,
                        ..
                    } = &mut slot.run
                {
                    *saved_frames = frames;
                }
                let some = self.heap.construct("Prelude.Option.Some", vec![yielded])?;
                self.store(caller_binding, some)?;
                Ok(())
            }
            _ => unreachable!("matched generator boundary"),
        }
    }

    fn seq_open(&mut self, binding: Local) -> Result<(), Error> {
        let source = self.scratch.pop().expect("seq source");
        let state = if let Ok((backing_id, offset, len)) = self.heap.list_view(&source) {
            let backing = self.heap.dup(&Value::Object(ObjectToken { id: backing_id }))?;
            self.heap.drop_value(source)?;
            IteratorState::List {
                backing,
                next: offset,
                end: offset + len,
            }
        } else {
            let Value::Object(token) = &source else {
                self.scratch.push(source);
                return Err(Error::ExpectedSeq);
            };
            let seq_kind = match &self.heap.object(token.id)?.data {
                Data::Seq(SeqData::FromList(list)) => Some((true, list.representation(), 0, None)),
                Data::Seq(SeqData::Generator {
                    execution,
                    code,
                    limit,
                    ..
                }) => {
                    if *execution != self.execution {
                        self.scratch.push(source);
                        return Err(Error::ForeignSeq);
                    }
                    Some((false, Value::Unit, *code, *limit))
                }
                _ => None,
            };
            let Some((from_list, inner_list, code, limit)) = seq_kind else {
                self.scratch.push(source);
                return Err(Error::ExpectedSeq);
            };
            if from_list {
                let (backing_id, offset, len) = self.heap.list_view(&inner_list)?;
                let backing = self.heap.dup(&Value::Object(ObjectToken { id: backing_id }))?;
                self.heap.drop_value(source)?;
                IteratorState::List {
                    backing,
                    next: offset,
                    end: offset + len,
                }
            } else {
                let id = self.handler_identity()?;
                self.generators.insert(
                    id,
                    GeneratorSlot {
                        run: GeneratorRun::Fresh { seq: source, code },
                        remaining: limit,
                    },
                );
                IteratorState::Generator { id }
            }
        };
        let root = self.heap.allocate(Data::Iterator {
            execution: self.execution,
            state,
        });
        let id = root.object_id().expect("iterator allocation");
        self.frames
            .last_mut()
            .expect("iterator frame")
            .iterators
            .push(root);
        self.store(
            binding,
            Value::Iterator(IteratorHandle {
                id,
                execution: self.execution,
            }),
        )?;
        Ok(())
    }

    fn seq_next(&mut self, binding: Local) -> Result<(), Error> {
        enum StepAction {
            Element(Value),
            Exhausted,
            DriveGenerator(usize),
        }
        let Value::Iterator(handle) = &self.scratch[0] else {
            return Err(Error::ExpectedIterator);
        };
        let handle = IteratorHandle {
            id: handle.id,
            execution: handle.execution,
        };
        self.release_operands()?;
        let action = match self.heap.iterator_state_mut(self.execution, &handle)? {
            IteratorState::List { backing, next, end } => {
                if *next < *end {
                    let index = *next;
                    *next += 1;
                    let Value::Object(backing_tok) = backing else {
                        return Err(Error::ExpectedObject);
                    };
                    let backing_id = backing_tok.id;
                    let elem = {
                        let Data::Tagged { fields, .. } = &self.heap.object(backing_id)?.data
                        else {
                            return Err(Error::ExpectedObject);
                        };
                        fields[index].representation()
                    };
                    let owned = self.heap.dup(&elem)?;
                    StepAction::Element(owned)
                } else {
                    StepAction::Exhausted
                }
            }
            IteratorState::Generator { id } => StepAction::DriveGenerator(*id),
            IteratorState::Exhausted => StepAction::Exhausted,
        };
        match action {
            StepAction::Element(elem) => {
                let some = self.heap.construct("Prelude.Option.Some", vec![elem])?;
                self.store(binding, some)
            }
            StepAction::Exhausted => {
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store(binding, none)
            }
            StepAction::DriveGenerator(gen_id) => self.drive_generator(gen_id, binding),
        }
    }

    fn drive_generator(&mut self, gen_id: usize, binding: Local) -> Result<(), Error> {
        let Some(slot) = self.generators.get_mut(&gen_id) else {
            let none = self.heap.construct("Prelude.Option.None", vec![])?;
            return self.store(binding, none);
        };
        if slot.remaining == Some(0) {
            let prev = mem::replace(&mut slot.run, GeneratorRun::Done);
            match prev {
                GeneratorRun::Fresh { seq, .. } => {
                    self.heap.drop_value(seq)?;
                    let none = self.heap.construct("Prelude.Option.None", vec![])?;
                    self.store(binding, none)?;
                }
                GeneratorRun::Suspended { frames, .. } => {
                    self.discard = Some(Discard {
                        frames,
                        owner: Some(self.current_owner),
                        next: TransferNext::DeliverNone { binding },
                        aborted: false,
                    });
                }
                GeneratorRun::Running | GeneratorRun::Done => {
                    let none = self.heap.construct("Prelude.Option.None", vec![])?;
                    self.store(binding, none)?;
                }
            }
            return Ok(());
        }
        let suspend_len = match &slot.run {
            GeneratorRun::Suspended { frames, .. } => Some(frames.len()),
            _ => None,
        };
        if let Some(len) = suspend_len {
            self.transfer_fuel(len)?;
        }
        let Some(slot) = self.generators.get_mut(&gen_id) else {
            unreachable!("checked generator slot");
        };
        let run = mem::replace(&mut slot.run, GeneratorRun::Running);
        match run {
            GeneratorRun::Fresh { seq, code } => {
                self.scratch.push(seq);
                self.scratch.push(Value::Unit);
                self.call(code, Destination::GeneratorStep { gen_id, binding })
            }
            GeneratorRun::Suspended {
                mut frames,
                owner,
                yield_binding,
            } => {
                if let Some(bottom) = frames.first_mut() {
                    bottom.destination = Destination::GeneratorStep { gen_id, binding };
                }
                self.current_owner = owner;
                self.frames.extend(frames);
                self.restore_evidence();
                self.store(yield_binding, Value::Unit)
            }
            GeneratorRun::Running | GeneratorRun::Done => {
                slot.run = GeneratorRun::Done;
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store(binding, none)
            }
        }
    }

    fn seq_to_list(&mut self, binding: Local) -> Result<(), Error> {
        let source = self.scratch.pop().expect("seq for Seq.to_list");
        if self.heap.list_view(&source).is_ok() {
            return self.store(binding, source);
        }
        let Value::Object(token) = &source else {
            self.scratch.push(source);
            return Err(Error::ExpectedSeq);
        };
        let info = match &self.heap.object(token.id)?.data {
            Data::Seq(SeqData::FromList(list)) => Some((true, list.representation(), 0, None)),
            Data::Seq(SeqData::Generator {
                execution,
                code,
                limit,
                ..
            }) => {
                if *execution != self.execution {
                    self.scratch.push(source);
                    return Err(Error::ForeignSeq);
                }
                Some((false, Value::Unit, *code, *limit))
            }
            _ => None,
        };
        let Some((from_list, inner_list, code, limit)) = info else {
            self.scratch.push(source);
            return Err(Error::ExpectedSeq);
        };
        if from_list {
            let owned = self.heap.dup(&inner_list)?;
            self.heap.drop_value(source)?;
            self.store(binding, owned)
        } else {
            self.scratch.push(source);
            self.scratch.push(Value::Unit);
            self.call(
                code,
                Destination::GenerateStrict {
                    items: Vec::new(),
                    remaining: limit,
                    binding,
                },
            )
        }
    }
}
