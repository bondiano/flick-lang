//! Register/value helpers, operand evaluation, frame resource release, and
//! leak-free execution cleanup for tier 1.

use std::mem;

use flick_core::ir::{BinOp, Pat};

use crate::stdlib::{self, FuelCounter};
use crate::tier0::{
    Data, DefectSpec, Error, IteratorState, LanguageExit, ProgramOutcome, Reason, Value, VarHandle,
    patterns,
};

use super::machine::{Frame, GlobalState, Machine, RegVal};
use super::machine_generators::GeneratorRun;
use super::machine_layers::MemoRestore;
use super::machine_unwind::{CapturedFrames, Discard, TransferNext};
use super::opcode::ValueSource;

impl Machine<'_, '_> {
    pub(super) fn finish(&mut self, result: Result<LanguageExit, Error>) -> ProgramOutcome {
        let outcome = match result.and_then(|exit| self.report_exit(exit)) {
            Ok(outcome) => outcome,
            Err(error) => ProgramOutcome::Fault(error),
        };
        if let Err(error) = self.cleanup() {
            let _ = outcome.release(self.heap);
            return ProgramOutcome::Fault(error);
        }
        outcome
    }

    pub(super) fn report_exit(&mut self, exit: LanguageExit) -> Result<ProgramOutcome, Error> {
        match exit {
            LanguageExit::Success(value) => {
                self.scratch.push(value);
                self.heap
                    .admit_ordinary(self.scratch.last().expect("success root"))?;
                Ok(ProgramOutcome::Returned(
                    self.scratch.pop().expect("ordinary success"),
                ))
            }
            LanguageExit::Failure(cause) => {
                self.failure = Some(cause);
                if self
                    .heap
                    .ordinary_failure(self.failure.as_ref().expect("failure root"))?
                    .is_some()
                {
                    Ok(ProgramOutcome::Raised(self.take_failure_payload()?))
                } else {
                    Ok(ProgramOutcome::Failed(
                        self.failure.take().expect("failure root"),
                    ))
                }
            }
        }
    }

    pub(super) fn materialize_reg(&mut self, reg: RegVal) -> Value {
        match reg {
            RegVal::Value(val) => val,
            RegVal::StaticClosure(code) => self.heap.allocate(Data::Closure {
                execution: self.execution,
                code: self.image.ir_codes[code],
                captures: Vec::new(),
            }),
        }
    }

    pub(super) fn dup_reg(&mut self, reg: &RegVal) -> Result<RegVal, Error> {
        match reg {
            RegVal::StaticClosure(code) => Ok(RegVal::StaticClosure(*code)),
            RegVal::Value(val) => {
                let rep = val.representation();
                Ok(RegVal::Value(self.heap.dup(&rep)?))
            }
        }
    }

    pub(super) fn drop_reg(&mut self, reg: RegVal) -> Result<(), Error> {
        match reg {
            RegVal::StaticClosure(_) => Ok(()),
            RegVal::Value(val) => self.heap.drop_value(val),
        }
    }

    pub(super) fn reg_into_roots(reg: RegVal, roots: &mut Vec<Value>) {
        if let RegVal::Value(val) = reg {
            roots.push(val);
        }
    }

    pub(super) fn take_reg(&mut self, slot: usize) -> Result<RegVal, Error> {
        let index = self.frame().base + slot;
        self.registers[index]
            .take()
            .ok_or(Error::NoToken(u32::try_from(slot).unwrap_or(u32::MAX)))
    }

    pub(super) fn take_value(&mut self, slot: usize) -> Result<Value, Error> {
        let reg = self.take_reg(slot)?;
        Ok(self.materialize_reg(reg))
    }

    pub(super) fn peek_reg(&self, slot: usize) -> Result<&RegVal, Error> {
        let index = self.frame().base + slot;
        self.registers[index]
            .as_ref()
            .ok_or(Error::NoToken(u32::try_from(slot).unwrap_or(u32::MAX)))
    }

    pub(super) fn peek_value(&self, slot: usize) -> Result<&Value, Error> {
        match self.peek_reg(slot)? {
            RegVal::Value(val) => Ok(val),
            RegVal::StaticClosure(_) => Err(Error::ExpectedObject),
        }
    }

    pub(super) fn store_reg(&mut self, slot: usize, reg: RegVal) -> Result<(), Error> {
        if let RegVal::Value(val) = reg {
            self.scratch.push(val);
            self.heap
                .admit_ordinary(self.scratch.last().expect("ordinary result"))?;
            let val = self.scratch.pop().expect("admitted result");
            self.store_raw(slot, RegVal::Value(val));
        } else {
            self.store_raw(slot, reg);
        }
        Ok(())
    }

    pub(super) fn store_raw(&mut self, slot: usize, reg: RegVal) {
        let index = self.frame().base + slot;
        debug_assert!(
            self.registers[index].is_none(),
            "store to occupied register {slot}"
        );
        self.registers[index] = Some(reg);
    }

    pub(super) fn frame(&self) -> &Frame {
        self.frames.last().expect("running frame")
    }

    pub(super) fn frame_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("running frame")
    }

    pub(super) fn release_operands(&mut self) -> Result<(), Error> {
        self.heap.drop_values(mem::take(&mut self.scratch))
    }

    pub(super) fn stage_slots(&mut self, slots: &[usize]) -> Result<(), Error> {
        for &slot in slots {
            let val = self.take_value(slot)?;
            self.scratch.push(val);
        }
        Ok(())
    }

    pub(super) fn raise_ordinary(&mut self) -> Result<(), Error> {
        let payload = self.scratch.last().expect("staged typed payload");
        let cause = self.heap.cause(&[Reason::Fail(payload)], &mut self.fuel)?;
        self.failure = Some(cause);
        self.failure_elected = false;
        self.release_operands()
    }

    pub(super) fn raise_defect(&mut self, spec: DefectSpec) -> Result<(), Error> {
        self.charge_fuel(1)?;
        let defect = self.heap.allocate(Data::Defect(spec));
        self.scratch.push(defect);
        let payload = self.scratch.last().expect("staged defect");
        let cause = self.heap.cause(&[Reason::Die(payload)], &mut self.fuel)?;
        self.failure = Some(cause);
        self.failure_elected = false;
        self.release_operands()
    }

    pub(super) fn charge_fuel(&mut self, amount: usize) -> Result<(), Error> {
        let cost = u64::try_from(amount).unwrap_or(u64::MAX);
        self.fuel = self.fuel.checked_sub(cost).ok_or(Error::OutOfFuel)?;
        Ok(())
    }

    pub(super) fn prepare_failure(&mut self) -> Result<(), Error> {
        if self.failure_elected {
            return Ok(());
        }
        let cause = self.failure.as_ref().expect("failure root");
        if let Some(elected) = self.heap.elect_failure(cause, &mut self.fuel)? {
            let original = self.failure.replace(elected).expect("original failure");
            self.heap.drop_cause(original)?;
        }
        self.failure_elected = true;
        Ok(())
    }

    pub(super) fn take_failure_payload(&mut self) -> Result<Value, Error> {
        self.charge_fuel(1)?;
        let rep = self
            .heap
            .ordinary_failure(self.failure.as_ref().expect("failure root"))?
            .expect("singleton ordinary Fail")
            .representation();
        let dup = self.heap.dup(&rep)?;
        self.pending = Some(RegVal::Value(dup));
        self.heap
            .drop_cause(self.failure.take().expect("failure root"))?;
        let Some(RegVal::Value(val)) = self.pending.take() else {
            unreachable!("staged payload value");
        };
        Ok(val)
    }

    pub(super) fn detach_frame_vars(&mut self, vars: Vec<Value>, roots: &mut Vec<Value>) {
        for root in &vars {
            if let Value::Object(token) = root
                && let Ok(object) = self.heap.object_mut(token.id)
                && let Data::Var { contents, .. } = &mut object.data
            {
                roots.extend(contents.take());
            }
        }
        roots.extend(vars);
    }

    pub(super) fn drain_captured_roots(
        &mut self,
        captured: CapturedFrames,
        roots: &mut Vec<Value>,
    ) {
        for reg in captured.registers.into_iter().flatten() {
            Self::reg_into_roots(reg, roots);
        }
        for frame in captured.frames {
            self.detach_frame_vars(frame.vars, roots);
            self.drain_frame_iterators(frame.iterators, roots);
            self.drain_end_roots(frame.end, roots);
        }
    }

    pub(super) fn cleanup(&mut self) -> Result<(), Error> {
        let mut roots = mem::take(&mut self.scratch);
        if let Some(reg) = self.pending.take() {
            Self::reg_into_roots(reg, &mut roots);
        }
        if let Some(cause) = self.failure.take() {
            roots.push(cause.into_value());
        }
        if let Some(reg) = self.pending_handler.take() {
            Self::reg_into_roots(reg, &mut roots);
        }
        if let Some(reg) = self.top.take() {
            Self::reg_into_roots(reg, &mut roots);
        }
        roots.extend(self.arguments.drain(..));
        for reg in mem::take(&mut self.registers).into_iter().flatten() {
            Self::reg_into_roots(reg, &mut roots);
        }
        for frame in mem::take(&mut self.frames) {
            self.detach_frame_vars(frame.vars, &mut roots);
            self.drain_frame_iterators(frame.iterators, &mut roots);
            self.drain_end_roots(frame.end, &mut roots);
        }
        self.cleanup_handlers(&mut roots);
        for provider in mem::take(&mut self.providers).into_values() {
            roots.push(provider.layer);
            if let MemoRestore::Restore(Some(m)) = provider.restore_memo {
                roots.push(m);
            }
        }
        for slot in mem::take(&mut self.generators).into_values() {
            match slot.run {
                GeneratorRun::Fresh { seq, .. } => roots.push(seq),
                GeneratorRun::Suspended { frames, .. } => {
                    self.drain_captured_roots(frames, &mut roots);
                }
                GeneratorRun::Running | GeneratorRun::Done => {}
            }
        }
        self.cleanup_async_roots(&mut roots);
        for state in mem::take(&mut self.recoveries).into_values() {
            roots.push(state.original.into_value());
        }
        for global in mem::take(&mut self.globals) {
            if let GlobalState::Ready(reg) = global {
                Self::reg_into_roots(reg, &mut roots);
            }
        }
        let mut owner_err = None;
        for owner in (0..self.owners.len()).rev() {
            if let Err(err) = self.close_owner(owner) {
                owner_err.get_or_insert(err);
            }
        }
        self.detach_root_cells_and_lazies(&mut roots, &mut owner_err);
        let released = self.heap.drop_values(roots);
        let _ = self.heap.take_dropped_continuations(self.execution);
        owner_err.map_or(released, Err)
    }

    pub(super) fn eval_match_flag(&mut self, object: usize, pattern: &Pat) -> Result<bool, Error> {
        let receiver = self.peek_value(object)?.representation();
        patterns::test_only(self.heap, &receiver, pattern, &mut self.fuel)
    }

    pub(super) fn eval_compute(
        &mut self,
        op: BinOp,
        left: ValueSource,
        right: ValueSource,
    ) -> Result<Value, Error> {
        let left_val = self.read_source(left)?;
        self.scratch.push(left_val);
        let right_val = self.read_source(right)?;
        self.scratch.push(right_val);
        let res = stdlib::eval_binary_slice(
            self.heap,
            op,
            &self.scratch,
            &mut FuelCounter(&mut self.fuel),
        )?;
        self.pending = Some(RegVal::Value(res));
        self.release_operands()?;
        let Some(RegVal::Value(val)) = self.pending.take() else {
            unreachable!("staged compute result");
        };
        Ok(val)
    }

    pub(super) fn read_source(&mut self, source: ValueSource) -> Result<Value, Error> {
        match source {
            ValueSource::Slot(slot) => self.take_value(slot),
            ValueSource::Cell(slot) => {
                let handle = Self::expect_var(self.peek_value(slot)?)?;
                let current = self
                    .heap
                    .var_contents(self.execution, &handle)?
                    .representation();
                self.heap.dup(&current)
            }
            ValueSource::Immediate(imm) => Ok(imm.to_value()),
        }
    }

    pub(super) fn expect_var(value: &Value) -> Result<VarHandle, Error> {
        let Value::Var(handle) = value else {
            return Err(Error::ExpectedVar);
        };
        Ok(VarHandle {
            id: handle.id,
            execution: handle.execution,
        })
    }

    pub(super) fn finish_primitive(&mut self, dst: usize, result: Value) -> Result<(), Error> {
        self.pending = Some(RegVal::Value(result));
        self.release_operands()?;
        let reg = self.pending.take().expect("staged primitive result");
        self.store_reg(dst, reg)
    }
    pub(super) fn release_frame_resources(
        &mut self,
        frame: &mut Frame,
    ) -> Result<CapturedFrames, Error> {
        let mut roots = Vec::new();
        self.detach_frame_vars(mem::take(&mut frame.vars), &mut roots);
        let mut gen_frames = CapturedFrames::default();
        for root in &frame.iterators {
            if let Value::Object(token) = root
                && let Ok(object) = self.heap.object_mut(token.id)
                && let Data::Iterator { state, .. } = &mut object.data
            {
                let prev = mem::replace(state, IteratorState::Exhausted);
                match prev {
                    IteratorState::List { backing, .. } => roots.push(backing),
                    IteratorState::Generator { id } => {
                        if let Some(slot) = self.generators.remove(&id) {
                            match slot.run {
                                GeneratorRun::Fresh { seq, .. } => roots.push(seq),
                                GeneratorRun::Suspended { frames, .. } => {
                                    Self::extend_captured(&mut gen_frames, frames);
                                }
                                GeneratorRun::Running | GeneratorRun::Done => {}
                            }
                        }
                    }
                    IteratorState::Exhausted => {}
                }
            }
        }
        roots.extend(mem::take(&mut frame.iterators));
        self.heap.drop_values(roots)?;
        Ok(gen_frames)
    }

    pub(super) fn drain_frame_iterators(&mut self, iterators: Vec<Value>, roots: &mut Vec<Value>) {
        for root in &iterators {
            if let Value::Object(token) = root
                && let Ok(object) = self.heap.object_mut(token.id)
                && let Data::Iterator { state, .. } = &mut object.data
            {
                let prev = mem::replace(state, IteratorState::Exhausted);
                match prev {
                    IteratorState::List { backing, .. } => roots.push(backing),
                    IteratorState::Generator { id } => {
                        if let Some(slot) = self.generators.remove(&id) {
                            match slot.run {
                                GeneratorRun::Fresh { seq, .. } => roots.push(seq),
                                GeneratorRun::Suspended { frames, .. } => {
                                    self.drain_captured_roots(frames, roots);
                                }
                                GeneratorRun::Running | GeneratorRun::Done => {}
                            }
                        }
                    }
                    IteratorState::Exhausted => {}
                }
            }
        }
        roots.extend(iterators);
    }

    pub(super) fn queue_discard_frames(&mut self, frames: CapturedFrames) {
        if frames.frames.is_empty() {
            return;
        }
        let next = if let Some(prev) = self.discard.take() {
            TransferNext::Chain(Box::new(prev))
        } else {
            let saved_failure = self.failure.take();
            if saved_failure.is_some() {
                self.failure_elected = false;
            }
            TransferNext::ResumeRunning {
                saved_failure,
                propagate_exit: false,
            }
        };
        self.discard = Some(Discard {
            frames,
            owner: Some(self.current_owner),
            next,
            aborted: false,
        });
    }
}
