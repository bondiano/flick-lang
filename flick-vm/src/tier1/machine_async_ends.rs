//! Async, masking, error-combinator, context, and layer-build end delivery and
//! unwinding for tier 1.

use std::mem;

use crate::stdlib::{self, FuelCounter, primitives};
use crate::tier0::{CauseToken, Data, DurationData, Error, ExitData, Reason, Value};

use super::machine::{End, Machine, RegVal};
use super::machine_coords::CachedEntry;
use super::machine_scopes::{BracketOutcome, ScopeOutcome};

impl Machine<'_, '_> {
    #[allow(clippy::too_many_lines)]
    pub(super) fn deliver_async_end(&mut self, end: End) -> Result<(), Error> {
        match end {
            End::RestoreMask {
                saved_mask,
                check_on_exit,
                next,
            } => {
                self.mask_depth = saved_mask;
                if check_on_exit && self.check_active_interrupt()? {
                    if let Some(v) = self.pending.take() {
                        self.drop_reg(v)?;
                    }
                    self.fail_end(*next)
                } else {
                    self.deliver_end(*next)
                }
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
            End::OnExitDiscardRunFin {
                saved_mask,
                masked,
                paused,
            } => {
                let opt = self.pending.take().expect("discard decide option");
                let opt_val = self.materialize_reg(opt);
                if let Some(task) = stdlib::runtime::coop::extract_opt_value(self.heap, opt_val)? {
                    self.await_task_reg(
                        RegVal::Value(task),
                        End::DiscardCleanupDone {
                            saved_mask,
                            masked,
                            paused,
                        },
                    )
                } else {
                    if masked {
                        self.mask_depth = saved_mask;
                    }
                    self.discard = Some(*paused);
                    Ok(())
                }
            }
            End::EnsuringFinish { finalizer, dst } => {
                let val = self.pending.take().expect("ensuring result");
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.await_task_reg(
                    finalizer,
                    End::EnsuringDone {
                        outcome: BracketOutcome::Success(val),
                        saved_mask,
                        dst,
                    },
                )
            }
            End::EnsuringDone {
                outcome,
                saved_mask,
                dst,
            }
            | End::OnExitFinish {
                outcome,
                saved_mask,
                dst,
            } => {
                let ignored = self.pending.take().expect("cleanup return");
                self.drop_reg(ignored)?;
                self.finish_masked_cleanup_ok(outcome, saved_mask, dst)
            }
            End::OnExitDecide {
                interruptible,
                decide,
                dst,
            } => {
                let reg = self.pending.take().expect("on_exit result");
                let val = self.materialize_reg(reg);
                let saved_mask = self.mask_depth;
                if !interruptible {
                    self.mask_depth += 1;
                }
                let dup = self.heap.dup(&val)?;
                let exit_val = self.heap.allocate(Data::Exit(ExitData::Success(dup)));
                self.apply_reg(
                    decide,
                    RegVal::Value(exit_val),
                    End::OnExitRunFin {
                        outcome: BracketOutcome::Success(RegVal::Value(val)),
                        saved_mask,
                        dst,
                    },
                    false,
                )
            }
            End::OnExitRunFin {
                outcome,
                saved_mask,
                dst,
            } => {
                let opt = self.pending.take().expect("decide option");
                let opt_val = self.materialize_reg(opt);
                if let Some(task) = stdlib::runtime::coop::extract_opt_value(self.heap, opt_val)? {
                    self.await_task_reg(
                        RegVal::Value(task),
                        End::OnExitFinish {
                            outcome,
                            saved_mask,
                            dst,
                        },
                    )
                } else {
                    self.finish_masked_cleanup_ok(outcome, saved_mask, dst)
                }
            }
            End::CatchCause { handler, dst } => {
                self.drop_reg(handler)?;
                let val = self.pending.take().expect("catch_cause ok");
                self.store_reg(dst, val)
            }
            End::OrDie { dst } => {
                let val = self.pending.take().expect("or_die ok");
                self.store_reg(dst, val)
            }
            End::TimedFinish { start_ns, dst } => {
                let reg = self.pending.take().expect("timed ok");
                let val = self.materialize_reg(reg);
                let elapsed = self.virtual_now_ns.saturating_sub(start_ns);
                let dur = self.heap.allocate(Data::Duration(DurationData::Nanos(
                    num_bigint::BigInt::from(elapsed),
                )));
                let pair = self.heap.construct("$tuple", vec![dur, val])?;
                self.store_reg(dst, RegVal::Value(pair))
            }
            End::CachedComplete { cell_id, next } => self.complete_cached_cell(cell_id, *next),
            End::ProvideContext { context, dst } => {
                self.bump_handler_epoch();
                self.heap.drop_value(context)?;
                let val = self.pending.take().expect("provide context ok");
                self.store_reg(dst, val)
            }
            End::WithServices { services, next } => {
                self.bump_handler_epoch();
                self.heap.drop_values(services)?;
                self.deliver_end(*next)
            }
            End::RestoreFiberRef { key, previous, dst } => {
                let old = match previous {
                    Some(v) => self.fiber_refs.insert(key, v),
                    None => self.fiber_refs.remove(&key),
                };
                if let Some(v) = old {
                    self.heap.drop_value(v)?;
                }
                let val = self.pending.take().expect("with_ref ok");
                self.store_reg(dst, val)
            }
            End::RestoreOwnerEvidence {
                previous,
                evidence,
                dst,
            } => {
                self.current_owner = previous;
                self.current_evidence = evidence;
                let val = self.pending.take().expect("owner_run ok");
                self.store_reg(dst, val)
            }
            End::RestoreEnvRefs { previous, dst } => {
                let old = mem::replace(&mut self.fiber_refs, previous);
                self.heap.drop_values(old.into_values().collect())?;
                let val = self.pending.take().expect("env_run ok");
                self.store_reg(dst, val)
            }
            End::RestoreRandom {
                previous,
                rng_id,
                dst,
            } => {
                self.seeded_random = previous;
                self.isaac_rngs.remove(&rng_id);
                let val = self.pending.take().expect("with_seed ok");
                self.store_reg(dst, val)
            }
            End::LayerBuildFinish {
                inner_owner,
                parent_scope,
                prev_owner,
                prev_ev,
                prev_memo,
                memo_entry,
                target,
                ..
            } => self.complete_layer_build_ok(
                inner_owner,
                parent_scope,
                prev_owner,
                prev_ev,
                prev_memo,
                memo_entry,
                target,
            ),
            _ => unreachable!("non-async end in deliver_async_end"),
        }
    }

    fn complete_cached_cell(&mut self, cell_id: usize, next: End) -> Result<(), Error> {
        let reg = self.pending.take().expect("cached ok");
        let val = self.materialize_reg(reg);
        let dup = self.heap.dup(&val)?;
        if let Some(cell) = self.cached_cells.get_mut(&cell_id) {
            let expires_ns = cell
                .ttl_ns
                .map(|ttl| self.virtual_now_ns.saturating_add(ttl));
            let old = mem::replace(
                &mut cell.entry,
                CachedEntry::Ready {
                    value: dup,
                    expires_ns,
                },
            );
            if let CachedEntry::Ready { value, .. } = old {
                self.heap.drop_value(value)?;
            }
        } else {
            self.heap.drop_value(dup)?;
        }
        self.pending = Some(RegVal::Value(val));
        self.deliver_end(next)
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn unwind_async_end(&mut self, end: End) -> Result<(), Error> {
        match end {
            End::RestoreMask {
                saved_mask, next, ..
            } => {
                self.mask_depth = saved_mask;
                self.fail_end(*next)
            }
            End::DiscardCleanupDone {
                saved_mask,
                masked,
                paused,
            }
            | End::OnExitDiscardRunFin {
                saved_mask,
                masked,
                paused,
            } => {
                if masked {
                    self.mask_depth = saved_mask;
                }
                let fin_cause = self.failure.take().expect("discard cleanup failure");
                self.escalate_paused_discard(*paused, fin_cause, false)
            }
            End::EnsuringFinish { finalizer, dst } => {
                let cause = self.failure.take().expect("ensuring failure");
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.await_task_reg(
                    finalizer,
                    End::EnsuringDone {
                        outcome: BracketOutcome::Failure(cause),
                        saved_mask,
                        dst,
                    },
                )
            }
            End::EnsuringDone {
                outcome,
                saved_mask,
                ..
            }
            | End::OnExitRunFin {
                outcome,
                saved_mask,
                ..
            }
            | End::OnExitFinish {
                outcome,
                saved_mask,
                ..
            } => self.finish_masked_cleanup_err(outcome, saved_mask),
            End::OnExitDecide {
                interruptible,
                decide,
                dst,
            } => {
                let cause = self.failure.take().expect("on_exit failure");
                let saved_mask = self.mask_depth;
                if !interruptible {
                    self.mask_depth += 1;
                }
                let dup = self.heap.dup_cause(&cause)?;
                let exit_val = self
                    .heap
                    .allocate(Data::Exit(ExitData::Failure(dup.into_value())));
                self.apply_reg(
                    decide,
                    RegVal::Value(exit_val),
                    End::OnExitRunFin {
                        outcome: BracketOutcome::Failure(cause),
                        saved_mask,
                        dst,
                    },
                    false,
                )
            }
            End::CatchCause { handler, dst } => {
                if self.skip_interrupted_handler()? {
                    return self.drop_reg(handler);
                }
                let cause = self.failure.take().expect("catch_cause failure");
                self.failure_elected = false;
                self.apply_reg(
                    handler,
                    RegVal::Value(cause.into_value()),
                    End::Store(dst),
                    false,
                )
            }
            End::OrDie { .. } => {
                let cause = self.failure.take().expect("or_die failure");
                let converted = primitives::fails_to_dies_cause(
                    self.heap,
                    &cause,
                    &mut FuelCounter(&mut self.fuel),
                );
                self.heap.drop_cause(cause)?;
                self.failure = Some(converted?);
                self.failure_elected = true;
                Ok(())
            }
            End::TimedFinish { .. } => Ok(()),
            End::CachedComplete { next, .. } => self.fail_end(*next),
            End::ProvideContext { context, .. } => {
                self.bump_handler_epoch();
                self.heap.drop_value(context)
            }
            End::WithServices { services, next } => {
                self.bump_handler_epoch();
                self.heap.drop_values(services)?;
                self.fail_end(*next)
            }
            End::RestoreFiberRef { key, previous, .. } => {
                let old = match previous {
                    Some(v) => self.fiber_refs.insert(key, v),
                    None => self.fiber_refs.remove(&key),
                };
                if let Some(v) = old {
                    self.heap.drop_value(v)?;
                }
                Ok(())
            }
            End::RestoreOwnerEvidence {
                previous, evidence, ..
            } => {
                self.current_owner = previous;
                self.current_evidence = evidence;
                Ok(())
            }
            End::RestoreEnvRefs { previous, .. } => {
                let old = mem::replace(&mut self.fiber_refs, previous);
                self.heap.drop_values(old.into_values().collect())
            }
            End::RestoreRandom {
                previous, rng_id, ..
            } => {
                self.seeded_random = previous;
                self.isaac_rngs.remove(&rng_id);
                Ok(())
            }
            End::LayerBuildFinish {
                inner_owner,
                prev_owner,
                prev_ev,
                prev_memo,
                memo_entry,
                or_die,
                target,
                ..
            } => self.complete_layer_build_err(
                inner_owner,
                prev_owner,
                prev_ev,
                prev_memo,
                memo_entry,
                or_die,
                target,
            ),
            _ => unreachable!("non-async end in unwind_async_end"),
        }
    }

    pub(super) fn is_cutoff_cleanup_end(&self, end: &End) -> Result<bool, Error> {
        let has_int = |c: &CauseToken| -> Result<bool, Error> {
            Ok(self
                .heap
                .cause_reasons(c)?
                .any(|r| matches!(r, Reason::Interrupt { .. })))
        };
        match end {
            End::DiscardCleanupDone { .. } | End::OnExitDiscardRunFin { .. } => Ok(true),
            End::ClosingScope { state } => match &state.outcome {
                ScopeOutcome::Discard(_) => Ok(true),
                ScopeOutcome::Failure(c)
                | ScopeOutcome::ChildClose {
                    inherited_exit: Some(c),
                    ..
                } => has_int(c),
                ScopeOutcome::Normal { .. }
                | ScopeOutcome::ChildClose {
                    inherited_exit: None,
                    ..
                } => Ok(false),
            },
            End::EnsuringDone {
                outcome: BracketOutcome::Failure(c),
                ..
            }
            | End::OnExitRunFin {
                outcome: BracketOutcome::Failure(c),
                ..
            }
            | End::OnExitFinish {
                outcome: BracketOutcome::Failure(c),
                ..
            }
            | End::BracketFinish {
                outcome: BracketOutcome::Failure(c),
                ..
            } => has_int(c),
            _ => Ok(false),
        }
    }

    pub(super) fn complete_cutoff_end(&mut self, end: End) -> Result<(), Error> {
        match end {
            End::DiscardCleanupDone {
                saved_mask,
                masked,
                paused,
            }
            | End::OnExitDiscardRunFin {
                saved_mask,
                masked,
                paused,
            } => {
                if masked {
                    self.mask_depth = saved_mask;
                }
                self.discard = Some(*paused);
                Ok(())
            }
            End::ClosingScope { state } => self.step_close_scope(state),
            End::EnsuringDone {
                outcome,
                saved_mask,
                dst,
            }
            | End::OnExitRunFin {
                outcome,
                saved_mask,
                dst,
            }
            | End::OnExitFinish {
                outcome,
                saved_mask,
                dst,
            }
            | End::BracketFinish {
                outcome,
                saved_mask,
                dst,
            } => self.finish_masked_cleanup_ok(outcome, saved_mask, dst),
            other => {
                self.pending = Some(RegVal::Value(Value::Unit));
                self.deliver_end(other)
            }
        }
    }
}
