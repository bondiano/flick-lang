//! Calls, tail-call frame reuse, loops, branches, and task awaiting for tier 1.

use flick_core::ir::memory;

use crate::tier0::{Data, Error, Value};

use super::machine::{End, Frame, GlobalState, Machine, RegVal};
use super::opcode::MAX_TAIL_ARGS;

impl Machine<'_, '_> {
    pub(super) fn push_call(
        &mut self,
        code: usize,
        args: impl IntoIterator<Item = RegVal, IntoIter: ExactSizeIterator>,
        end: End,
    ) -> Result<(), Error> {
        let args = args.into_iter();
        let actual = args.len();
        let function = &self.image.functions[code];
        if actual != function.params.len() {
            let mut roots = Vec::new();
            for arg in args {
                Self::reg_into_roots(arg, &mut roots);
            }
            self.drain_end_roots(end, &mut roots);
            self.heap.drop_values(roots)?;
            return Err(Error::ArgumentCount {
                expected: function.params.len(),
                actual,
            });
        }
        let recovery_id = match &end {
            End::RecoverySelection { id } | End::RecoveryHelper { id, .. } => Some(*id),
            _ => None,
        };
        let base = self.registers.len();
        self.registers.resize_with(base + function.slots, || None);
        for (&slot, arg) in function.params.iter().zip(args) {
            self.registers[base + slot] = Some(arg);
        }
        self.frames.push(Frame {
            code,
            pc: 0,
            base,
            vars: Vec::new(),
            iterators: Vec::new(),
            recovery_id,
            evidence: self.current_evidence,
            end,
        });
        Ok(())
    }

    fn is_tail_safe_end(end: &End) -> bool {
        !matches!(
            end,
            End::RecoverySelection { .. } | End::RecoveryHelper { .. }
        )
    }

    fn can_tail_call(&self) -> bool {
        let frame = self.frame();
        frame.vars.is_empty()
            && frame.iterators.is_empty()
            && frame.recovery_id.is_none()
            && Self::is_tail_safe_end(&frame.end)
            && self.registers[frame.base..].iter().all(Option::is_none)
    }

    fn reuse_frame(&mut self, code: usize, values: impl IntoIterator<Item = Option<RegVal>>) {
        let base = self.frame().base;
        let evidence = self.current_evidence;
        let function = &self.image.functions[code];
        self.registers.resize_with(base + function.slots, || None);
        for (&slot, value) in function.params.iter().zip(values) {
            self.registers[base + slot] = value;
        }
        let frame = self.frame_mut();
        frame.code = code;
        frame.pc = 0;
        frame.evidence = evidence;
    }

    pub(super) fn call_registers(
        &mut self,
        code: usize,
        args: &[usize],
        dst: usize,
        tail: bool,
    ) -> Result<(), Error> {
        if tail && args.len() <= MAX_TAIL_ARGS && self.can_tail_call_after_args(args) {
            let mut values = [const { None }; MAX_TAIL_ARGS];
            for (&src, slot) in args.iter().zip(&mut values) {
                *slot = Some(self.take_reg(src)?);
            }
            self.reuse_frame(code, values);
            return Ok(());
        }
        let end = match self.frame().recovery_id {
            Some(id) => End::RecoveryHelper { id, dst },
            None => End::Store(dst),
        };
        let mut vals = Vec::with_capacity(args.len());
        for &slot in args {
            vals.push(self.take_reg(slot)?);
        }
        self.push_call(code, vals, end)
    }

    fn can_tail_call_after_args(&self, args: &[usize]) -> bool {
        let frame = self.frame();
        if !frame.vars.is_empty()
            || !frame.iterators.is_empty()
            || frame.recovery_id.is_some()
            || !Self::is_tail_safe_end(&frame.end)
        {
            return false;
        }
        let occupied = self.registers[frame.base..]
            .iter()
            .filter(|slot| slot.is_some())
            .count();
        occupied == args.len()
    }

    pub(super) fn apply_reg(
        &mut self,
        callee: RegVal,
        argument: RegVal,
        end: End,
        tail: bool,
    ) -> Result<(), Error> {
        if let RegVal::Value(Value::Object(tok)) = &callee
            && let Ok(obj) = self.heap.object(tok.id)
        {
            match obj.data {
                Data::Continuation { execution, id } => {
                    return self.apply_continuation(callee, argument, execution, id, end, tail);
                }
                Data::MaskRestore {
                    execution,
                    outer_mask,
                } => {
                    if execution != self.execution {
                        let mut roots = Vec::new();
                        Self::reg_into_roots(callee, &mut roots);
                        Self::reg_into_roots(argument, &mut roots);
                        self.drain_end_roots(end, &mut roots);
                        self.heap.drop_values(roots)?;
                        return Err(Error::ForeignClosure);
                    }
                    let task = self.materialize_reg(argument);
                    self.drop_reg(callee)?;
                    let masked = self.heap.allocate(Data::MaskedTask {
                        target_mask: outer_mask,
                        task,
                    });
                    self.pending = Some(RegVal::Value(masked));
                    return self.deliver_end(end);
                }
                _ => {}
            }
        }
        let code = match self.checked_closure(&callee) {
            Ok(code) => code,
            Err(err) => {
                let mut roots = Vec::new();
                Self::reg_into_roots(callee, &mut roots);
                Self::reg_into_roots(argument, &mut roots);
                self.drain_end_roots(end, &mut roots);
                self.heap.drop_values(roots)?;
                return Err(err);
            }
        };
        if tail && matches!(end, End::Store(_)) && self.can_tail_call() {
            self.reuse_frame(code, [Some(callee), Some(argument)]);
            Ok(())
        } else {
            self.push_call(code, [callee, argument], end)
        }
    }

    pub(super) fn checked_closure(&self, callee: &RegVal) -> Result<usize, Error> {
        let code = match callee {
            RegVal::StaticClosure(code) => *code,
            RegVal::Value(Value::Object(tok)) => match &self.heap.object(tok.id)?.data {
                Data::Closure {
                    execution, code, ..
                } => {
                    if *execution != self.execution {
                        return Err(Error::ForeignClosure);
                    }
                    *self
                        .image
                        .code_indices
                        .get(code)
                        .ok_or(Error::InvalidProgram(memory::Error::UnknownCode(*code)))?
                }
                _ => return Err(Error::ExpectedClosure),
            },
            RegVal::Value(_) => return Err(Error::ExpectedClosure),
        };
        let expected = self.image.functions[code].params.len();
        if expected != 2 {
            return Err(Error::ArgumentCount {
                expected,
                actual: 2,
            });
        }
        Ok(code)
    }

    pub(super) fn load_global(&mut self, id: usize, dst: usize) -> Result<(), Error> {
        match &self.globals[id] {
            GlobalState::Ready(reg) => {
                let dup = match reg {
                    RegVal::StaticClosure(code) => RegVal::StaticClosure(*code),
                    RegVal::Value(val) => {
                        self.heap.admit_ordinary(val)?;
                        let rep = val.representation();
                        RegVal::Value(self.heap.dup(&rep)?)
                    }
                };
                self.store_reg(dst, dup)
            }
            GlobalState::Initializing => {
                Err(Error::GlobalCycle(self.image.global_names[id].clone()))
            }
            GlobalState::Empty => {
                self.globals[id] = GlobalState::Initializing;
                self.push_call(self.image.globals[id], [], End::Global { id, dst })
            }
        }
    }

    pub(super) fn return_reg(&mut self, reg: RegVal) -> Result<(), Error> {
        let is_recovery = matches!(
            self.frame().end,
            End::RecoverySelection { .. } | End::RecoveryHelper { .. }
        );
        self.pending = Some(reg);
        if let Some(RegVal::Value(val)) = self.pending.as_ref() {
            if is_recovery {
                self.heap.admit_not_reuse_token(val)?;
            } else {
                self.heap.admit_ordinary(val)?;
            }
        }
        let mut frame = self.frames.pop().expect("returning frame");
        let base = frame.base;
        let leftover: Vec<Value> = self
            .registers
            .drain(base..)
            .flatten()
            .filter_map(|r| match r {
                RegVal::Value(v) => Some(v),
                RegVal::StaticClosure(_) => None,
            })
            .collect();
        self.heap.drop_values(leftover)?;
        let gen_frames = self.release_frame_resources(&mut frame)?;
        self.restore_evidence();
        self.deliver_end(frame.end)?;
        self.queue_discard_frames(gen_frames);
        Ok(())
    }

    pub(super) fn deliver_end(&mut self, end: End) -> Result<(), Error> {
        match end {
            End::Top => {
                self.top = self.pending.take();
            }
            End::Store(dst) | End::FunctionBoundary { dst } => {
                let reg = self.pending.take().expect("returned value");
                self.store_reg(dst, reg)?;
            }
            End::Global { id, dst } => {
                let reg = self.pending.take().expect("global value");
                let cached = self.dup_reg(&reg)?;
                self.globals[id] = GlobalState::Ready(cached);
                if dst == usize::MAX {
                    self.top = Some(reg);
                } else {
                    self.store_reg(dst, reg)?;
                }
            }
            End::Modify { cell, dst } => self.complete_modify(&cell, dst)?,
            End::Loop(state) => {
                let ignored = self.pending.take().expect("loop iteration value");
                self.drop_reg(ignored)?;
                self.restart_loop(state)?;
            }
            End::GenerateStrict { items, dst, .. } => {
                let ignored = self.pending.take().expect("generator return");
                self.drop_reg(ignored)?;
                self.finish_generate_strict(dst, items)?;
            }
            End::GeneratorStep { gen_id, dst } => {
                let ignored = self.pending.take().expect("lazy generator return");
                self.drop_reg(ignored)?;
                self.finish_lazy_generator_eof(gen_id, dst)?;
            }
            End::LazyComplete { lazy, dst } => self.complete_lazy_ok(&lazy, dst)?,
            End::Try { selector, dst } => {
                self.pending_handler = Some(selector);
                let sel = self.pending_handler.take().expect("unused selector");
                self.drop_reg(sel)?;
                let reg = self.pending.take().expect("try result");
                self.store_reg(dst, reg)?;
            }
            End::RecoverySelection { id } => self.complete_recovery(id)?,
            End::RecoveryHelper { id, dst } => {
                let reg = self.pending.take().expect("helper result");
                self.store_transport(dst, reg, id)?;
            }
            End::RecoveryBody {
                dst,
                owner,
                evidence,
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
                let reg = self.pending.take().expect("selected body result");
                self.store_reg(dst, reg)?;
            }
            End::Handle { activation, dst } => self.handler_returned(activation, dst)?,
            End::Discontinue { cont_id, dst } => self.discontinue_returned(cont_id, dst)?,
            End::Invocation(id) => {
                self.invocations
                    .get_mut(&id)
                    .expect("running invocation")
                    .callee = self.pending.take();
                self.ready = Some(id);
            }
            other => self.deliver_scope_end(other)?,
        }
        Ok(())
    }
}
