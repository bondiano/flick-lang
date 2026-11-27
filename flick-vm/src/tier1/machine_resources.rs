//! `AcquireRelease`, `Bracket`, `Uninterruptible`, and scope/resource end delivery for tier 1.

use crate::stdlib::{FuelCounter, primitives};
use crate::tier0::{DefectSpec, Error};

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
                next: Box::new(End::Store(dst)),
            },
        )
    }

    pub(super) fn finish_masked_cleanup_ok(
        &mut self,
        outcome: BracketOutcome,
        saved_mask: usize,
        dst: usize,
    ) -> Result<(), Error> {
        self.mask_depth = saved_mask;
        match outcome {
            BracketOutcome::Success(val) => self.store_reg(dst, val),
            BracketOutcome::Failure(cause) => {
                self.failure = Some(cause);
                self.failure_elected = true;
                Ok(())
            }
        }
    }

    fn finish_masked_cleanup_err(
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
                self.owners[owner].finalizers.push(Finalizer::Release {
                    value: dup,
                    release_fn: release,
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
            End::RestoreMask { saved_mask, next } => {
                self.mask_depth = saved_mask;
                self.deliver_end(*next)
            }
            End::DiscardCleanupDone {
                saved_mask,
                masked,
                paused,
            } => {
                let ignored = self.pending.take().expect("discard cleanup return");
                self.drop_reg(ignored)?;
                if masked {
                    self.mask_depth = saved_mask;
                }
                self.discard = Some(*paused);
                Ok(())
            }
            End::DiscardFinalizer => {
                let ignored = self.pending.take().expect("discarded result");
                self.drop_reg(ignored)
            }
            _ => unreachable!("non-scope end in deliver_scope_end"),
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
            End::RestoreMask { saved_mask, next } => {
                self.mask_depth = saved_mask;
                self.fail_end(*next)
            }
            End::DiscardCleanupDone {
                saved_mask,
                masked,
                paused,
            } => {
                if masked {
                    self.mask_depth = saved_mask;
                }
                let fin_cause = self.failure.take().expect("discard cleanup failure");
                self.escalate_paused_discard(*paused, fin_cause)
            }
            _ => unreachable!("non-scope end in unwind_scope_end"),
        }
    }
}
