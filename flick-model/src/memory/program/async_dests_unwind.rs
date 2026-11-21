// Stage 4A destination unwind and cutoff-cleanup handlers.

impl<'p> Machine<'p, '_> {
    fn unwind_async_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::RestoreMask {
                saved_mask, next, ..
            } => {
                self.mask_depth = saved_mask;
                self.fail_destination(*next)
            }
            Destination::DiscardCleanupDone {
                saved_mask,
                masked,
                paused,
            }
            | Destination::OnExitDiscardRunFin {
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
            Destination::EnsuringFinish { finalizer, binding } => {
                let cause = self.failure.take().expect("ensuring failure");
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.scratch.extend([finalizer, Value::Unit]);
                self.await_task_fresh(Destination::EnsuringDone {
                    outcome: BracketOutcome::Failure(cause),
                    saved_mask,
                    binding,
                })
            }
            Destination::EnsuringDone {
                outcome,
                saved_mask,
                ..
            }
            | Destination::OnExitRunFin {
                outcome,
                saved_mask,
                ..
            }
            | Destination::OnExitFinish {
                outcome,
                saved_mask,
                ..
            } => self.finish_masked_cleanup_err(outcome, saved_mask),
            Destination::OnExitDecide {
                interruptible,
                decide,
                binding,
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
                self.scratch.extend([decide, exit_val]);
                self.apply_fresh(Destination::OnExitRunFin {
                    outcome: BracketOutcome::Failure(cause),
                    saved_mask,
                    binding,
                })
            }
            other => self.unwind_context_destination(other),
        }
    }

    fn unwind_context_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::CatchCause { handler, binding } => {
                if self.skip_interrupted_handler()? {
                    return self.heap.drop_value(handler);
                }
                let cause = self.failure.take().expect("catch_cause failure");
                self.failure_elected = false;
                self.scratch.extend([handler, cause.into_value()]);
                self.apply_fresh(Destination::Bind(binding))
            }
            Destination::OrDie { .. } => {
                let cause = self.failure.take().expect("or_die failure");
                let converted = primitives::fails_to_dies_cause(self.heap, &cause, &mut self.fuel);
                self.heap.drop_cause(cause)?;
                self.failure = Some(converted?);
                self.failure_elected = true;
                Ok(())
            }
            Destination::TimedFinish { .. } => Ok(()),
            Destination::CachedComplete { next, .. } => self.fail_destination(*next),
            Destination::ProvideContext { context, .. } => self.heap.drop_value(context),
            Destination::WithServices { services, next } => {
                self.heap.drop_values(services)?;
                self.fail_destination(*next)
            }
            Destination::RestoreFiberRef { key, previous, .. } => {
                let old = match previous {
                    Some(v) => self.fiber_refs.insert(key, v),
                    None => self.fiber_refs.remove(&key),
                };
                if let Some(v) = old {
                    self.heap.drop_value(v)?;
                }
                Ok(())
            }
            Destination::RestoreOwnerEvidence {
                previous, evidence, ..
            } => {
                self.current_owner = previous;
                self.current_evidence = evidence;
                Ok(())
            }
            Destination::RestoreEnvRefs { previous, .. } => {
                let old = mem::replace(&mut self.fiber_refs, previous);
                self.heap.drop_values(old.into_values().collect())
            }
            Destination::RestoreRandom {
                previous, rng_id, ..
            } => {
                self.seeded_random = previous;
                self.isaac_rngs.remove(&rng_id);
                Ok(())
            }
            Destination::LayerBuildFinish {
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
            _ => unreachable!("non-async destination"),
        }
    }

    pub(super) fn is_cutoff_cleanup_destination(
        &self,
        dest: &Destination<'p>,
    ) -> Result<bool, Error> {
        let has_int = |c: &CauseToken| -> Result<bool, Error> {
            Ok(self
                .heap
                .cause_reasons(c)?
                .any(|r| matches!(r, Reason::Interrupt { .. })))
        };
        match dest {
            Destination::DiscardCleanupDone { .. } | Destination::OnExitDiscardRunFin { .. } => {
                Ok(true)
            }
            Destination::ClosingScope { state } => match &state.outcome {
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
            Destination::EnsuringDone {
                outcome: BracketOutcome::Failure(c),
                ..
            }
            | Destination::OnExitRunFin {
                outcome: BracketOutcome::Failure(c),
                ..
            }
            | Destination::OnExitFinish {
                outcome: BracketOutcome::Failure(c),
                ..
            }
            | Destination::BracketFinish {
                outcome: BracketOutcome::Failure(c),
                ..
            } => has_int(c),
            _ => Ok(false),
        }
    }

    pub(super) fn complete_cutoff_destination(
        &mut self,
        dest: Destination<'p>,
    ) -> Result<(), Error> {
        match dest {
            Destination::DiscardCleanupDone {
                saved_mask,
                masked,
                paused,
            }
            | Destination::OnExitDiscardRunFin {
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
            Destination::ClosingScope { state } => self.step_close_scope(state),
            Destination::EnsuringDone {
                outcome,
                saved_mask,
                binding,
            }
            | Destination::OnExitRunFin {
                outcome,
                saved_mask,
                binding,
            }
            | Destination::OnExitFinish {
                outcome,
                saved_mask,
                binding,
            }
            | Destination::BracketFinish {
                outcome,
                saved_mask,
                binding,
            } => self.finish_masked_cleanup_ok(outcome, saved_mask, binding),
            other => {
                self.pending = Some(Value::Unit);
                self.returned_frame(other)
            }
        }
    }
}
