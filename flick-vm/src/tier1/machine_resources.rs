//! `AcquireRelease`, `Bracket`, `Uninterruptible`, `EffectExit`, `EffectCatchDefect`,
//! and scope/resource end delivery for tier 1.

use flick_core::ir::Lit;

use crate::stdlib::{FuelCounter, primitives};
use crate::tier0::{Data, DefectSpec, Error, ExitData};

use super::machine::{End, Machine, RegVal};
use super::machine_scopes::{BracketOutcome, Finalizer, ScopeOutcome};

impl Machine<'_, '_> {
    pub(super) fn exec_acquire_release(
        &mut self,
        dst: usize,
        acquire_slot: usize,
        release_slot: usize,
    ) -> Result<(), Error> {
        let acquire = self.take_reg(acquire_slot)?;
        let release = self.take_reg(release_slot)?;
        let owner = self.current_owner;
        if self.owners[owner].is_finalizing_or_closed() {
            let mut roots = Vec::new();
            Self::reg_into_roots(acquire, &mut roots);
            Self::reg_into_roots(release, &mut roots);
            self.heap.drop_values(roots)?;
            return self.raise_defect(DefectSpec::arena_owner_closed());
        }
        let saved_mask = self.mask_depth;
        self.mask_depth += 1;
        self.await_task_reg(
            acquire,
            End::RestoreMask {
                saved_mask,
                check_on_exit: true,
                next: Box::new(End::AcquireRegister {
                    owner,
                    release,
                    dst,
                }),
            },
        )
    }

    pub(super) fn exec_bracket(
        &mut self,
        dst: usize,
        acquire_slot: usize,
        use_slot: usize,
        release_slot: usize,
    ) -> Result<(), Error> {
        let acquire = self.take_reg(acquire_slot)?;
        let use_fn = self.take_reg(use_slot)?;
        let release_fn = self.take_reg(release_slot)?;
        let saved_mask = self.mask_depth;
        self.mask_depth += 1;
        self.await_task_reg(
            acquire,
            End::RestoreMask {
                saved_mask,
                check_on_exit: true,
                next: Box::new(End::BracketUse {
                    use_fn,
                    release_fn,
                    dst,
                }),
            },
        )
    }

    pub(super) fn exec_uninterruptible(
        &mut self,
        dst: usize,
        task_slot: usize,
    ) -> Result<(), Error> {
        let task = self.take_reg(task_slot)?;
        let saved_mask = self.mask_depth;
        self.mask_depth += 1;
        self.await_task_reg(
            task,
            End::RestoreMask {
                saved_mask,
                check_on_exit: true,
                next: Box::new(End::Store(dst)),
            },
        )
    }

    pub(super) fn exec_effect_exit(&mut self, dst: usize, task_slot: usize) -> Result<(), Error> {
        let task = self.take_reg(task_slot)?;
        self.await_task_reg(task, End::CaptureExit { dst })
    }

    pub(super) fn exec_effect_catch_defect(
        &mut self,
        dst: usize,
        handler_slot: usize,
        task_slot: usize,
    ) -> Result<(), Error> {
        let handler = self.take_reg(handler_slot)?;
        let task = self.take_reg(task_slot)?;
        self.pending_handler = Some(handler);
        self.await_task_reg(task, End::Store(dst))?;
        if let Some(h) = self.pending_handler.take() {
            if let Some(frame) = self.frames.last_mut()
                && matches!(frame.end, End::Store(d) if d == dst)
            {
                frame.end = End::CatchDefect { handler: h, dst };
            } else {
                self.drop_reg(h)?;
            }
        }
        Ok(())
    }

    pub(super) fn skip_interrupted_handler(&mut self) -> Result<bool, Error> {
        let Some(by) = self.interrupt_requested else {
            return Ok(false);
        };
        if self.mask_depth > 0 {
            return Ok(false);
        }
        let prev = self.failure.take();
        let fid = usize::try_from(by).ok();
        let res = primitives::strip_fail_and_add_interrupt(
            self.heap,
            prev.as_ref(),
            fid,
            &mut FuelCounter(&mut self.fuel),
        );
        if let Some(p) = prev {
            self.heap.drop_cause(p)?;
        }
        self.failure = Some(res?);
        self.failure_elected = true;
        Ok(true)
    }

    pub(super) fn finish_masked_cleanup_ok(
        &mut self,
        outcome: BracketOutcome,
        saved_mask: usize,
        dst: usize,
    ) -> Result<(), Error> {
        self.mask_depth = saved_mask;
        match outcome {
            BracketOutcome::Success(val) => match self.check_active_interrupt() {
                Ok(true) => self.drop_reg(val),
                Ok(false) => self.store_reg(dst, val),
                Err(err) => {
                    self.drop_reg(val)?;
                    Err(err)
                }
            },
            BracketOutcome::Failure(cause) => {
                self.failure = Some(cause);
                self.failure_elected = true;
                Ok(())
            }
        }
    }

    pub(super) fn finish_masked_cleanup_err(
        &mut self,
        outcome: BracketOutcome,
        saved_mask: usize,
    ) -> Result<(), Error> {
        self.mask_depth = saved_mask;
        match outcome {
            BracketOutcome::Success(val) => {
                self.drop_reg(val)?;
                self.failure_elected = false;
                Ok(())
            }
            BracketOutcome::Failure(orig) => {
                let joined = if let Some(secondary) = self.failure.take() {
                    let res = primitives::join_causes(
                        self.heap,
                        &orig,
                        &secondary,
                        &mut FuelCounter(&mut self.fuel),
                    );
                    self.heap.drop_cause(orig)?;
                    self.heap.drop_cause(secondary)?;
                    res?
                } else {
                    orig
                };
                self.failure = Some(joined);
                self.failure_elected = false;
                Ok(())
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn deliver_scope_end(&mut self, end: End) -> Result<(), Error> {
        match end {
            End::CloseScope {
                owner,
                previous,
                dst,
            } => {
                let value = self.pending.take().expect("scope result");
                self.begin_close_scope(owner, previous, ScopeOutcome::Normal { dst, value }, None)
            }
            End::ClosingScope { state } => {
                let unit = self.pending.take().expect("finalizer return");
                self.drop_reg(unit)?;
                self.step_close_scope(state)
            }
            End::RestoreOwner { previous, dst } => {
                self.current_owner = previous;
                let value = self.pending.take().expect("selected-owner result");
                self.store_reg(dst, value)
            }
            End::Modify { cell, dst } => self.complete_modify(&cell, dst),
            End::GenerateStrict { items, dst, .. } => {
                let ignored = self.pending.take().expect("strict generator return");
                self.drop_reg(ignored)?;
                let list = self.heap.construct("$list", items)?;
                self.store_reg(dst, RegVal::Value(list))
            }
            End::GeneratorStep { gen_id, dst } => {
                let ignored = self.pending.take().expect("lazy generator return");
                self.drop_reg(ignored)?;
                if let Some(slot) = self.generators.get_mut(&gen_id) {
                    slot.run = super::machine_generators::GeneratorRun::Done;
                }
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store_reg(dst, RegVal::Value(none))
            }
            End::LazyComplete { lazy, dst } => self.complete_lazy_ok(&lazy, dst),
            End::CaptureExit { dst } => {
                let reg = self.pending.take().expect("exit result");
                let value = self.materialize_reg(reg);
                let exit = self.heap.allocate(Data::Exit(ExitData::Success(value)));
                self.store_reg(dst, RegVal::Value(exit))
            }
            End::CatchDefect { handler, dst } => {
                self.pending_handler = Some(handler);
                let h = self.pending_handler.take().expect("unused defect handler");
                self.drop_reg(h)?;
                let val = self.pending.take().expect("catch_defect ok");
                self.store_reg(dst, val)
            }
            End::LayerRun { provider, dst } => {
                let value = self.pending.take().expect("layer result");
                self.leave_provider_outcome(
                    provider,
                    ScopeOutcome::Normal {
                        dst: Some(dst),
                        value,
                    },
                )
            }
            End::AcquireRegister {
                owner,
                release,
                dst,
            } => {
                let reg = self.pending.take().expect("acquired resource");
                let value = self.materialize_reg(reg);
                let dup = match self.heap.dup(&value) {
                    Ok(dup) => dup,
                    Err(err) => {
                        let mut roots = vec![value];
                        Self::reg_into_roots(release, &mut roots);
                        self.heap.drop_values(roots)?;
                        return Err(err);
                    }
                };
                let services = self.snapshot_active_services()?;
                self.owners[owner].finalizers.push(Finalizer::Release {
                    value: dup,
                    release_fn: release,
                    services,
                });
                self.store_reg(dst, RegVal::Value(value))
            }
            End::BracketUse {
                use_fn,
                release_fn,
                dst,
            } => {
                let reg = self.pending.take().expect("bracket acquired resource");
                let resource = self.materialize_reg(reg);
                let saved = match self.heap.dup(&resource) {
                    Ok(dup) => dup,
                    Err(err) => {
                        let mut roots = vec![resource];
                        Self::reg_into_roots(use_fn, &mut roots);
                        Self::reg_into_roots(release_fn, &mut roots);
                        self.heap.drop_values(roots)?;
                        return Err(err);
                    }
                };
                self.apply_reg(
                    use_fn,
                    RegVal::Value(resource),
                    End::BracketRelease {
                        resource: saved,
                        release_fn,
                        dst,
                    },
                    false,
                )
            }
            End::BracketRelease {
                resource,
                release_fn,
                dst,
            } => {
                let value = self.pending.take().expect("bracket use result");
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.apply_reg(
                    release_fn,
                    RegVal::Value(resource),
                    End::BracketFinish {
                        outcome: BracketOutcome::Success(value),
                        saved_mask,
                        dst,
                    },
                    false,
                )
            }
            End::BracketFinish {
                outcome,
                saved_mask,
                dst,
            } => {
                let ignored = self.pending.take().expect("bracket release result");
                self.drop_reg(ignored)?;
                self.finish_masked_cleanup_ok(outcome, saved_mask, dst)
            }
            End::DiscardFinalizer => {
                let ignored = self.pending.take().expect("discarded result");
                self.drop_reg(ignored)
            }
            other => self.deliver_async_end(other),
        }
    }

    pub(super) fn unwind_scope_end(&mut self, end: End) -> Result<(), Error> {
        match end {
            End::CloseScope {
                owner, previous, ..
            } => {
                let cause = self.failure.take().expect("scope failure cause");
                self.begin_close_scope(owner, previous, ScopeOutcome::Failure(cause), None)
            }
            End::ClosingScope { state } => self.finalizer_failed(state),
            End::RestoreOwner { previous, .. } => {
                self.current_owner = previous;
                Ok(())
            }
            End::Modify { .. } => Ok(()),
            End::GenerateStrict { items, .. } => self.heap.drop_values(items),
            End::GeneratorStep { gen_id, .. } => {
                if let Some(slot) = self.generators.get_mut(&gen_id) {
                    slot.run = super::machine_generators::GeneratorRun::Done;
                }
                Ok(())
            }
            End::LazyComplete { lazy, .. } => self.unwind_lazy(&lazy),
            End::CaptureExit { dst } => {
                if self.skip_interrupted_handler()? {
                    return Ok(());
                }
                let cause = self.failure.take().expect("captured exit failure");
                self.failure_elected = false;
                let exit = self
                    .heap
                    .allocate(Data::Exit(ExitData::Failure(cause.into_value())));
                self.store_reg(dst, RegVal::Value(exit))
            }
            End::CatchDefect { handler, dst } => {
                if self.skip_interrupted_handler()? {
                    return self.drop_reg(handler);
                }
                self.pending_handler = Some(handler);
                let defect_msg = self
                    .failure
                    .as_ref()
                    .map(|c| primitives::first_defect_message(self.heap, c.object_id()))
                    .transpose()?
                    .flatten();
                let handler = self.pending_handler.take().expect("defect handler");
                if let Some(msg) = defect_msg {
                    if let Some(cause) = self.failure.take() {
                        self.heap.drop_cause(cause)?;
                    }
                    self.failure_elected = false;
                    let msg_val = self.heap.literal(&Lit::String(msg))?;
                    self.apply_reg(handler, RegVal::Value(msg_val), End::Store(dst), false)
                } else {
                    self.drop_reg(handler)
                }
            }
            End::LayerRun { provider, .. } => {
                let cause = self.failure.take().expect("provider failure cause");
                self.leave_provider_outcome(provider, ScopeOutcome::Failure(cause))
            }
            End::AcquireRegister { release, .. } => self.drop_reg(release),
            End::BracketUse {
                use_fn, release_fn, ..
            } => {
                let mut roots = Vec::new();
                Self::reg_into_roots(use_fn, &mut roots);
                Self::reg_into_roots(release_fn, &mut roots);
                self.heap.drop_values(roots)
            }
            End::BracketRelease {
                resource,
                release_fn,
                dst,
            } => {
                let cause = self.failure.take().expect("bracket body failure");
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.apply_reg(
                    release_fn,
                    RegVal::Value(resource),
                    End::BracketFinish {
                        outcome: BracketOutcome::Failure(cause),
                        saved_mask,
                        dst,
                    },
                    false,
                )
            }
            End::BracketFinish {
                outcome,
                saved_mask,
                ..
            } => self.finish_masked_cleanup_err(outcome, saved_mask),
            other => self.unwind_async_end(other),
        }
    }
}
