// Stage 4A destination normal-return and unwind handlers.

impl<'p> Machine<'p, '_> {
    fn finish_masked_cleanup_ok(
        &mut self,
        outcome: BracketOutcome,
        saved_mask: usize,
        binding: Local,
    ) -> Result<(), Error> {
        self.mask_depth = saved_mask;
        match outcome {
            BracketOutcome::Success(val) => match self.check_active_interrupt() {
                Ok(true) => {
                    self.heap.drop_value(val)?;
                    Ok(())
                }
                Ok(false) => self.store(binding, val),
                Err(err) => {
                    self.heap.drop_value(val)?;
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

    fn finish_masked_cleanup_err(
        &mut self,
        outcome: BracketOutcome,
        saved_mask: usize,
    ) -> Result<(), Error> {
        self.mask_depth = saved_mask;
        match outcome {
            BracketOutcome::Success(val) => {
                self.heap.drop_value(val)?;
                self.failure_elected = false;
                Ok(())
            }
            BracketOutcome::Failure(orig) => {
                let joined = if let Some(secondary) = self.failure.take() {
                    let res = primitives::join_causes(self.heap, &orig, &secondary, &mut FuelCounter(&mut self.fuel));
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

    fn returned_async_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::RestoreMask {
                saved_mask,
                check_on_exit,
                next,
            } => {
                self.mask_depth = saved_mask;
                if check_on_exit && self.check_active_interrupt()? {
                    if let Some(v) = self.pending.take() {
                        self.heap.drop_value(v)?;
                    }
                    self.fail_destination(*next)
                } else {
                    self.returned_frame(*next)
                }
            }
            Destination::DiscardCleanupDone {
                saved_mask,
                masked,
                paused,
            } => {
                let ignored = self.pending.take().expect("discard cleanup return");
                self.heap.drop_value(ignored)?;
                if masked {
                    self.mask_depth = saved_mask;
                }
                self.discard = Some(*paused);
                Ok(())
            }
            Destination::OnExitDiscardRunFin {
                saved_mask,
                masked,
                paused,
            } => {
                let opt = self.pending.take().expect("discard decide option");
                if let Some(task) = self.extract_opt_value(opt)? {
                    self.scratch.extend([task, Value::Unit]);
                    self.await_task_fresh(Destination::DiscardCleanupDone {
                        saved_mask,
                        masked,
                        paused,
                    })
                } else {
                    if masked {
                        self.mask_depth = saved_mask;
                    }
                    self.discard = Some(*paused);
                    Ok(())
                }
            }
            Destination::EnsuringFinish { finalizer, binding } => {
                let val = self.pending.take().expect("ensuring result");
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                self.scratch.extend([finalizer, Value::Unit]);
                self.await_task_fresh(Destination::EnsuringDone {
                    outcome: BracketOutcome::Success(val),
                    saved_mask,
                    binding,
                })
            }
            Destination::EnsuringDone {
                outcome,
                saved_mask,
                binding,
            }
            | Destination::OnExitFinish {
                outcome,
                saved_mask,
                binding,
            } => {
                let ignored = self.pending.take().expect("cleanup return");
                self.heap.drop_value(ignored)?;
                self.finish_masked_cleanup_ok(outcome, saved_mask, binding)
            }
            other => self.returned_effect_destination(other),
        }
    }

    fn returned_effect_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::OnExitDecide {
                interruptible,
                decide,
                binding,
            } => {
                let val = self.pending.take().expect("on_exit result");
                let saved_mask = self.mask_depth;
                if !interruptible {
                    self.mask_depth += 1;
                }
                let dup = self.heap.dup(&val)?;
                let exit_val = self.heap.allocate(Data::Exit(ExitData::Success(dup)));
                self.scratch.extend([decide, exit_val]);
                self.apply_fresh(Destination::OnExitRunFin {
                    outcome: BracketOutcome::Success(val),
                    saved_mask,
                    binding,
                })
            }
            Destination::OnExitRunFin {
                outcome,
                saved_mask,
                binding,
            } => {
                let opt = self.pending.take().expect("decide option");
                if let Some(task) = self.extract_opt_value(opt)? {
                    self.scratch.extend([task, Value::Unit]);
                    self.await_task_fresh(Destination::OnExitFinish {
                        outcome,
                        saved_mask,
                        binding,
                    })
                } else {
                    self.finish_masked_cleanup_ok(outcome, saved_mask, binding)
                }
            }
            Destination::CatchCause { handler, binding } => {
                self.heap.drop_value(handler)?;
                let val = self.pending.take().expect("catch_cause ok");
                self.store(binding, val)
            }
            Destination::OrDie { binding } => {
                let val = self.pending.take().expect("or_die ok");
                self.store(binding, val)
            }
            Destination::TimedFinish { start_ns, binding } => {
                let val = self.pending.take().expect("timed ok");
                let elapsed = self.virtual_now_ns.saturating_sub(start_ns);
                let dur = self
                    .heap
                    .allocate(Data::Duration(super::DurationData::Nanos(
                        num_bigint::BigInt::from(elapsed),
                    )));
                let pair = self.heap.construct("$tuple", vec![dur, val])?;
                self.store(binding, pair)
            }
            Destination::CachedComplete { cell_id, next } => {
                self.complete_cached_cell(cell_id, *next)
            }
            other => self.returned_context_destination(other),
        }
    }

    fn complete_cached_cell(&mut self, cell_id: usize, next: Destination<'p>) -> Result<(), Error> {
        let val = self.pending.take().expect("cached ok");
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
        self.pending = Some(val);
        self.returned_frame(next)
    }

    fn returned_context_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::ProvideContext { context, binding } => {
                self.heap.drop_value(context)?;
                let val = self.pending.take().expect("provide context ok");
                self.store(binding, val)
            }
            Destination::WithServices { services, next } => {
                self.heap.drop_values(services)?;
                self.returned_frame(*next)
            }
            Destination::RestoreFiberRef {
                key,
                previous,
                binding,
            } => {
                let old = match previous {
                    Some(v) => self.fiber_refs.insert(key, v),
                    None => self.fiber_refs.remove(&key),
                };
                if let Some(v) = old {
                    self.heap.drop_value(v)?;
                }
                let val = self.pending.take().expect("with_ref ok");
                self.store(binding, val)
            }
            Destination::RestoreOwnerEvidence {
                previous,
                evidence,
                binding,
            } => {
                self.current_owner = previous;
                self.current_evidence = evidence;
                let val = self.pending.take().expect("owner_run ok");
                self.store(binding, val)
            }
            Destination::RestoreEnvRefs { previous, binding } => {
                let old = mem::replace(&mut self.fiber_refs, previous);
                self.heap.drop_values(old.into_values().collect())?;
                let val = self.pending.take().expect("env_run ok");
                self.store(binding, val)
            }
            Destination::RestoreRandom {
                previous,
                rng_id,
                binding,
            } => {
                self.seeded_random = previous;
                self.isaac_rngs.remove(&rng_id);
                let val = self.pending.take().expect("with_seed ok");
                self.store(binding, val)
            }
            Destination::LayerBuildFinish {
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
            _ => unreachable!("non-async destination"),
        }
    }
}
