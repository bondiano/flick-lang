//! Strict and lazy sequences, generators, and iterator cursors for tier 1.

use std::mem;

use flick_core::ir::memory;

use crate::tier0::{Data, Error, IteratorHandle, IteratorState, ObjectToken, SeqData, Value};

use super::machine::{End, Machine, RegVal};
use super::machine_unwind::{CapturedFrames, Discard, TransferNext};

pub(super) enum GeneratorRun {
    Fresh {
        seq: Value,
        code: usize,
    },
    Suspended {
        frames: CapturedFrames,
        owner: usize,
        yield_dst: usize,
    },
    Running,
    Done,
}

pub(super) struct GeneratorSlot {
    pub(super) run: GeneratorRun,
    pub(super) remaining: Option<usize>,
}

impl Machine<'_, '_> {
    pub(super) fn exec_generate_strict(
        &mut self,
        dst: usize,
        code: usize,
        args: &[usize],
    ) -> Result<(), Error> {
        let mut vals = Vec::with_capacity(args.len());
        for &slot in args {
            vals.push(self.take_reg(slot)?);
        }
        self.push_call(
            code,
            vals,
            End::GenerateStrict {
                items: Vec::new(),
                remaining: None,
                dst,
            },
        )
    }

    pub(super) fn exec_generate_lazy(
        &mut self,
        dst: usize,
        code: usize,
        captures: &[usize],
        limit: Option<usize>,
    ) -> Result<(), Error> {
        self.stage_slots(captures)?;
        if let Some(lim_slot) = limit {
            self.stage_slots(&[lim_slot])?;
        }
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
        let captures_owned = mem::take(&mut self.scratch);
        let value = self.heap.allocate(Data::Seq(SeqData::Generator {
            execution: self.execution,
            code: self.image.ir_codes[code],
            captures: captures_owned,
            limit: parsed_limit,
        }));
        self.store_reg(dst, RegVal::Value(value))
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn exec_yield(&mut self, dst: usize, value_slot: usize) -> Result<(), Error> {
        self.stage_slots(&[value_slot])?;
        let yielded = self.scratch.pop().expect("yielded element");
        let Some(at) = self.frames.iter().rposition(|frame| {
            matches!(
                frame.end,
                End::GenerateStrict { .. } | End::GeneratorStep { .. }
            )
        }) else {
            self.scratch.push(yielded);
            return Err(Error::MissingGeneratorBoundary);
        };
        match &mut self.frames[at].end {
            End::GenerateStrict {
                items,
                remaining,
                dst: strict_dst,
            } => {
                if let Some(rem) = remaining {
                    if *rem == 0 {
                        let strict_dst = *strict_dst;
                        let collected = mem::take(items);
                        self.scratch.push(yielded);
                        self.release_operands()?;
                        let frames = self.split_frames(at);
                        self.bump_handler_epoch();
                        self.restore_evidence();
                        self.discard = Some(Discard {
                            frames,
                            owner: None,
                            next: TransferNext::FinishGenerateStrict {
                                dst: strict_dst,
                                items: collected,
                            },
                            aborted: false,
                        });
                        return Ok(());
                    }
                    *rem -= 1;
                }
                items.push(yielded);
                self.store_reg(dst, RegVal::Value(Value::Unit))
            }
            End::GeneratorStep {
                gen_id,
                dst: caller_dst,
            } => {
                let gen_id = *gen_id;
                let caller_dst = *caller_dst;
                let old_owner = self.current_owner;
                let captured = self.split_frames(at);
                self.bump_handler_epoch();
                self.restore_evidence();
                let exhausted = if let Some(slot) = self.generators.get_mut(&gen_id) {
                    if let Some(rem) = &mut slot.remaining {
                        if *rem == 0 {
                            slot.run = GeneratorRun::Done;
                            true
                        } else {
                            *rem -= 1;
                            slot.run = GeneratorRun::Suspended {
                                frames: CapturedFrames::default(),
                                owner: old_owner,
                                yield_dst: dst,
                            };
                            false
                        }
                    } else {
                        slot.run = GeneratorRun::Suspended {
                            frames: CapturedFrames::default(),
                            owner: old_owner,
                            yield_dst: dst,
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
                        frames: captured,
                        owner: Some(self.current_owner),
                        next: TransferNext::DeliverNone { dst: caller_dst },
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
                    *saved_frames = captured;
                }
                let some = self.heap.construct("Prelude.Option.Some", vec![yielded])?;
                self.store_reg(caller_dst, RegVal::Value(some))
            }
            _ => unreachable!("matched generator boundary"),
        }
    }

    pub(super) fn exec_seq_open(&mut self, dst: usize, source_slot: usize) -> Result<(), Error> {
        self.stage_slots(&[source_slot])?;
        let source = self.scratch.pop().expect("seq source");
        let state = if let Ok((backing_id, offset, len)) = self.heap.list_view(&source) {
            let backing = self
                .heap
                .dup(&Value::Object(ObjectToken { id: backing_id }))?;
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
                    let code_idx = *self
                        .image
                        .code_indices
                        .get(code)
                        .ok_or(Error::InvalidProgram(memory::Error::UnknownCode(*code)))?;
                    Some((false, Value::Unit, code_idx, *limit))
                }
                _ => None,
            };
            let Some((from_list, inner_list, code, limit)) = seq_kind else {
                self.scratch.push(source);
                return Err(Error::ExpectedSeq);
            };
            if from_list {
                let (backing_id, offset, len) = self.heap.list_view(&inner_list)?;
                let backing = self
                    .heap
                    .dup(&Value::Object(ObjectToken { id: backing_id }))?;
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
        self.frame_mut().iterators.push(root);
        self.store_reg(
            dst,
            RegVal::Value(Value::Iterator(IteratorHandle {
                id,
                execution: self.execution,
            })),
        )
    }

    pub(super) fn exec_seq_next(&mut self, dst: usize, iter_slot: usize) -> Result<(), Error> {
        enum StepAction {
            Element(Value),
            Exhausted,
            DriveGenerator(usize),
        }
        self.stage_slots(&[iter_slot])?;
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
                self.store_reg(dst, RegVal::Value(some))
            }
            StepAction::Exhausted => {
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store_reg(dst, RegVal::Value(none))
            }
            StepAction::DriveGenerator(gen_id) => self.drive_generator(gen_id, dst),
        }
    }

    fn drive_generator(&mut self, gen_id: usize, dst: usize) -> Result<(), Error> {
        let Some(slot) = self.generators.get_mut(&gen_id) else {
            let none = self.heap.construct("Prelude.Option.None", vec![])?;
            return self.store_reg(dst, RegVal::Value(none));
        };
        if slot.remaining == Some(0) {
            let prev = mem::replace(&mut slot.run, GeneratorRun::Done);
            match prev {
                GeneratorRun::Fresh { seq, .. } => {
                    self.heap.drop_value(seq)?;
                    let none = self.heap.construct("Prelude.Option.None", vec![])?;
                    self.store_reg(dst, RegVal::Value(none))?;
                }
                GeneratorRun::Suspended { frames, .. } => {
                    self.discard = Some(Discard {
                        frames,
                        owner: Some(self.current_owner),
                        next: TransferNext::DeliverNone { dst },
                        aborted: false,
                    });
                }
                GeneratorRun::Running | GeneratorRun::Done => {
                    let none = self.heap.construct("Prelude.Option.None", vec![])?;
                    self.store_reg(dst, RegVal::Value(none))?;
                }
            }
            return Ok(());
        }
        let suspend_len = match &slot.run {
            GeneratorRun::Suspended { frames, .. } => Some(frames.frames.len()),
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
            GeneratorRun::Fresh { seq, code } => self.push_call(
                code,
                [RegVal::Value(seq), RegVal::Value(Value::Unit)],
                End::GeneratorStep { gen_id, dst },
            ),
            GeneratorRun::Suspended {
                mut frames,
                owner,
                yield_dst,
            } => {
                if let Some(bottom) = frames.frames.first_mut() {
                    bottom.end = End::GeneratorStep { gen_id, dst };
                }
                self.current_owner = owner;
                self.restore_frames(frames);
                self.bump_handler_epoch();
                self.restore_evidence();
                self.store_reg(yield_dst, RegVal::Value(Value::Unit))
            }
            GeneratorRun::Running | GeneratorRun::Done => {
                slot.run = GeneratorRun::Done;
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store_reg(dst, RegVal::Value(none))
            }
        }
    }

    pub(super) fn exec_seq_of_list(&mut self, dst: usize, list_slot: usize) -> Result<(), Error> {
        self.stage_slots(&[list_slot])?;
        let list = self.scratch.pop().expect("list for Seq.of_list");
        let value = self.heap.allocate(Data::Seq(SeqData::FromList(list)));
        self.store_reg(dst, RegVal::Value(value))
    }

    pub(super) fn exec_seq_to_list(&mut self, dst: usize, seq_slot: usize) -> Result<(), Error> {
        self.stage_slots(&[seq_slot])?;
        let source = self.scratch.pop().expect("seq for Seq.to_list");
        if self.heap.list_view(&source).is_ok() {
            return self.store_reg(dst, RegVal::Value(source));
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
                let code_idx = *self
                    .image
                    .code_indices
                    .get(code)
                    .ok_or(Error::InvalidProgram(memory::Error::UnknownCode(*code)))?;
                Some((false, Value::Unit, code_idx, *limit))
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
            self.store_reg(dst, RegVal::Value(owned))
        } else {
            self.push_call(
                code,
                [RegVal::Value(source), RegVal::Value(Value::Unit)],
                End::GenerateStrict {
                    items: Vec::new(),
                    remaining: limit,
                    dst,
                },
            )
        }
    }

    pub(super) fn finish_generate_strict(
        &mut self,
        dst: usize,
        items: Vec<Value>,
    ) -> Result<(), Error> {
        let list = self.heap.construct("$list", items)?;
        self.store_reg(dst, RegVal::Value(list))
    }

    pub(super) fn finish_lazy_generator_eof(
        &mut self,
        gen_id: usize,
        dst: usize,
    ) -> Result<(), Error> {
        if let Some(slot) = self.generators.get_mut(&gen_id) {
            slot.run = GeneratorRun::Done;
        }
        let none = self.heap.construct("Prelude.Option.None", vec![])?;
        self.store_reg(dst, RegVal::Value(none))
    }

    pub(super) fn unwind_lazy_generator(&mut self, gen_id: usize) {
        if let Some(slot) = self.generators.get_mut(&gen_id) {
            slot.run = GeneratorRun::Done;
        }
    }
}
