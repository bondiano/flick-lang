//! Frame unwinding, loops, `Option?`, and typed-error `try` recovery selection for tier 1.

use std::mem;

use crate::tier0::{Data, Error, Reason, Value};

use super::machine::{End, GlobalState, LoopState, Machine, RecoveryState, RegVal};
use super::machine_unwind::{Discard, TransferNext};

impl Machine<'_, '_> {
    pub(super) fn enter_loop(
        &mut self,
        dst: usize,
        id: usize,
        code: usize,
        args: &[usize],
    ) -> Result<(), Error> {
        let mut call_args = Vec::with_capacity(args.len());
        for &slot in args {
            call_args.push(self.take_reg(slot)?);
        }
        let mut saved = Vec::with_capacity(call_args.len());
        for arg in &call_args {
            match self.dup_reg(arg) {
                Ok(dup) => saved.push(dup),
                Err(err) => {
                    let mut roots = Vec::new();
                    for r in call_args.into_iter().chain(saved) {
                        Self::reg_into_roots(r, &mut roots);
                    }
                    self.heap.drop_values(roots)?;
                    return Err(err);
                }
            }
        }
        self.push_call(
            code,
            call_args,
            End::Loop(LoopState {
                id,
                code,
                args: saved,
                dst,
            }),
        )
    }

    pub(super) fn restart_loop(&mut self, state: LoopState) -> Result<(), Error> {
        let mut next_args = Vec::with_capacity(state.args.len());
        for arg in &state.args {
            match self.dup_reg(arg) {
                Ok(dup) => next_args.push(dup),
                Err(err) => {
                    let mut roots = Vec::new();
                    for r in state.args.into_iter().chain(next_args) {
                        Self::reg_into_roots(r, &mut roots);
                    }
                    self.heap.drop_values(roots)?;
                    return Err(err);
                }
            }
        }
        let code = state.code;
        self.push_call(code, next_args, End::Loop(state))
    }

    pub(super) fn exit_loop(&mut self, id: usize, value: Option<usize>) -> Result<(), Error> {
        let break_val = if let Some(slot) = value {
            Some(self.take_reg(slot)?)
        } else {
            None
        };
        let target = self
            .frames
            .iter()
            .rposition(|frame| matches!(&frame.end, End::Loop(state) if state.id == id));
        let Some(target) = target else {
            if let Some(val) = break_val {
                self.drop_reg(val)?;
            }
            return Err(Error::MissingLoopTarget(self.image.loops[id].clone()));
        };
        if let Some(val) = break_val {
            let captured = self.split_frames(target);
            let End::Loop(state) = &captured.frames[0].end else {
                unreachable!("validated loop target");
            };
            let dst = state.dst;
            self.bump_handler_epoch();
            self.discard = Some(Discard {
                frames: captured,
                owner: None,
                next: TransferNext::DeliverValue { dst, value: val },
                aborted: false,
            });
        } else {
            let mut captured = self.split_frames(target);
            let End::Loop(state) = mem::replace(&mut captured.frames[0].end, End::DiscardFinalizer)
            else {
                unreachable!("validated loop target");
            };
            self.bump_handler_epoch();
            self.discard = Some(Discard {
                frames: captured,
                owner: None,
                next: TransferNext::LoopContinue(state),
                aborted: false,
            });
        }
        Ok(())
    }

    pub(super) fn exec_unwrap_option(&mut self, dst: usize, option: usize) -> Result<(), Error> {
        self.stage_slots(&[option])?;
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedOption);
        };
        let (tag, ordinary_fields, total_fields) = match &self.heap.object(token.id)?.data {
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
                ..
            } => (tag.as_str(), *ordinary_fields, fields.len()),
            _ => return Err(Error::ExpectedOption),
        };
        match (tag, ordinary_fields, total_fields) {
            ("Prelude.Option.Some", 1, 1) => {
                let value = self.heap.project(&self.scratch[0], 0)?;
                self.finish_primitive(dst, value)
            }
            ("Prelude.Option.None", 0, 0) => {
                self.release_operands()?;
                let target = self
                    .frames
                    .iter()
                    .rposition(|f| matches!(f.end, End::FunctionBoundary { .. }))
                    .ok_or(Error::MissingFunctionBoundary)?;
                let captured = self.split_frames(target);
                let End::FunctionBoundary { dst: out_dst } = captured.frames[0].end else {
                    unreachable!("validated boundary frame");
                };
                self.bump_handler_epoch();
                self.discard = Some(Discard {
                    frames: captured,
                    owner: None,
                    next: TransferNext::ReturnNone { dst: out_dst },
                    aborted: false,
                });
                Ok(())
            }
            _ => Err(Error::ExpectedOption),
        }
    }

    pub(super) fn exec_from_result(&mut self, dst: usize, src: usize) -> Result<(), Error> {
        self.stage_slots(&[src])?;
        let Value::Object(token) = &self.scratch[0] else {
            return Err(Error::ExpectedResult);
        };
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
            ..
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedResult);
        };
        if *ordinary_fields != 1
            || fields.len() != 1
            || !matches!(tag.as_str(), "Prelude.Result.Ok" | "Prelude.Result.Err")
        {
            return Err(Error::ExpectedResult);
        }
        let failed = tag == "Prelude.Result.Err";
        let value = self.heap.project(&self.scratch[0], 0)?;
        if failed {
            self.scratch.push(value);
            self.raise_ordinary()
        } else {
            self.finish_primitive(dst, value)
        }
    }

    pub(super) fn unwind_frame(&mut self) -> Result<(), Error> {
        let mut frame = self.frames.pop().expect("unwound frame");
        let mut roots = Vec::new();
        for reg in self.registers.drain(frame.base..).flatten() {
            Self::reg_into_roots(reg, &mut roots);
        }
        self.heap.drop_values(roots)?;
        let gen_frames = self.release_frame_resources(&mut frame)?;
        self.restore_evidence();
        self.fail_end(frame.end)?;
        self.queue_discard_frames(gen_frames);
        Ok(())
    }

    pub(super) fn fail_end(&mut self, end: End) -> Result<(), Error> {
        match end {
            End::Handle { activation, dst } => self.handler_failed(activation, dst),
            End::Discontinue { cont_id, .. } => {
                self.unwind_discontinue(cont_id);
                Ok(())
            }
            End::Invocation(id) => self.invocation_failed(id),
            End::Try { selector, dst } => {
                if self.skip_interrupted_handler()? {
                    return self.drop_reg(selector);
                }
                self.pending_handler = Some(selector);
                self.begin_recovery(dst)
            }
            End::RecoverySelection { id } => self.abandon_recovery(id),
            End::RecoveryBody {
                owner, evidence, ..
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
                Ok(())
            }
            End::Global { id, .. } => {
                self.globals[id] = GlobalState::Empty;
                Ok(())
            }
            End::Modify { cell, .. } => {
                let _ = cell;
                Ok(())
            }
            End::CliStep { state, .. } => {
                let mut roots = Vec::new();
                state.drain_roots(&mut roots);
                self.heap.drop_values(roots)
            }
            End::Loop(state) => {
                let mut arg_roots = Vec::new();
                for arg in state.args {
                    Self::reg_into_roots(arg, &mut arg_roots);
                }
                self.heap.drop_values(arg_roots)
            }
            End::GenerateStrict { items, .. } => self.heap.drop_values(items),
            End::GeneratorStep { gen_id, .. } => {
                self.unwind_lazy_generator(gen_id);
                Ok(())
            }
            End::LazyComplete { lazy, .. } => self.unwind_lazy(&lazy),
            End::Top
            | End::Store(_)
            | End::FunctionBoundary { .. }
            | End::RecoveryHelper { .. }
            | End::DiscardFinalizer => Ok(()),
            other => self.unwind_scope_end(other),
        }
    }

    pub(super) fn begin_recovery(&mut self, dst: usize) -> Result<(), Error> {
        self.prepare_failure()?;
        let original = self.failure.as_ref().expect("failure root");
        let has_die = self
            .heap
            .cause_reasons(original)?
            .any(|reason| matches!(reason, Reason::Die(_)));
        if has_die {
            self.failure_elected = false;
            self.prepare_failure()?;
            let sel = self.pending_handler.take().expect("unused selector");
            return self.drop_reg(sel);
        }
        let Some(payload) = self.heap.first_fail(original, &mut self.fuel)? else {
            let sel = self.pending_handler.take().expect("unused selector");
            return self.drop_reg(sel);
        };
        self.charge_fuel(1)?;
        let id = self.next_recovery;
        self.next_recovery = id.checked_add(1).ok_or(Error::RecoveryIdentityOverflow)?;
        self.recoveries.insert(
            id,
            RecoveryState {
                original: self.failure.take().expect("eligible whole cause"),
                dst,
                owner: self.current_owner,
                evidence: self.current_evidence,
            },
        );
        self.charge_fuel(1)?;
        let arg = self.heap.dup(&payload)?;
        let selector = self.pending_handler.take().expect("rooted selector");
        self.apply_reg(
            selector,
            RegVal::Value(arg),
            End::RecoverySelection { id },
            false,
        )
    }

    pub(super) fn abandon_recovery(&mut self, id: usize) -> Result<(), Error> {
        let state = self
            .recoveries
            .remove(&id)
            .ok_or(Error::ForeignRecoveryResult)?;
        self.current_owner = state.owner;
        self.current_evidence = state.evidence;
        self.heap.drop_cause(state.original)
    }

    fn executing_recovery(&self) -> Result<usize, Error> {
        let id = self
            .frame()
            .recovery_id
            .ok_or(Error::RecoveryOutsideSelection)?;
        if !self.recoveries.contains_key(&id) {
            return Err(Error::ForeignRecoveryResult);
        }
        Ok(id)
    }

    pub(super) fn exec_recovery(
        &mut self,
        dst: usize,
        selected: Option<usize>,
    ) -> Result<(), Error> {
        let id = self.executing_recovery()?;
        let selected_val = if let Some(slot) = selected {
            let val = self.take_value(slot)?;
            self.scratch.push(val);
            self.heap
                .admit_ordinary(self.scratch.last().expect("selected closure"))?;
            self.checked_closure(&RegVal::Value(
                self.scratch.last().expect("selected").representation(),
            ))?;
            self.charge_fuel(1)?;
            self.scratch.pop()
        } else {
            self.charge_fuel(1)?;
            None
        };
        let marker = self.heap.allocate(Data::RecoveryResult {
            execution: self.execution,
            recovery_id: id,
            selected: selected_val,
        });
        self.store_transport(dst, RegVal::Value(marker), id)
    }

    pub(super) fn store_transport(
        &mut self,
        dst: usize,
        reg: RegVal,
        id: usize,
    ) -> Result<(), Error> {
        if self.executing_recovery()? != id {
            self.drop_reg(reg)?;
            return Err(Error::ForeignRecoveryResult);
        }
        if let RegVal::Value(val) = reg {
            self.scratch.push(val);
            let top = self.scratch.last().expect("transport root");
            if let Value::Object(tok) = top
                && matches!(self.heap.object(tok.id)?.data, Data::RecoveryResult { .. })
            {
                self.recovery_marker(top, id)?;
            } else {
                self.heap.admit_ordinary(top)?;
            }
            let validated = self.scratch.pop().expect("validated transport");
            self.store_raw(dst, RegVal::Value(validated));
        } else {
            self.store_raw(dst, reg);
        }
        Ok(())
    }

    fn recovery_marker(&self, value: &Value, id: usize) -> Result<Option<Value>, Error> {
        let Value::Object(token) = value else {
            return Err(Error::ExpectedRecoveryResult);
        };
        let Data::RecoveryResult {
            execution,
            recovery_id,
            selected,
        } = &self.heap.object(token.id)?.data
        else {
            return Err(Error::ExpectedRecoveryResult);
        };
        if *execution != self.execution || *recovery_id != id || !self.recoveries.contains_key(&id)
        {
            return Err(Error::ForeignRecoveryResult);
        }
        Ok(selected.as_ref().map(Value::representation))
    }

    pub(super) fn complete_recovery(&mut self, id: usize) -> Result<(), Error> {
        let Some(RegVal::Value(marker)) = self.pending.as_ref() else {
            return Err(Error::ExpectedRecoveryResult);
        };
        let selected = self.recovery_marker(marker, id)?;
        if let Some(sel_rep) = &selected {
            self.checked_closure(&RegVal::Value(sel_rep.representation()))?;
        }
        if let Some(sel_rep) = selected {
            self.charge_fuel(1)?;
            let body = self.heap.dup(&sel_rep)?;
            self.scratch.push(body);
        }
        let marker_reg = self.pending.take().expect("selector result");
        self.drop_reg(marker_reg)?;
        let state = self
            .recoveries
            .remove(&id)
            .ok_or(Error::ForeignRecoveryResult)?;
        self.current_owner = state.owner;
        self.current_evidence = state.evidence;
        if let Some(body) = self.scratch.pop() {
            self.heap.drop_cause(state.original)?;
            self.apply_reg(
                RegVal::Value(body),
                RegVal::Value(Value::Unit),
                End::RecoveryBody {
                    dst: state.dst,
                    owner: state.owner,
                    evidence: state.evidence,
                },
                false,
            )
        } else {
            self.failure = Some(state.original);
            self.failure_elected = true;
            Ok(())
        }
    }
}
