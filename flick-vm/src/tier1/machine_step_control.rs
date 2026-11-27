//! Control-instruction dispatch and `Task`/`MaskedTask`/`CachedTask`/`ExitTask`
//! awaiting for tier 1.

use std::mem;

use flick_core::ir::memory;

use crate::tier0::{CauseToken, Data, Error, ExitData, ObjectId, Reason, Value};

use super::machine::{End, Machine, RegVal};
use super::machine_coords::CachedEntry;
use super::opcode::Instr;

enum AwaitTaskKind {
    Code(memory::CodeId),
    Masked { target_mask: usize, task_rep: Value },
    CachedRun(usize),
    CachedInvalidate(usize),
    ExitOk(Value),
    ExitErr(Value),
}

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
            Instr::Await { dst, task } => {
                let task_reg = self.take_reg(*task)?;
                self.await_task_reg(task_reg, End::Store(*dst))?;
            }
            Instr::Within {
                dst,
                owner,
                code,
                args,
            } => self.exec_within(*dst, *owner, *code, args)?,
            Instr::WithOwnerEvidence {
                dst,
                owner,
                code,
                args,
            } => self.exec_with_owner_evidence(*dst, *owner, *code, args)?,
            Instr::GenerateStrict { dst, code, args } => {
                self.exec_generate_strict(*dst, *code, args)?;
            }
            Instr::GenerateLazy {
                dst,
                code,
                captures,
                limit,
            } => self.exec_generate_lazy(*dst, *code, captures, *limit)?,
            Instr::Yield { dst, value } => self.exec_yield(*dst, *value)?,
            Instr::SeqOpen { dst, source } => self.exec_seq_open(*dst, *source)?,
            Instr::SeqNext { dst, iterator } => self.exec_seq_next(*dst, *iterator)?,
            Instr::SeqOfList { dst, list } => self.exec_seq_of_list(*dst, *list)?,
            Instr::SeqToList { dst, seq } => self.exec_seq_to_list(*dst, *seq)?,
            Instr::RefMake { dst, src } => self.exec_ref_make(*dst, *src)?,
            Instr::RefGet { dst, cell } => self.exec_ref_get(*dst, *cell)?,
            Instr::RefSet { dst, cell, value } => self.exec_ref_set(*dst, *cell, *value)?,
            Instr::RefModify { dst, cell, func } => self.exec_ref_modify(*dst, *cell, *func)?,
            Instr::RefSame { dst, left, right } => self.exec_ref_same(*dst, *left, *right)?,
            Instr::ArenaMake { dst, unit } => self.exec_arena_make(*dst, *unit)?,
            Instr::ArenaAlloc { dst, arena, value } => {
                self.exec_arena_alloc(*dst, *arena, *value)?;
            }
            Instr::ArenaGet { dst, view } => self.exec_arena_get(*dst, *view)?,
            Instr::LazyMake { dst, task, local } => self.exec_lazy_make(*dst, *task, *local)?,
            Instr::LazyForce { dst, lazy } => self.exec_lazy_force(*dst, *lazy)?,
            Instr::EffectExit { dst, task } => self.exec_effect_exit(*dst, *task)?,
            Instr::EffectCatchDefect { dst, handler, task } => {
                self.exec_effect_catch_defect(*dst, *handler, *task)?;
            }
            Instr::LayerSucceed {
                dst,
                tag,
                implementation,
            } => self.exec_layer_succeed(*dst, *tag, *implementation)?,
            Instr::LayerMerge { dst, left, right } => self.exec_layer_merge(*dst, *left, *right)?,
            Instr::LayerRun { dst, layer, body } => self.exec_layer_run(*dst, *layer, *body)?,
            Instr::AsyncOp { dst, op, args } => self.exec_async_op(*dst, *op, args)?,
            Instr::LayerOp { dst, op, args } => self.exec_layer_op(*dst, *op, args)?,
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

    fn await_cached_run(&mut self, cell_id: usize, end: End) -> Result<(), Error> {
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
            self.pending = Some(RegVal::Value(val));
            return self.deliver_end(end);
        }
        let task_rep = self.cached_cells[&cell_id].task.representation();
        let task = self.heap.dup(&task_rep)?;
        self.await_task_reg(
            RegVal::Value(task),
            End::CachedComplete {
                cell_id,
                next: Box::new(end),
            },
        )
    }

    pub(super) fn await_task_reg(&mut self, task: RegVal, end: End) -> Result<(), Error> {
        if self.check_active_interrupt()? {
            self.drop_reg(task)?;
            return self.fail_end(end);
        }
        let kind = match &task {
            RegVal::Value(Value::Object(tok)) => self.classify_task(tok.id),
            _ => Err(Error::ExpectedTask),
        };
        let kind = match kind {
            Ok(k) => k,
            Err(err) => {
                let mut roots = Vec::new();
                Self::reg_into_roots(task, &mut roots);
                self.drain_end_roots(end, &mut roots);
                self.heap.drop_values(roots)?;
                return Err(err);
            }
        };
        match kind {
            AwaitTaskKind::Code(ir_code) => {
                let code = *self
                    .image
                    .code_indices
                    .get(&ir_code)
                    .ok_or(Error::InvalidProgram(memory::Error::UnknownCode(ir_code)))?;
                self.push_call(code, [task, RegVal::Value(Value::Unit)], end)
            }
            AwaitTaskKind::Masked {
                target_mask,
                task_rep,
            } => {
                let inner_task = self.heap.dup(&task_rep)?;
                self.drop_reg(task)?;
                let saved_mask = self.mask_depth;
                self.mask_depth = target_mask;
                self.await_task_reg(
                    RegVal::Value(inner_task),
                    End::RestoreMask {
                        saved_mask,
                        check_on_exit: true,
                        next: Box::new(end),
                    },
                )
            }
            AwaitTaskKind::CachedRun(cell_id) => {
                self.drop_reg(task)?;
                self.await_cached_run(cell_id, end)
            }
            AwaitTaskKind::CachedInvalidate(cell_id) => {
                self.drop_reg(task)?;
                if let Some(cell) = self.cached_cells.get_mut(&cell_id) {
                    let old = mem::replace(&mut cell.entry, CachedEntry::Empty);
                    if let CachedEntry::Ready { value, .. } = old {
                        self.heap.drop_value(value)?;
                    }
                }
                self.pending = Some(RegVal::Value(Value::Unit));
                self.deliver_end(end)
            }
            AwaitTaskKind::ExitOk(rep) => {
                let val = self.heap.dup(&rep)?;
                self.drop_reg(task)?;
                self.pending = Some(RegVal::Value(val));
                self.deliver_end(end)
            }
            AwaitTaskKind::ExitErr(rep) => {
                let cval = self.heap.dup(&rep)?;
                self.drop_reg(task)?;
                let cause = CauseToken::from_cause_value(self.heap, cval)?;
                self.failure = Some(cause);
                self.failure_elected = true;
                self.fail_end(end)
            }
        }
    }
}
