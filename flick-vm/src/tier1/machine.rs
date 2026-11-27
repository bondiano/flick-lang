//! Tier-1 register bytecode virtual machine core, main loop, and leak-free cleanup.

use std::collections::{BTreeMap, VecDeque};
use std::mem;

use crate::stdlib::HostServices;
use crate::tier0::causes::StepFailure;
use crate::tier0::{
    CauseToken, Data, DefectSpec, Error, ExecutionReport, Heap, HostMode, LanguageExit,
    ProgramOutcome, Reason, Value, program as tier0_program,
};

use super::opcode::{Instr, Prepared};

/// Compact register value: captureless closures stay unallocated in registers
/// until they escape to a heap container, primitive, or top-level return.
#[derive(Debug)]
pub(crate) enum RegVal {
    Value(Value),
    StaticClosure(usize),
}

pub(super) enum GlobalState {
    Empty,
    Initializing,
    Ready(RegVal),
}

pub(super) struct LoopState {
    pub(super) id: usize,
    pub(super) code: usize,
    pub(super) args: Vec<RegVal>,
    pub(super) dst: usize,
}

pub(super) struct RecoveryState {
    pub(super) original: CauseToken,
    pub(super) dst: usize,
}

pub(super) enum End {
    Top,
    Store(usize),
    Global { id: usize, dst: usize },
    Loop(LoopState),
    FunctionBoundary { dst: usize },
    Try { selector: RegVal, dst: usize },
    RecoverySelection { id: usize },
    RecoveryHelper { id: usize, dst: usize },
}

pub(super) struct Frame {
    pub(super) code: usize,
    pub(super) pc: usize,
    pub(super) base: usize,
    pub(super) vars: Vec<Value>,
    pub(super) recovery_id: Option<usize>,
    pub(super) end: End,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MainStage {
    Function,
    Done,
}

pub(super) struct Machine<'p, 'h> {
    pub(super) heap: &'h mut Heap,
    pub(super) image: &'p Prepared,
    pub(super) execution: usize,
    pub(super) frames: Vec<Frame>,
    pub(super) registers: Vec<Option<RegVal>>,
    pub(super) globals: Vec<GlobalState>,
    pub(super) scratch: Vec<Value>,
    pub(super) pending: Option<RegVal>,
    pub(super) failure: Option<CauseToken>,
    pub(super) failure_elected: bool,
    pub(super) recoveries: BTreeMap<usize, RecoveryState>,
    pub(super) next_recovery: usize,
    pub(super) pending_handler: Option<RegVal>,
    pub(super) arguments: VecDeque<Value>,
    pub(super) top: Option<RegVal>,
    pub(super) main_stage: MainStage,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) program_args: Vec<String>,
    pub(super) host_services: HostServices,
    pub(super) fuel: u64,
    pub(super) host_mode: HostMode,
}

pub(super) fn execute_prepared(
    heap: &mut Heap,
    image: &Prepared,
    arguments: Vec<Value>,
    fuel: u64,
    host_mode: HostMode,
    program_args: Vec<String>,
    main_protocol: bool,
) -> ExecutionReport {
    if let Err(error) = heap.ensure_owned(&arguments) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(error),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    if let Err(error) = heap.admit_values(&arguments) {
        let _ = heap.drop_values(arguments);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(error),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let mut machine = Machine {
        heap,
        image,
        execution: tier0_program::next_execution_id(),
        frames: Vec::new(),
        registers: Vec::new(),
        globals: (0..image.globals.len())
            .map(|_| GlobalState::Empty)
            .collect(),
        scratch: Vec::new(),
        pending: None,
        failure: None,
        failure_elected: false,
        recoveries: BTreeMap::new(),
        next_recovery: 0,
        pending_handler: None,
        arguments: arguments.into(),
        top: None,
        main_stage: if main_protocol {
            MainStage::Function
        } else {
            MainStage::Done
        },
        stdout: String::new(),
        stderr: String::new(),
        program_args,
        host_services: HostServices::new(),
        fuel,
        host_mode,
    };
    let result = machine.start_entry().and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

impl Machine<'_, '_> {
    fn start_entry(&mut self) -> Result<(), Error> {
        let entry = self.image.entry;
        self.globals[entry] = GlobalState::Initializing;
        self.push_call(
            self.image.globals[entry],
            [],
            End::Global {
                id: entry,
                dst: usize::MAX,
            },
        )
    }

    fn finish(&mut self, result: Result<LanguageExit, Error>) -> ProgramOutcome {
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

    fn report_exit(&mut self, exit: LanguageExit) -> Result<ProgramOutcome, Error> {
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

    fn run(&mut self) -> Result<LanguageExit, Error> {
        let image = self.image;
        loop {
            if self.failure.is_some() && !self.failure_elected {
                self.prepare_failure()?;
            }
            if self.failure.is_some() && self.frames.is_empty() {
                let cause = self.failure.take().expect("failure root");
                return Ok(LanguageExit::Failure(cause));
            }
            if self.frames.is_empty() && self.arguments.is_empty() && self.top.is_some() {
                if self.main_stage != MainStage::Done {
                    self.advance_main()?;
                    continue;
                }
                let reg = self.top.take().expect("completed top value");
                let value = self.materialize_reg(reg);
                return Ok(LanguageExit::Success(value));
            }
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;

            if self.failure.is_some() {
                self.unwind_frame()?;
            } else if let Some(callee) = self.top.take() {
                let arg = self
                    .arguments
                    .pop_front()
                    .expect("top application has an argument");
                self.apply_reg(callee, RegVal::Value(arg), End::Top, false)?;
            } else {
                let (code, pc) = {
                    let frame = self.frames.last_mut().expect("running frame");
                    let at = frame.pc;
                    frame.pc += 1;
                    (frame.code, at)
                };
                let instr = &image.functions[code].instructions[pc];
                if let Err(error) = self.step(instr) {
                    match StepFailure::classify(error) {
                        StepFailure::Language(spec) => self.raise_defect(spec)?,
                        StepFailure::Evaluator(err) => return Err(err),
                    }
                }
            }
        }
    }

    fn advance_main(&mut self) -> Result<(), Error> {
        self.main_stage = MainStage::Done;
        let is_closure = match self.top.as_ref() {
            Some(RegVal::StaticClosure(_)) => true,
            Some(RegVal::Value(Value::Object(tok))) => {
                matches!(self.heap.object(tok.id)?.data, Data::Closure { .. })
            }
            _ => false,
        };
        if is_closure {
            if self.fuel == 0 {
                return Err(Error::OutOfFuel);
            }
            self.fuel -= 1;
            let callee = self.top.take().expect("main closure");
            self.apply_reg(callee, RegVal::Value(Value::Unit), End::Top, false)?;
        }
        Ok(())
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
            RegVal::Value(val) => Ok(RegVal::Value(self.heap.dup(val)?)),
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

    pub(super) fn drain_end_roots(end: End, roots: &mut Vec<Value>) {
        match end {
            End::Loop(state) => {
                for arg in state.args {
                    Self::reg_into_roots(arg, roots);
                }
            }
            End::Try { selector, .. } => Self::reg_into_roots(selector, roots),
            End::Top
            | End::Store(_)
            | End::Global { .. }
            | End::FunctionBoundary { .. }
            | End::RecoverySelection { .. }
            | End::RecoveryHelper { .. } => {}
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
            roots.extend(frame.vars);
            Self::drain_end_roots(frame.end, &mut roots);
        }
        for state in mem::take(&mut self.recoveries).into_values() {
            roots.push(state.original.into_value());
        }
        for global in mem::take(&mut self.globals) {
            if let GlobalState::Ready(reg) = global {
                Self::reg_into_roots(reg, &mut roots);
            }
        }
        self.heap.drop_values(roots)
    }

    pub(super) fn step(&mut self, instr: &Instr) -> Result<(), Error> {
        self.step_instr(instr)
    }
}
