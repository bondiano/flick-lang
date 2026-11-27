//! Frame capture/restore and deterministic LIFO discard/unwinding for tier 1.

use std::mem;

use crate::stdlib::{FuelCounter, primitives};
use crate::tier0::{CauseToken, Error, Reason, Value};

use super::machine::{End, Frame, GlobalState, LoopState, Machine, RegVal};
use super::machine_handlers::InvocationEnd;
use super::machine_scopes::{BracketOutcome, ClosingScopeState, ScopeOutcome};

/// Contiguous slice of call frames and their normalized register window (`base` relative to 0).
#[derive(Default)]
pub(super) struct CapturedFrames {
    pub(super) frames: Vec<Frame>,
    pub(super) registers: Vec<Option<RegVal>>,
}

pub(super) enum TransferNext {
    Invoke(usize),
    Raise,
    LoopContinue(LoopState),
    DeliverValue {
        dst: usize,
        value: RegVal,
    },
    ReturnNone {
        dst: usize,
    },
    ResumeRunning {
        saved_failure: Option<CauseToken>,
        propagate_exit: bool,
    },
    FinishCutoff,
}

pub(super) struct Discard {
    pub(super) frames: CapturedFrames,
    pub(super) owner: Option<usize>,
    pub(super) next: TransferNext,
    pub(super) aborted: bool,
}

impl Machine<'_, '_> {
    pub(super) fn split_frames(&mut self, at: usize) -> CapturedFrames {
        let mut frames = self.frames.split_off(at);
        if frames.is_empty() {
            return CapturedFrames::default();
        }
        let reg_base = frames[0].base;
        let registers = self.registers.split_off(reg_base);
        for frame in &mut frames {
            frame.base -= reg_base;
        }
        CapturedFrames { frames, registers }
    }

    pub(super) fn restore_frames(&mut self, mut captured: CapturedFrames) {
        let offset = self.registers.len();
        for frame in &mut captured.frames {
            frame.base += offset;
        }
        self.registers.extend(captured.registers);
        self.frames.extend(captured.frames);
    }

    fn extend_captured(dst: &mut CapturedFrames, mut src: CapturedFrames) {
        let offset = dst.registers.len();
        for frame in &mut src.frames {
            frame.base += offset;
        }
        dst.registers.extend(src.registers);
        dst.frames.extend(src.frames);
    }

    pub(super) fn discard_step(&mut self) -> Result<(), Error> {
        let popped = {
            let discard = self.discard.as_mut().expect("active transfer");
            discard.frames.frames.pop().map(|frame| {
                let regs = discard.frames.registers.split_off(frame.base);
                (frame, regs)
            })
        };
        let Some((mut frame, regs)) = popped else {
            return self.finish_discard();
        };
        let mut roots = Vec::new();
        for reg in regs.into_iter().flatten() {
            Self::reg_into_roots(reg, &mut roots);
        }
        self.detach_frame_vars(mem::take(&mut frame.vars), &mut roots);
        self.heap.drop_values(roots)?;
        self.discard_frame_end(frame.end)
    }

    fn finish_discard(&mut self) -> Result<(), Error> {
        let discard = self.discard.take().expect("completed transfer");
        if let Some(owner) = discard.owner {
            self.current_owner = owner;
        }
        match discard.next {
            TransferNext::Invoke(id) => {
                self.ready = Some(id);
            }
            TransferNext::Raise => {}
            TransferNext::LoopContinue(state) => self.restart_loop(state)?,
            TransferNext::DeliverValue { dst, value } => self.store_reg(dst, value)?,
            TransferNext::ReturnNone { dst } => {
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store_reg(dst, RegVal::Value(none))?;
            }
            TransferNext::ResumeRunning { saved_failure, .. } => {
                if let Some(cause) = saved_failure {
                    if let Some(existing) = self.failure.take() {
                        self.heap.drop_cause(existing)?;
                    }
                    self.failure = Some(cause);
                    self.failure_elected = false;
                }
            }
            TransferNext::FinishCutoff => {
                let mut frame = self.frames.pop().expect("cutoff cleanup frame");
                let mut roots = Vec::new();
                for reg in self.registers.drain(frame.base..).flatten() {
                    Self::reg_into_roots(reg, &mut roots);
                }
                self.detach_frame_vars(mem::take(&mut frame.vars), &mut roots);
                self.heap.drop_values(roots)?;
                self.complete_cutoff_end(frame.end)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn discard_frame_end(&mut self, end: End) -> Result<(), Error> {
        let mut roots = Vec::new();
        match end {
            End::RecoverySelection { id } => self.abandon_recovery(id)?,
            End::RecoveryBody { owner, .. } => {
                self.current_owner = owner;
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
            End::Try { selector, .. } => Self::reg_into_roots(selector, &mut roots),
            End::Global { id, .. } => {
                self.globals[id] = GlobalState::Empty;
            }
            End::Loop(state) => {
                for arg in state.args {
                    Self::reg_into_roots(arg, &mut roots);
                }
            }
            End::CloseScope {
                owner, previous, ..
            } => {
                if self.owner_needs_close_work(owner) {
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
            } => {
                if masked {
                    self.mask_depth = saved_mask;
                }
                self.drain_discard_roots(*paused, &mut roots);
            }
            End::RestoreMask { saved_mask, next } => {
                self.mask_depth = saved_mask;
                return self.discard_frame_end(*next);
            }
            End::Top
            | End::Store(_)
            | End::FunctionBoundary { .. }
            | End::RecoveryHelper { .. }
            | End::DiscardFinalizer => {}
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
            InvocationEnd::Abort(_) => {}
        }
    }

    fn take_defect_outcome(
        &self,
        outcome: BracketOutcome,
        roots: &mut Vec<Value>,
    ) -> Result<Option<CauseToken>, Error> {
        match outcome {
            BracketOutcome::Success(v) => {
                Self::reg_into_roots(v, roots);
                Ok(None)
            }
            BracketOutcome::Failure(cause) => {
                let has_die = self
                    .heap
                    .cause_reasons(&cause)?
                    .any(|r| matches!(r, Reason::Die(_)));
                if has_die {
                    Ok(Some(cause))
                } else {
                    roots.push(cause.into_value());
                    Ok(None)
                }
            }
        }
    }

    fn escalate_discard_defect(
        &mut self,
        cause: CauseToken,
        mut roots: Vec<Value>,
    ) -> Result<(), Error> {
        if let Some(mut discard) = self.discard.take() {
            self.drain_transfer_next_roots(&mut discard.next, &mut roots);
            self.restore_frames(discard.frames);
            self.bump_handler_epoch();
        }
        if let Some(prev) = self.failure.replace(cause) {
            roots.push(prev.into_value());
        }
        self.failure_elected = true;
        self.heap.drop_values(roots)
    }

    pub(super) fn escalate_paused_discard(
        &mut self,
        mut discard: Discard,
        fin_cause: CauseToken,
    ) -> Result<(), Error> {
        let preserved = match &mut discard.next {
            TransferNext::ResumeRunning { saved_failure, .. } => saved_failure.take(),
            _ => None,
        };
        let mut roots = Vec::new();
        self.drain_transfer_next_roots(&mut discard.next, &mut roots);
        self.heap.drop_values(roots)?;
        let combined = if let Some(prev) = preserved {
            let res = primitives::join_causes(
                self.heap,
                &prev,
                &fin_cause,
                &mut FuelCounter(&mut self.fuel),
            );
            self.heap.drop_cause(prev)?;
            self.heap.drop_cause(fin_cause)?;
            match res {
                Ok(c) => c,
                Err(err) => {
                    let mut droots = Vec::new();
                    self.drain_discard_roots(discard, &mut droots);
                    self.heap.drop_values(droots)?;
                    return Err(err);
                }
            }
        } else {
            fin_cause
        };
        self.failure = Some(combined);
        self.failure_elected = true;
        self.restore_frames(discard.frames);
        self.bump_handler_epoch();
        Ok(())
    }

    pub(super) fn peek_discard_exit_cause(next: &TransferNext) -> Option<&CauseToken> {
        match next {
            TransferNext::ResumeRunning {
                saved_failure,
                propagate_exit: true,
            } => saved_failure.as_ref(),
            _ => None,
        }
    }

    pub(super) fn is_cutoff_cleanup_end(end: &End) -> bool {
        match end {
            End::DiscardCleanupDone { .. } => true,
            End::ClosingScope { state } => matches!(state.outcome, ScopeOutcome::Discard(_)),
            _ => false,
        }
    }

    fn complete_cutoff_end(&mut self, end: End) -> Result<(), Error> {
        match end {
            End::DiscardCleanupDone {
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
            End::BracketFinish {
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

    fn drain_transfer_next_roots(&mut self, next: &mut TransferNext, roots: &mut Vec<Value>) {
        let old = mem::replace(next, TransferNext::Raise);
        match old {
            TransferNext::DeliverValue { value, .. } => Self::reg_into_roots(value, roots),
            TransferNext::LoopContinue(state) => {
                for arg in state.args {
                    Self::reg_into_roots(arg, roots);
                }
            }
            TransferNext::ResumeRunning { saved_failure, .. } => {
                if let Some(cause) = saved_failure {
                    roots.push(cause.into_value());
                }
            }
            TransferNext::Invoke(id) => {
                if let Some(inv) = self.invocations.remove(&id) {
                    if let Some(callee) = inv.callee {
                        Self::reg_into_roots(callee, roots);
                    }
                    for arg in inv.arguments {
                        Self::reg_into_roots(arg, roots);
                    }
                }
            }
            TransferNext::Raise | TransferNext::ReturnNone { .. } | TransferNext::FinishCutoff => {}
        }
    }

    pub(super) fn drain_discard_roots(&mut self, mut discard: Discard, roots: &mut Vec<Value>) {
        self.drain_captured_roots(discard.frames, roots);
        self.drain_transfer_next_roots(&mut discard.next, roots);
    }

    pub(super) fn drain_closing_scope_roots(
        &mut self,
        state: ClosingScopeState,
        roots: &mut Vec<Value>,
    ) {
        for fin in state.remaining {
            Self::finalizer_roots(fin, roots);
        }
        if let Some(cause) = state.exit_override {
            roots.push(cause.into_value());
        }
        if let Some(cause) = state.finalizer_failure {
            roots.push(cause.into_value());
        }
        match state.outcome {
            ScopeOutcome::Normal { value, .. } => Self::reg_into_roots(value, roots),
            ScopeOutcome::Failure(cause) => roots.push(cause.into_value()),
            ScopeOutcome::Discard(discard) => self.drain_discard_roots(*discard, roots),
        }
    }

    pub(super) fn drain_end_roots(&mut self, end: End, roots: &mut Vec<Value>) {
        match end {
            End::Loop(state) => {
                for arg in state.args {
                    Self::reg_into_roots(arg, roots);
                }
            }
            End::Try { selector, .. } => Self::reg_into_roots(selector, roots),
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
            End::BracketFinish { outcome, .. } => match outcome {
                BracketOutcome::Success(v) => Self::reg_into_roots(v, roots),
                BracketOutcome::Failure(cause) => roots.push(cause.into_value()),
            },
            End::RestoreMask { next, .. } => self.drain_end_roots(*next, roots),
            End::DiscardCleanupDone { paused, .. } => self.drain_discard_roots(*paused, roots),
            End::Top
            | End::Store(_)
            | End::Global { .. }
            | End::FunctionBoundary { .. }
            | End::RecoverySelection { .. }
            | End::RecoveryHelper { .. }
            | End::RecoveryBody { .. }
            | End::Handle { .. }
            | End::Discontinue { .. }
            | End::Invocation(_)
            | End::CloseScope { .. }
            | End::DiscardFinalizer => {}
        }
    }
}
