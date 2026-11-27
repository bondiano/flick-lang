//! Per-`End` discard unwinding and root-draining helpers for tier 1.

use std::mem;

use crate::tier0::{Data, Error, ExitData, Reason, Value};

use super::machine::{End, GlobalState, Machine, RegVal};
use super::machine_generators::GeneratorRun;
use super::machine_handlers::InvocationEnd;
use super::machine_scopes::{BracketOutcome, ScopeOutcome};

impl Machine<'_, '_> {
    pub(super) fn discard_frame_end(&mut self, end: End) -> Result<(), Error> {
        let mut roots = Vec::new();
        match end {
            End::RecoverySelection { id } => self.abandon_recovery(id)?,
            End::RecoveryBody {
                owner, evidence, ..
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
            }
            End::LayerRun { provider, .. } => {
                let paused = self.discard.take().expect("active transfer");
                self.heap.drop_values(roots)?;
                return self
                    .leave_provider_outcome(provider, ScopeOutcome::Discard(Box::new(paused)));
            }
            End::Handle { activation, .. } => {
                let activation = self
                    .activations
                    .remove(&activation)
                    .expect("departed handler");
                self.bump_handler_epoch();
                self.current_owner = activation.owner;
                Self::activation_roots(activation, &mut roots);
            }
            End::Discontinue { cont_id, .. } => {
                if let Some(captured) = self
                    .continuations
                    .get_mut(&cont_id)
                    .and_then(|slot| slot.continuation.take())
                {
                    let discard = self.discard.as_mut().expect("active transfer");
                    Self::extend_captured(&mut discard.frames, captured.frames);
                }
            }
            End::Invocation(id) => self.discard_invocation(id, &mut roots),
            End::Try { selector, .. }
            | End::CatchDefect {
                handler: selector, ..
            }
            | End::CatchCause {
                handler: selector, ..
            } => Self::reg_into_roots(selector, &mut roots),
            End::Global { id, .. } => {
                self.globals[id] = GlobalState::Empty;
            }
            End::Loop(state) => {
                for arg in state.args {
                    Self::reg_into_roots(arg, &mut roots);
                }
            }
            End::GenerateStrict { items, .. } => roots.extend(items),
            End::GeneratorStep { gen_id, .. } => {
                if let Some(slot) = self.generators.get_mut(&gen_id) {
                    slot.run = GeneratorRun::Done;
                }
            }
            End::LazyComplete { lazy, .. } => self.reset_lazy_or_handoff(&lazy)?,
            End::RestoreOwner { previous, .. } => self.current_owner = previous,
            End::CloseScope {
                owner, previous, ..
            } => {
                if self.owner_needs_async_close(owner) {
                    let aborted = self.discard.as_ref().expect("active transfer").aborted;
                    let exit_override = if aborted {
                        Some(
                            self.heap
                                .cause(&[Reason::Fail(&Value::Unit)], &mut self.fuel)?,
                        )
                    } else {
                        None
                    };
                    let paused = self.discard.take().expect("active transfer");
                    return self.begin_close_scope(
                        owner,
                        previous,
                        ScopeOutcome::Discard(Box::new(paused)),
                        exit_override,
                    );
                }
                self.close_owner(owner)?;
                self.current_owner = previous;
            }
            End::ClosingScope { state } => {
                let owner = state.owner;
                let previous = state.previous;
                if state.masked {
                    self.mask_depth = state.saved_mask;
                }
                self.drain_closing_scope_roots(*state, &mut roots);
                self.close_owner(owner)?;
                self.current_owner = previous;
            }
            other => return self.discard_cleanup_and_env_end(other, roots),
        }
        self.heap.drop_values(roots)
    }

    #[allow(clippy::too_many_lines)]
    fn discard_cleanup_and_env_end(
        &mut self,
        end: End,
        mut roots: Vec<Value>,
    ) -> Result<(), Error> {
        match end {
            End::AcquireRegister { release, .. } => Self::reg_into_roots(release, &mut roots),
            End::BracketUse {
                use_fn, release_fn, ..
            } => {
                Self::reg_into_roots(use_fn, &mut roots);
                Self::reg_into_roots(release_fn, &mut roots);
            }
            End::BracketRelease {
                resource,
                release_fn,
                ..
            } => {
                let paused = self.discard.take().expect("active transfer");
                self.heap.drop_values(roots)?;
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                return self.apply_reg(
                    release_fn,
                    RegVal::Value(resource),
                    End::DiscardCleanupDone {
                        saved_mask,
                        masked: true,
                        paused: Box::new(paused),
                    },
                    false,
                );
            }
            End::BracketFinish {
                outcome,
                saved_mask,
                ..
            }
            | End::EnsuringDone {
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
            } => {
                self.mask_depth = saved_mask;
                if let Some(cause) = self.take_defect_outcome(outcome, &mut roots)? {
                    return self.escalate_discard_defect(cause, roots);
                }
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
                self.drain_discard_roots(*paused, &mut roots);
            }
            End::RestoreMask {
                saved_mask, next, ..
            } => {
                self.mask_depth = saved_mask;
                self.heap.drop_values(roots)?;
                return self.discard_frame_end(*next);
            }
            End::EnsuringFinish { finalizer, .. } => {
                let paused = self.discard.take().expect("active transfer");
                self.heap.drop_values(roots)?;
                let saved_mask = self.mask_depth;
                self.mask_depth += 1;
                return self.await_task_reg(
                    finalizer,
                    End::DiscardCleanupDone {
                        saved_mask,
                        masked: true,
                        paused: Box::new(paused),
                    },
                );
            }
            End::OnExitDecide {
                interruptible,
                decide,
                ..
            } => {
                let paused = self.discard.take().expect("active transfer");
                self.heap.drop_values(roots)?;
                let exit_val = if let Some(cause) = Self::peek_discard_exit_cause(&paused.next) {
                    let dup = match self.heap.dup_cause(cause) {
                        Ok(dup) => dup,
                        Err(err) => {
                            self.discard = Some(paused);
                            self.drop_reg(decide)?;
                            return Err(err);
                        }
                    };
                    self.heap
                        .allocate(Data::Exit(ExitData::Failure(dup.into_value())))
                } else {
                    self.heap
                        .allocate(Data::Exit(ExitData::Success(Value::Unit)))
                };
                let saved_mask = self.mask_depth;
                if !interruptible {
                    self.mask_depth += 1;
                }
                return self.apply_reg(
                    decide,
                    RegVal::Value(exit_val),
                    End::OnExitDiscardRunFin {
                        saved_mask,
                        masked: !interruptible,
                        paused: Box::new(paused),
                    },
                    false,
                );
            }
            End::ProvideContext { context, .. } => {
                self.bump_handler_epoch();
                roots.push(context);
            }
            End::WithServices { services, next } => {
                self.bump_handler_epoch();
                roots.extend(services);
                self.heap.drop_values(roots)?;
                return self.discard_frame_end(*next);
            }
            End::CachedComplete { next, .. } => {
                self.heap.drop_values(roots)?;
                return self.discard_frame_end(*next);
            }
            End::RestoreFiberRef { key, previous, .. } => {
                let old = match previous {
                    Some(v) => self.fiber_refs.insert(key, v),
                    None => self.fiber_refs.remove(&key),
                };
                roots.extend(old);
            }
            End::RestoreOwnerEvidence {
                previous, evidence, ..
            } => {
                self.current_owner = previous;
                self.current_evidence = evidence;
            }
            End::RestoreEnvRefs { previous, .. } => {
                let old = mem::replace(&mut self.fiber_refs, previous);
                roots.extend(old.into_values());
            }
            End::RestoreRandom {
                previous, rng_id, ..
            } => {
                self.seeded_random = previous;
                self.isaac_rngs.remove(&rng_id);
            }
            End::LayerBuildFinish {
                inner_owner,
                prev_owner,
                prev_ev,
                prev_memo,
                memo_entry,
                target,
                ..
            } => {
                self.heap.drop_values(roots)?;
                return self.discard_layer_build_finish(
                    inner_owner,
                    prev_owner,
                    prev_ev,
                    prev_memo,
                    memo_entry,
                    target,
                );
            }
            End::Top
            | End::Store(_)
            | End::Modify { .. }
            | End::FunctionBoundary { .. }
            | End::RecoveryHelper { .. }
            | End::CaptureExit { .. }
            | End::OrDie { .. }
            | End::TimedFinish { .. }
            | End::DiscardFinalizer => {}
            _ => unreachable!("handled in discard_frame_end"),
        }
        self.heap.drop_values(roots)
    }

    fn discard_invocation(&mut self, id: usize, roots: &mut Vec<Value>) {
        let inv = self.invocations.remove(&id).expect("departed invocation");
        if let Some(callee) = inv.callee {
            Self::reg_into_roots(callee, roots);
        }
        for arg in inv.arguments {
            Self::reg_into_roots(arg, roots);
        }
        match inv.end {
            InvocationEnd::Tail(seg_id) => {
                let segment = self
                    .segments
                    .remove(&seg_id)
                    .expect("departed tail segment");
                let discard = self.discard.as_mut().expect("active transfer");
                Self::extend_captured(&mut discard.frames, segment.frames);
            }
            InvocationEnd::Control { cont_id, .. } => {
                if let Some(captured) = self
                    .continuations
                    .get_mut(&cont_id)
                    .and_then(|slot| slot.continuation.take())
                {
                    let discard = self.discard.as_mut().expect("active transfer");
                    Self::extend_captured(&mut discard.frames, captured.frames);
                }
            }
            InvocationEnd::Caller(_) | InvocationEnd::Abort(_) => {}
        }
    }

    pub(super) fn drain_end_roots(&mut self, end: End, roots: &mut Vec<Value>) {
        match end {
            End::Loop(state) => {
                for arg in state.args {
                    Self::reg_into_roots(arg, roots);
                }
            }
            End::GenerateStrict { items, .. } => roots.extend(items),
            End::Try { selector, .. }
            | End::CatchDefect {
                handler: selector, ..
            }
            | End::CatchCause {
                handler: selector, ..
            } => Self::reg_into_roots(selector, roots),
            End::ClosingScope { state } => self.drain_closing_scope_roots(*state, roots),
            End::AcquireRegister { release, .. } => Self::reg_into_roots(release, roots),
            End::BracketUse {
                use_fn, release_fn, ..
            } => {
                Self::reg_into_roots(use_fn, roots);
                Self::reg_into_roots(release_fn, roots);
            }
            End::BracketRelease {
                resource,
                release_fn,
                ..
            } => {
                roots.push(resource);
                Self::reg_into_roots(release_fn, roots);
            }
            End::BracketFinish { outcome, .. }
            | End::EnsuringDone { outcome, .. }
            | End::OnExitRunFin { outcome, .. }
            | End::OnExitFinish { outcome, .. } => match outcome {
                BracketOutcome::Success(v) => Self::reg_into_roots(v, roots),
                BracketOutcome::Failure(cause) => roots.push(cause.into_value()),
            },
            End::RestoreMask { next, .. } | End::CachedComplete { next, .. } => {
                self.drain_end_roots(*next, roots);
            }
            End::DiscardCleanupDone { paused, .. } | End::OnExitDiscardRunFin { paused, .. } => {
                self.drain_discard_roots(*paused, roots);
            }
            End::EnsuringFinish { finalizer, .. } => Self::reg_into_roots(finalizer, roots),
            End::OnExitDecide { decide, .. } => Self::reg_into_roots(decide, roots),
            End::ProvideContext { context, .. } => roots.push(context),
            End::WithServices { services, next } => {
                roots.extend(services);
                self.drain_end_roots(*next, roots);
            }
            End::RestoreFiberRef { previous, .. } => roots.extend(previous),
            End::RestoreEnvRefs { previous, .. } => roots.extend(previous.into_values()),
            End::LayerBuildFinish { prev_memo, .. } => roots.extend(prev_memo),
            End::Top
            | End::Store(_)
            | End::Global { .. }
            | End::Modify { .. }
            | End::FunctionBoundary { .. }
            | End::GeneratorStep { .. }
            | End::LazyComplete { .. }
            | End::CaptureExit { .. }
            | End::RecoverySelection { .. }
            | End::RecoveryHelper { .. }
            | End::RecoveryBody { .. }
            | End::Handle { .. }
            | End::Discontinue { .. }
            | End::Invocation(_)
            | End::LayerRun { .. }
            | End::CloseScope { .. }
            | End::RestoreOwner { .. }
            | End::OrDie { .. }
            | End::TimedFinish { .. }
            | End::RestoreOwnerEvidence { .. }
            | End::RestoreRandom { .. }
            | End::DiscardFinalizer => {}
        }
    }
}
