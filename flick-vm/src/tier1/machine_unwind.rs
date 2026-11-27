//! Frame capture/restore and deterministic LIFO discard/unwinding for tier 1.

use std::mem;

use crate::stdlib::{FuelCounter, primitives};
use crate::tier0::{CauseToken, Error, Reason, Value};

use super::machine::{Frame, LoopState, Machine, RegVal};
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
    DeliverNone {
        dst: usize,
    },
    FinishGenerateStrict {
        dst: usize,
        items: Vec<Value>,
    },
    ResumeRunning {
        saved_failure: Option<CauseToken>,
        propagate_exit: bool,
    },
    FinishCutoff,
    Chain(Box<Discard>),
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

    pub(super) fn extend_captured(dst: &mut CapturedFrames, mut src: CapturedFrames) {
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
        self.heap.drop_values(roots)?;
        let gen_frames = self.release_frame_resources(&mut frame)?;
        if !gen_frames.frames.is_empty() {
            let discard = self.discard.as_mut().expect("active transfer");
            Self::extend_captured(&mut discard.frames, gen_frames);
        }
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
            TransferNext::LoopContinue(state) => {
                self.restore_evidence();
                self.restart_loop(state)?;
            }
            TransferNext::DeliverValue { dst, value } => {
                self.restore_evidence();
                self.store_reg(dst, value)?;
            }
            TransferNext::ReturnNone { dst } | TransferNext::DeliverNone { dst } => {
                self.restore_evidence();
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store_reg(dst, RegVal::Value(none))?;
            }
            TransferNext::FinishGenerateStrict { dst, items } => {
                self.restore_evidence();
                let list = self.heap.construct("$list", items)?;
                self.store_reg(dst, RegVal::Value(list))?;
            }
            TransferNext::ResumeRunning { saved_failure, .. } => {
                self.restore_evidence();
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
                self.heap.drop_values(roots)?;
                let gen_frames = self.release_frame_resources(&mut frame)?;
                self.restore_evidence();
                self.complete_cutoff_end(frame.end)?;
                self.queue_discard_frames(gen_frames);
            }
            TransferNext::Chain(next_discard) => {
                self.discard = Some(*next_discard);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn take_defect_outcome(
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

    pub(super) fn escalate_discard_defect(
        &mut self,
        cause: CauseToken,
        mut roots: Vec<Value>,
    ) -> Result<(), Error> {
        if let Some(mut discard) = self.discard.take() {
            self.drain_transfer_next_roots(&mut discard.next, &mut roots);
            self.restore_frames(discard.frames);
            self.bump_handler_epoch();
            self.restore_evidence();
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
        elect_on_fail: bool,
    ) -> Result<(), Error> {
        let preserved = Self::take_saved_failure(&mut discard.next);
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
        self.failure_elected = !elect_on_fail;
        self.restore_frames(discard.frames);
        self.bump_handler_epoch();
        self.restore_evidence();
        Ok(())
    }

    fn take_saved_failure(next: &mut TransferNext) -> Option<CauseToken> {
        match next {
            TransferNext::ResumeRunning { saved_failure, .. } => saved_failure.take(),
            TransferNext::Chain(chained) => Self::take_saved_failure(&mut chained.next),
            _ => None,
        }
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

    fn drain_transfer_next_roots(&mut self, next: &mut TransferNext, roots: &mut Vec<Value>) {
        let old = mem::replace(next, TransferNext::Raise);
        match old {
            TransferNext::DeliverValue { value, .. } => Self::reg_into_roots(value, roots),
            TransferNext::LoopContinue(state) => {
                for arg in state.args {
                    Self::reg_into_roots(arg, roots);
                }
            }
            TransferNext::FinishGenerateStrict { items, .. } => roots.extend(items),
            TransferNext::ResumeRunning { saved_failure, .. } => {
                if let Some(cause) = saved_failure {
                    roots.push(cause.into_value());
                }
            }
            TransferNext::Chain(chained) => self.drain_discard_roots(*chained, roots),
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
            TransferNext::Raise
            | TransferNext::ReturnNone { .. }
            | TransferNext::DeliverNone { .. }
            | TransferNext::FinishCutoff => {}
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
            ScopeOutcome::ChildClose {
                parent,
                inherited_exit,
            } => {
                if let Some(c) = inherited_exit {
                    roots.push(c.into_value());
                }
                self.drain_closing_scope_roots(*parent, roots);
            }
        }
    }
}
