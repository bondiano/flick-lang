//! Handler invocation stepping, one-shot continuation resume, discontinuation,
//! and handler/continuation root cleanup for tier 1.

use std::mem;

use crate::tier0::{Error, Value};

use super::machine::{End, Machine, RegVal};
use super::machine_handlers::InvocationEnd;
use super::machine_unwind::{Discard, TransferNext};

impl Machine<'_, '_> {
    pub(super) fn invoke_step(&mut self, id: usize) -> Result<(), Error> {
        let is_final_control = {
            let inv = self.invocations.get(&id).expect("pending invocation");
            inv.arguments.len() == 1 && matches!(inv.end, InvocationEnd::Control { .. })
        };
        if is_final_control {
            let mut inv = self.invocations.remove(&id).expect("final invocation step");
            let callee = inv.callee.take().expect("invocation function");
            let arg = inv.arguments.pop_front().expect("continuation arg");
            let InvocationEnd::Control { cont_id, dst } = inv.end else {
                unreachable!("control invocation matched above");
            };
            return self.apply_reg(callee, arg, End::Discontinue { cont_id, dst }, false);
        }
        let next_arg = self
            .invocations
            .get_mut(&id)
            .expect("pending invocation")
            .arguments
            .pop_front();
        if let Some(arg) = next_arg {
            let callee = self
                .invocations
                .get_mut(&id)
                .expect("pending invocation")
                .callee
                .take()
                .expect("invocation function");
            return self.apply_reg(callee, arg, End::Invocation(id), false);
        }
        let mut inv = self.invocations.remove(&id).expect("completed invocation");
        let value = inv.callee.take().expect("invocation result");
        match inv.end {
            InvocationEnd::Tail(segment_id) => {
                let count = self.segments[&segment_id].frames.frames.len();
                if let Err(err) = self.transfer_fuel(count) {
                    self.drop_reg(value)?;
                    return Err(err);
                }
                let segment = self.segments.remove(&segment_id).expect("parked tail body");
                self.current_owner = segment.owner;
                let restore_epoch = self.frames.len() == segment.frame_base
                    && self.handler_epoch == segment.arm_epoch;
                self.restore_frames(segment.frames);
                if restore_epoch {
                    self.handler_epoch = segment.saved_epoch;
                } else {
                    self.bump_handler_epoch();
                }
                self.store_reg(segment.dst, value)
            }
            InvocationEnd::Abort(dst) => self.store_reg(dst, value),
            InvocationEnd::Control { cont_id, dst } => {
                if let Some(captured) = self
                    .continuations
                    .get_mut(&cont_id)
                    .and_then(|slot| slot.continuation.take())
                {
                    self.discard = Some(Discard {
                        frames: captured.frames,
                        owner: Some(self.current_owner),
                        next: TransferNext::DeliverValue { dst, value },
                        aborted: true,
                    });
                    Ok(())
                } else {
                    self.store_reg(dst, value)
                }
            }
        }
    }

    pub(super) fn apply_continuation(
        &mut self,
        callee: RegVal,
        argument: RegVal,
        execution: usize,
        id: usize,
        end: End,
        tail: bool,
    ) -> Result<(), Error> {
        if execution != self.execution {
            let mut roots = Vec::new();
            Self::reg_into_roots(callee, &mut roots);
            Self::reg_into_roots(argument, &mut roots);
            self.drain_end_roots(end, &mut roots);
            self.heap.drop_values(roots)?;
            return Err(Error::ForeignContinuation);
        }
        let End::Store(dst) = end else {
            let mut roots = Vec::new();
            Self::reg_into_roots(callee, &mut roots);
            Self::reg_into_roots(argument, &mut roots);
            self.drain_end_roots(end, &mut roots);
            self.heap.drop_values(roots)?;
            return Err(Error::UnsupportedOperation(
                "non-call continuation application",
            ));
        };
        self.pending = Some(argument);
        self.drop_reg(callee)?;
        let Some(mut captured) = self
            .continuations
            .get_mut(&id)
            .and_then(|slot| slot.continuation.take())
        else {
            if let Some(arg) = self.pending.take() {
                self.drop_reg(arg)?;
            }
            self.last_defect_span = self
                .continuations
                .get(&id)
                .and_then(|slot| slot.perform_span);
            return Err(Error::ResumeTwice);
        };
        if let Err(err) = self.transfer_fuel(captured.frames.frames.len()) {
            if let Some(slot) = self.continuations.get_mut(&id) {
                slot.continuation = Some(captured);
            }
            return Err(err);
        }
        let target_dst = if tail
            && let Some(top) = self.frames.last()
            && top.vars.is_empty()
            && top.recovery_id.is_none()
            && self.registers[top.base..].iter().all(Option::is_none)
            && (matches!(top.end, End::Store(_))
                || matches!(top.end, End::Discontinue { cont_id, .. } if cont_id == id))
        {
            let popped = self.frames.pop().expect("tail-resume frame");
            self.registers.truncate(popped.base);
            match popped.end {
                End::Store(outer) | End::Discontinue { dst: outer, .. } => outer,
                _ => unreachable!("matched tail-resume end"),
            }
        } else {
            dst
        };
        if let Some(bottom) = captured.frames.frames.first_mut()
            && let End::Handle {
                dst: handle_dst, ..
            } = &mut bottom.end
        {
            *handle_dst = target_dst;
        }
        self.current_owner = captured.owner;
        self.restore_frames(captured.frames);
        self.bump_handler_epoch();
        let arg = self.pending.take().expect("resumed argument");
        self.store_reg(captured.dst, arg)
    }

    pub(super) fn handler_returned(&mut self, id: usize, dst: usize) -> Result<(), Error> {
        let mut activation = self.activations.remove(&id).expect("completed handler");
        self.bump_handler_epoch();
        self.current_owner = activation.owner;
        self.pending_handler = activation.ret.take();
        let mut roots = Vec::new();
        Self::activation_roots(activation, &mut roots);
        self.heap.drop_values(roots)?;
        let value = self.pending.take().expect("handler body value");
        if let Some(ret) = self.pending_handler.take() {
            self.apply_reg(ret, value, End::Store(dst), false)
        } else {
            self.store_reg(dst, value)
        }
    }

    pub(super) fn handler_failed(&mut self, id: usize, dst: usize) -> Result<(), Error> {
        let mut activation = self.activations.remove(&id).expect("failed handler");
        self.bump_handler_epoch();
        self.current_owner = activation.owner;
        self.pending_handler = activation.errors.take();
        let mut roots = Vec::new();
        Self::activation_roots(activation, &mut roots);
        self.heap.drop_values(roots)?;
        if self.pending_handler.is_some() {
            self.begin_recovery(dst)?;
        }
        Ok(())
    }

    pub(super) fn discontinue_returned(&mut self, cont_id: usize, dst: usize) -> Result<(), Error> {
        let value = self.pending.take().expect("control arm result");
        if let Some(captured) = self
            .continuations
            .get_mut(&cont_id)
            .and_then(|slot| slot.continuation.take())
        {
            self.discard = Some(Discard {
                frames: captured.frames,
                owner: Some(self.current_owner),
                next: TransferNext::DeliverValue { dst, value },
                aborted: true,
            });
            Ok(())
        } else {
            self.store_reg(dst, value)
        }
    }

    pub(super) fn unwind_discontinue(&mut self, cont_id: usize) {
        if let Some(captured) = self
            .continuations
            .get_mut(&cont_id)
            .and_then(|slot| slot.continuation.take())
        {
            let cause = self.failure.take();
            self.failure_elected = false;
            self.discard = Some(Discard {
                frames: captured.frames,
                owner: Some(self.current_owner),
                next: TransferNext::ResumeRunning {
                    saved_failure: cause,
                    propagate_exit: true,
                },
                aborted: true,
            });
        }
    }

    pub(super) fn invocation_failed(&mut self, id: usize) -> Result<(), Error> {
        let inv = self.invocations.remove(&id).expect("failed invocation");
        let mut roots = Vec::new();
        for arg in inv.arguments {
            Self::reg_into_roots(arg, &mut roots);
        }
        if let Some(callee) = inv.callee {
            Self::reg_into_roots(callee, &mut roots);
        }
        self.heap.drop_values(roots)?;
        match inv.end {
            InvocationEnd::Tail(seg_id) => {
                let segment = self.segments.remove(&seg_id).expect("failed tail segment");
                let cause = self.failure.take();
                self.failure_elected = false;
                self.discard = Some(Discard {
                    frames: segment.frames,
                    owner: Some(self.current_owner),
                    next: TransferNext::ResumeRunning {
                        saved_failure: cause,
                        propagate_exit: true,
                    },
                    aborted: true,
                });
            }
            InvocationEnd::Control { cont_id, .. } => self.unwind_discontinue(cont_id),
            InvocationEnd::Abort(_) => {}
        }
        Ok(())
    }

    pub(super) fn drain_dropped_continuations(&mut self) {
        let dropped = self.heap.take_dropped_continuations(self.execution);
        for id in dropped {
            if self
                .continuations
                .get(&id)
                .is_some_and(|slot| slot.continuation.is_none())
            {
                self.continuations.remove(&id);
            }
        }
    }

    pub(super) fn cleanup_handlers(&mut self, roots: &mut Vec<Value>) {
        for activation in mem::take(&mut self.activations).into_values() {
            Self::activation_roots(activation, roots);
        }
        for segment in mem::take(&mut self.segments).into_values() {
            self.drain_captured_roots(segment.frames, roots);
        }
        for slot in mem::take(&mut self.continuations).into_values() {
            if let Some(captured) = slot.continuation {
                self.drain_captured_roots(captured.frames, roots);
            }
        }
        if let Some(discard) = self.discard.take() {
            self.drain_discard_roots(discard, roots);
        }
        for inv in mem::take(&mut self.invocations).into_values() {
            if let Some(callee) = inv.callee {
                Self::reg_into_roots(callee, roots);
            }
            for arg in inv.arguments {
                Self::reg_into_roots(arg, roots);
            }
        }
    }
}
