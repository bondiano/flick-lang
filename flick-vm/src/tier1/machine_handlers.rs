//! Effect handler installation, epoch-cached lookup, service fallback, and
//! tail/abort/control dispatch for tier 1.

use std::collections::VecDeque;
use std::{iter, mem};

use flick_core::ir::memory::MemorySpan;
use flick_core::types::OpClass;

use crate::stdlib::runtime::layers as std_layers;
use crate::stdlib::{self, FuelCounter, HostContext};
use crate::tier0::{Data, Error, HostMode, Value};

use super::machine::{End, Machine, OwnerEvidence, RegVal};
use super::machine_unwind::{CapturedFrames, Discard, TransferNext};
use super::opcode::CompiledHandlerOp;

pub(super) struct InstalledOperation {
    pub(super) op_id: usize,
    pub(super) class: OpClass,
    pub(super) dicts: usize,
    pub(super) arity: usize,
    pub(super) function: RegVal,
}

pub(super) struct Activation {
    pub(super) operations: Vec<InstalledOperation>,
    pub(super) ret: Option<RegVal>,
    pub(super) errors: Option<RegVal>,
    pub(super) owner: usize,
}

pub(super) struct Segment {
    pub(super) frames: CapturedFrames,
    pub(super) owner: usize,
    pub(super) dst: usize,
    pub(super) frame_base: usize,
    pub(super) saved_epoch: u64,
    pub(super) arm_epoch: u64,
}

pub(super) struct CapturedContinuation {
    pub(super) frames: CapturedFrames,
    pub(super) owner: usize,
    pub(super) dst: usize,
}

pub(super) struct CapturedSlot {
    pub(super) continuation: Option<CapturedContinuation>,
    pub(super) perform_span: Option<MemorySpan>,
}

pub(super) enum InvocationEnd {
    Tail(usize),
    Abort(usize),
    Control { cont_id: usize, dst: usize },
    Caller(usize),
}

pub(super) struct Invocation {
    pub(super) callee: Option<RegVal>,
    pub(super) arguments: VecDeque<RegVal>,
    pub(super) end: InvocationEnd,
    pub(super) evidence: Option<OwnerEvidence>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CachedHandlerHit {
    pub(super) frame_index: usize,
    pub(super) activation_id: usize,
    pub(super) op_index: usize,
}

enum PerformLookup {
    Handler(CachedHandlerHit),
    Service(Value),
    None,
}

impl Machine<'_, '_> {
    pub(super) fn bump_handler_epoch(&mut self) {
        self.handler_epoch = self.next_handler_epoch;
        self.next_handler_epoch = self.next_handler_epoch.wrapping_add(1);
    }

    pub(super) fn handler_identity(&mut self) -> Result<usize, Error> {
        let id = self.next_handler;
        self.next_handler = id.checked_add(1).ok_or(Error::HandlerIdentityOverflow)?;
        Ok(id)
    }

    pub(super) fn transfer_fuel(&mut self, frames: usize) -> Result<(), Error> {
        let amount = u64::try_from(frames).map_err(|_| Error::OutOfFuel)?;
        self.fuel = self.fuel.checked_sub(amount).ok_or(Error::OutOfFuel)?;
        Ok(())
    }

    pub(super) fn activation_roots(activation: Activation, roots: &mut Vec<Value>) {
        for op in activation.operations {
            Self::reg_into_roots(op.function, roots);
        }
        if let Some(ret) = activation.ret {
            Self::reg_into_roots(ret, roots);
        }
        if let Some(errors) = activation.errors {
            Self::reg_into_roots(errors, roots);
        }
    }

    fn admit_reg_ordinary(&self, reg: &RegVal) -> Result<(), Error> {
        match reg {
            RegVal::StaticClosure(_) => Ok(()),
            RegVal::Value(val) => self.heap.admit_ordinary(val),
        }
    }

    pub(super) fn exec_handle(
        &mut self,
        dst: usize,
        body_slot: usize,
        compiled_ops: &[CompiledHandlerOp],
        ret_slot: Option<usize>,
        errors_slot: Option<usize>,
    ) -> Result<(), Error> {
        let mut taken: Vec<RegVal> = Vec::with_capacity(
            1 + compiled_ops.len()
                + usize::from(ret_slot.is_some())
                + usize::from(errors_slot.is_some()),
        );
        let slots = iter::once(body_slot)
            .chain(compiled_ops.iter().map(|op| op.function))
            .chain(ret_slot)
            .chain(errors_slot);
        for slot in slots {
            let reg = match self.take_reg(slot) {
                Ok(reg) => reg,
                Err(err) => {
                    let mut roots = Vec::new();
                    for r in taken {
                        Self::reg_into_roots(r, &mut roots);
                    }
                    self.heap.drop_values(roots)?;
                    return Err(err);
                }
            };
            let admit = self.admit_reg_ordinary(&reg);
            taken.push(reg);
            if let Err(err) = admit {
                let mut roots = Vec::new();
                for r in taken {
                    Self::reg_into_roots(r, &mut roots);
                }
                self.heap.drop_values(roots)?;
                return Err(err);
            }
        }
        let id = match self.handler_identity() {
            Ok(id) => id,
            Err(err) => {
                let mut roots = Vec::new();
                for r in taken {
                    Self::reg_into_roots(r, &mut roots);
                }
                self.heap.drop_values(roots)?;
                return Err(err);
            }
        };
        let mut iter = taken.into_iter();
        let body = iter.next().expect("handler body");
        let operations: Vec<InstalledOperation> = compiled_ops
            .iter()
            .map(|op| InstalledOperation {
                op_id: op.op_id,
                class: op.class,
                dicts: op.dicts,
                arity: op.arity,
                function: iter.next().expect("handler operation"),
            })
            .collect();
        let ret = ret_slot.map(|_| iter.next().expect("return arm"));
        let errors = errors_slot.map(|_| iter.next().expect("error dispatcher"));
        let activation = Activation {
            operations,
            ret,
            errors,
            owner: self.current_owner,
        };
        self.activations.insert(id, activation);
        self.bump_handler_epoch();
        self.apply_reg(
            body,
            RegVal::Value(Value::Unit),
            End::Handle {
                activation: id,
                dst,
            },
            false,
        )
    }

    fn lookup_perform(&mut self, op_id: usize) -> Result<PerformLookup, Error> {
        if self.handler_cache_epoch != self.handler_epoch {
            self.handler_cache.clear();
            self.handler_cache_epoch = self.handler_epoch;
        }
        if let Some(&cached) = self.handler_cache.get(&op_id) {
            #[cfg(test)]
            {
                self.handler_cache_hits += 1;
            }
            return Ok(cached.map_or(PerformLookup::None, PerformLookup::Handler));
        }
        #[cfg(test)]
        {
            self.handler_cache_misses += 1;
        }
        let effect = &self.image.operations[op_id].effect;
        let mut found = None;
        for at in (0..self.frames.len()).rev() {
            if let Some(impl_rep) = Self::find_end_service(
                self.heap,
                &self.frames[at].end,
                &self.providers,
                effect,
                &mut self.fuel,
            )? {
                return Ok(PerformLookup::Service(impl_rep));
            }
            if let End::Handle { activation, .. } = self.frames[at].end
                && let Some(handler) = self.activations.get(&activation)
                && let Some(op_index) = handler.operations.iter().position(|op| op.op_id == op_id)
            {
                found = Some(CachedHandlerHit {
                    frame_index: at,
                    activation_id: activation,
                    op_index,
                });
                break;
            }
        }
        self.handler_cache.insert(op_id, found);
        Ok(found.map_or(PerformLookup::None, PerformLookup::Handler))
    }

    pub(super) fn exec_perform(
        &mut self,
        dst: usize,
        op_id: usize,
        dicts: &[usize],
        args: &[usize],
        perform_span: Option<MemorySpan>,
    ) -> Result<(), Error> {
        self.stage_slots(dicts)?;
        self.stage_slots(args)?;
        let hit = match self.lookup_perform(op_id)? {
            PerformLookup::Service(impl_rep) => {
                let op = self.image.operations[op_id].op.clone();
                return self.invoke_service_impl(&impl_rep, dst, &op);
            }
            PerformLookup::Handler(hit) => hit,
            PerformLookup::None => {
                return self.perform_fallback(dst, op_id, dicts.len(), args.len());
            }
        };

        let End::Handle {
            dst: result_dst, ..
        } = self.frames[hit.frame_index].end
        else {
            unreachable!("cached handler hit must point at End::Handle");
        };
        let (expected_dicts, expected_args, class, outer_owner) = {
            let handler = &self.activations[&hit.activation_id];
            let op = &handler.operations[hit.op_index];
            (op.dicts, op.arity, op.class, handler.owner)
        };
        if expected_dicts != dicts.len() || expected_args != args.len() {
            let prep_op = &self.image.operations[op_id];
            return Err(Error::OperationArgumentCount {
                effect: prep_op.effect.clone(),
                op: prep_op.op.clone(),
                expected_dicts,
                actual_dicts: dicts.len(),
                expected_args,
                actual_args: args.len(),
            });
        }
        if class == OpClass::Control && self.maybe_cutoff_control(hit.frame_index)? {
            return Ok(());
        }
        let func = {
            let op = &self.activations[&hit.activation_id].operations[hit.op_index];
            match &op.function {
                RegVal::StaticClosure(code) => RegVal::StaticClosure(*code),
                RegVal::Value(val) => {
                    let rep = val.representation();
                    RegVal::Value(self.heap.dup(&rep)?)
                }
            }
        };
        self.dispatch_handler_operation(
            hit.frame_index,
            dst,
            result_dst,
            outer_owner,
            class,
            func,
            perform_span,
        )
    }

    fn perform_fallback(
        &mut self,
        dst: usize,
        op_id: usize,
        dict_count: usize,
        arg_count: usize,
    ) -> Result<(), Error> {
        let prep_op = &self.image.operations[op_id];
        let effect = prep_op.effect.clone();
        let op_name = prep_op.op.clone();
        let console_fallback = prep_op.console_fallback;
        let mut inherited_rep = None;
        for s in &self.inherited_services {
            if let Some(imp) = std_layers::lookup_layer_service(
                self.heap,
                s,
                &effect,
                &mut FuelCounter(&mut self.fuel),
            )? {
                inherited_rep = Some(imp.representation());
                break;
            }
        }
        if let Some(rep) = inherited_rep {
            return self.invoke_service_impl(&rep, dst, &op_name);
        }
        let inherited_op = self
            .inherited_tail_ops
            .iter()
            .find(|cand| cand.op_id == op_id)
            .map(|cand| {
                let func = match &cand.function {
                    RegVal::StaticClosure(code) => Ok(RegVal::StaticClosure(*code)),
                    RegVal::Value(val) => {
                        let rep = val.representation();
                        self.heap.dup(&rep).map(RegVal::Value)
                    }
                };
                (cand.dicts, cand.arity, func)
            });
        if let Some((expected_dicts, expected_args, func_res)) = inherited_op {
            if expected_dicts != dict_count || expected_args != arg_count {
                if let Ok(func) = func_res {
                    self.drop_reg(func)?;
                }
                return Err(Error::OperationArgumentCount {
                    effect,
                    op: op_name,
                    expected_dicts,
                    actual_dicts: dict_count,
                    expected_args,
                    actual_args: arg_count,
                });
            }
            let func = func_res?;
            return self.queue_service_invocation(func, dst);
        }
        if let Some(root) = self.root_with(&effect)? {
            return self.invoke_provider(root, dst, &effect, &op_name);
        }
        if self.root.is_none()
            && let Some(kind) = console_fallback
            && dict_count == 0
            && arg_count == 1
        {
            let mut hooks = HostContext::new(&mut self.fuel, self.host_mode, &mut self.stdout)
                .with_stderr(&mut self.stderr)
                .with_program_args(&self.program_args)
                .with_host_services(&mut self.host_services);
            if self.host_mode == HostMode::Deterministic || self.test_clock_mode {
                hooks = hooks.with_virtual_now_ns(self.virtual_now_ns);
            }
            let res = stdlib::eval_intrinsic(self.heap, kind, &self.scratch[0], &mut hooks)?;
            return self.finish_primitive(dst, res);
        }
        self.charge_fuel(effect.len().saturating_add(op_name.len()).saturating_add(1))?;
        Err(Error::MissingHandler {
            effect,
            op: op_name,
        })
    }

    fn maybe_cutoff_control(&mut self, at: usize) -> Result<bool, Error> {
        let mut cutoff_at = None;
        for idx in (at + 1..self.frames.len()).rev() {
            if self.is_cutoff_cleanup_end(&self.frames[idx].end)? {
                cutoff_at = Some(idx);
                break;
            }
        }
        let Some(idx) = cutoff_at else {
            return Ok(false);
        };
        self.release_operands()?;
        self.transfer_fuel(self.frames.len() - (idx + 1))?;
        let cleanup_frames = self.split_frames(idx + 1);
        self.bump_handler_epoch();
        self.discard = Some(Discard {
            frames: cleanup_frames,
            owner: Some(self.current_owner),
            next: TransferNext::FinishCutoff,
            aborted: true,
        });
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_handler_operation(
        &mut self,
        at: usize,
        dst: usize,
        result_dst: usize,
        outer_owner: usize,
        class: OpClass,
        func: RegVal,
        perform_span: Option<MemorySpan>,
    ) -> Result<(), Error> {
        if let Err(err) = self.transfer_fuel(self.frames.len() - at) {
            self.drop_reg(func)?;
            return Err(err);
        }
        let segment_id = match self.handler_identity() {
            Ok(id) => id,
            Err(err) => {
                self.drop_reg(func)?;
                return Err(err);
            }
        };
        let invocation_id = match self.handler_identity() {
            Ok(id) => id,
            Err(err) => {
                self.drop_reg(func)?;
                return Err(err);
            }
        };
        let saved_epoch = self.handler_epoch;
        let captured = self.split_frames(at);
        let old_owner = self.current_owner;
        self.current_owner = outer_owner;
        self.bump_handler_epoch();
        let arm_epoch = self.handler_epoch;

        let mut args: VecDeque<RegVal> = mem::take(&mut self.scratch)
            .into_iter()
            .map(RegVal::Value)
            .collect();
        let end = match class {
            OpClass::Tail => {
                self.segments.insert(
                    segment_id,
                    Segment {
                        frames: captured,
                        owner: old_owner,
                        dst,
                        frame_base: at,
                        saved_epoch,
                        arm_epoch,
                    },
                );
                InvocationEnd::Tail(segment_id)
            }
            OpClass::Abort => {
                self.discard = Some(Discard {
                    frames: captured,
                    owner: Some(outer_owner),
                    next: TransferNext::Invoke(invocation_id),
                    aborted: true,
                });
                InvocationEnd::Abort(result_dst)
            }
            OpClass::Control => {
                self.continuations.insert(
                    segment_id,
                    CapturedSlot {
                        continuation: Some(CapturedContinuation {
                            frames: captured,
                            owner: old_owner,
                            dst,
                        }),
                        perform_span,
                    },
                );
                let cont = self.heap.allocate(Data::Continuation {
                    execution: self.execution,
                    id: segment_id,
                });
                args.push_back(RegVal::Value(cont));
                InvocationEnd::Control {
                    cont_id: segment_id,
                    dst: result_dst,
                }
            }
        };
        self.invocations.insert(
            invocation_id,
            Invocation {
                callee: Some(func),
                arguments: args,
                end,
                evidence: self.current_evidence,
            },
        );
        if matches!(class, OpClass::Tail | OpClass::Control) {
            self.ready = Some(invocation_id);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "machine_handlers_tests.rs"]
mod tests;
