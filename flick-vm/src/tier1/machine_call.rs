//! Calls, tail-call frame reuse, loops, branches, and `Option?` control flow for tier 1.

use std::mem;

use flick_core::ir::memory;

use crate::tier0::{CauseToken, Data, Error, Reason, Value};

use super::machine::{End, Frame, GlobalState, Machine, RegVal};
use super::opcode::{Instr, MAX_TAIL_ARGS};

impl Machine<'_, '_> {
    #[allow(clippy::too_many_lines)]
    pub(super) fn step_control(&mut self, instr: &Instr) -> Result<(), Error> {
        match instr {
            Instr::Global { dst, global } => self.load_global(*global, *dst)?,
            Instr::Call { dst, code, args } => self.call_registers(*code, args, *dst, false)?,
            Instr::TailCall { dst, code, args } => self.call_registers(*code, args, *dst, true)?,
            Instr::Apply {
                dst,
                function,
                argument,
            } => {
                let callee = self.take_reg(*function)?;
                let arg = self.take_reg(*argument)?;
                self.apply_reg(callee, arg, End::Store(*dst), false)?;
            }
            Instr::TailApply {
                dst,
                function,
                argument,
            } => {
                let callee = self.take_reg(*function)?;
                let arg = self.take_reg(*argument)?;
                self.apply_reg(callee, arg, End::Store(*dst), true)?;
            }
            Instr::Loop {
                dst,
                id,
                code,
                args,
            } => self.enter_loop(*dst, *id, *code, args)?,
            Instr::LoopExit { id, value } => self.exit_loop(*id, *value)?,
            Instr::FunctionBoundary { dst, code, args } => {
                let mut vals = Vec::with_capacity(args.len());
                for &slot in args {
                    vals.push(self.take_reg(slot)?);
                }
                self.push_call(*code, vals, End::FunctionBoundary { dst: *dst })?;
            }
            Instr::UnwrapOption { dst, option } => self.exec_unwrap_option(*dst, *option)?,
            Instr::Try {
                dst,
                body,
                selector,
            } => {
                let body_reg = self.take_reg(*body)?;
                let sel_reg = self.take_reg(*selector)?;
                self.pending_handler = Some(sel_reg);
                self.apply_reg(
                    body_reg,
                    RegVal::Value(Value::Unit),
                    End::Store(*dst),
                    false,
                )?;
                let sel = self.pending_handler.take().expect("staged selector");
                self.frame_mut().end = End::Try {
                    selector: sel,
                    dst: *dst,
                };
            }
            Instr::Handle {
                dst,
                body,
                operations,
                ret,
                errors,
            } => self.exec_handle(*dst, *body, operations, *ret, *errors)?,
            Instr::ScopeRun { dst, body } => self.exec_scope_run(*dst, *body)?,
            Instr::ScopeCurrent { dst, unit } => self.exec_scope_current(*dst, *unit)?,
            Instr::ScopeAddFinalizer { dst, task } => self.exec_scope_add_finalizer(*dst, *task)?,
            Instr::ScopeAddFinalizerExit { dst, func } => {
                self.exec_scope_add_finalizer_exit(*dst, *func)?;
            }
            Instr::ScopeAddClose { dst, func } => self.exec_scope_add_close(*dst, *func)?,
            Instr::AcquireRelease {
                dst,
                acquire,
                release,
            } => self.exec_acquire_release(*dst, *acquire, *release)?,
            Instr::Bracket {
                dst,
                acquire,
                use_fn,
                release,
            } => self.exec_bracket(*dst, *acquire, *use_fn, *release)?,
            Instr::Uninterruptible { dst, task } => self.exec_uninterruptible(*dst, *task)?,
            Instr::Recovery { dst, selected } => self.exec_recovery(*dst, *selected)?,
            Instr::FromResult { dst, src } => self.exec_from_result(*dst, *src)?,
            Instr::Raise { src } => {
                self.stage_slots(&[*src])?;
                self.raise_ordinary()?;
            }
            Instr::Die { src } => {
                self.stage_slots(&[*src])?;
                let msg = self.scratch.pop().expect("die message");
                let cause = self.heap.cause(&[Reason::Die(&msg)], &mut self.fuel);
                self.heap.drop_value(msg)?;
                self.failure = Some(cause?);
                self.failure_elected = false;
            }
            Instr::FailureRethrow { src } => {
                self.stage_slots(&[*src])?;
                let val = self.scratch.pop().expect("rethrow cause");
                let token = CauseToken::from_cause_value(self.heap, val)?;
                self.failure = Some(token);
                self.failure_elected = true;
            }
            Instr::MatchFail => return Err(Error::NoMatch),
            Instr::Return { value } => {
                let reg = self.take_reg(*value)?;
                self.return_reg(reg)?;
            }
            Instr::ReturnUnit => self.return_reg(RegVal::Value(Value::Unit))?,
            Instr::Branch {
                condition,
                then_pc,
                else_pc,
            } => {
                let val = self.take_value(*condition)?;
                let Value::Bool(cond) = val else {
                    self.scratch.push(val);
                    return Err(Error::ExpectedBoolean);
                };
                self.frame_mut().pc = if cond { *then_pc } else { *else_pc };
            }
            Instr::CompareBranch {
                op,
                left,
                right,
                then_pc,
                else_pc,
            } => {
                let val = self.eval_compute(*op, *left, *right)?;
                let Value::Bool(cond) = val else {
                    self.scratch.push(val);
                    return Err(Error::ExpectedBoolean);
                };
                self.frame_mut().pc = if cond { *then_pc } else { *else_pc };
            }
            _ => unreachable!("non-control instruction in step_control"),
        }
        Ok(())
    }

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
            recovery_id,
            end,
        });
        Ok(())
    }

    fn is_tail_safe_end(end: &End) -> bool {
        matches!(end, End::Top | End::Store(_) | End::Global { .. })
    }

    fn can_tail_call(&self) -> bool {
        let frame = self.frame();
        frame.vars.is_empty()
            && frame.recovery_id.is_none()
            && Self::is_tail_safe_end(&frame.end)
            && self.registers[frame.base..].iter().all(Option::is_none)
    }

    fn reuse_frame(&mut self, code: usize, values: impl IntoIterator<Item = Option<RegVal>>) {
        let base = self.frame().base;
        let function = &self.image.functions[code];
        self.registers.resize_with(base + function.slots, || None);
        for (&slot, value) in function.params.iter().zip(values) {
            self.registers[base + slot] = value;
        }
        let frame = self.frame_mut();
        frame.code = code;
        frame.pc = 0;
    }

    fn call_registers(
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
            && let Data::Continuation { execution, id } = obj.data
        {
            return self.apply_continuation(callee, argument, execution, id, end, tail);
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

    pub(super) fn await_task_reg(&mut self, task: RegVal, end: End) -> Result<(), Error> {
        let task_info = if let RegVal::Value(Value::Object(tok)) = &task {
            if let Data::Task {
                execution, code, ..
            } = &self.heap.object(tok.id)?.data
            {
                Some((*execution, *code))
            } else {
                None
            }
        } else {
            None
        };
        let Some((execution, ir_code)) = task_info else {
            let mut roots = Vec::new();
            Self::reg_into_roots(task, &mut roots);
            self.drain_end_roots(end, &mut roots);
            self.heap.drop_values(roots)?;
            return Err(Error::ExpectedTask);
        };
        if execution != self.execution {
            let mut roots = Vec::new();
            Self::reg_into_roots(task, &mut roots);
            self.drain_end_roots(end, &mut roots);
            self.heap.drop_values(roots)?;
            return Err(Error::ForeignTask);
        }
        let code = *self
            .image
            .code_indices
            .get(&ir_code)
            .ok_or(Error::InvalidProgram(memory::Error::UnknownCode(ir_code)))?;
        self.push_call(code, [task, RegVal::Value(Value::Unit)], end)
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

    fn load_global(&mut self, id: usize, dst: usize) -> Result<(), Error> {
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

    fn return_reg(&mut self, reg: RegVal) -> Result<(), Error> {
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
        let mut leftover: Vec<Value> = self
            .registers
            .drain(base..)
            .flatten()
            .filter_map(|r| match r {
                RegVal::Value(v) => Some(v),
                RegVal::StaticClosure(_) => None,
            })
            .collect();
        self.detach_frame_vars(mem::take(&mut frame.vars), &mut leftover);
        self.heap.drop_values(leftover)?;
        self.deliver_end(frame.end)
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
            End::Loop(state) => {
                let ignored = self.pending.take().expect("loop iteration value");
                self.drop_reg(ignored)?;
                self.restart_loop(state)?;
            }
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
            End::RecoveryBody { dst, owner } => {
                self.current_owner = owner;
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
