//! Instruction dispatch and value/primitive/data operations for tier 1.
//!
//! All binary ops, unary ops, and standard-library intrinsics delegate directly
//! to [`crate::stdlib`] over [`crate::tier0::Heap`].

use std::collections::BTreeSet;
use std::mem;

use flick_core::ir::Lit;

use crate::stdlib::{self, FuelCounter, HostContext};
use crate::tier0::{Data, Error, HostMode, Value, VarHandle, patterns};

use super::machine::{Machine, RegVal};
use super::opcode::{Instr, ValueSource};

impl Machine<'_, '_> {
    #[allow(clippy::too_many_lines)]
    pub(super) fn step_instr(&mut self, instr: &Instr) -> Result<(), Error> {
        match instr {
            Instr::Literal { dst, value } => {
                self.store_raw(*dst, RegVal::Value(value.to_value()));
            }
            Instr::String { dst, value } => {
                let val = self.heap.literal(&Lit::String(value.clone()))?;
                self.store_raw(*dst, RegVal::Value(val));
            }
            Instr::BigInt { dst, value } => {
                let val = self.heap.literal(&Lit::BigInt(value.clone()))?;
                self.store_raw(*dst, RegVal::Value(val));
            }
            Instr::Label { dst, value } => {
                let val = self.heap.allocate(Data::Label(value.clone()));
                self.store_raw(*dst, RegVal::Value(val));
            }
            Instr::ServiceTag { dst, key } => {
                let val = self.heap.service_tag(key)?;
                self.store_raw(*dst, RegVal::Value(val));
            }
            Instr::ReadOwnerEvidence { dst } => {
                let value = self
                    .current_evidence
                    .map_or(Value::Unit, super::machine_state::OwnerEvidence::value);
                self.store_raw(*dst, RegVal::Value(value));
            }
            Instr::Dup { src, dst } => {
                let reg = self.peek_reg(*src)?;
                let dup = match reg {
                    RegVal::StaticClosure(code) => RegVal::StaticClosure(*code),
                    RegVal::Value(val) => {
                        self.heap.admit_not_reuse_token(val)?;
                        let rep = val.representation();
                        RegVal::Value(self.heap.dup(&rep)?)
                    }
                };
                self.store_raw(*dst, dup);
            }
            Instr::Drop { src } => {
                let reg = self.take_reg(*src)?;
                self.drop_reg(reg)?;
            }
            Instr::Binary {
                dst,
                op,
                left,
                right,
            } => {
                let val =
                    self.eval_compute(*op, ValueSource::Slot(*left), ValueSource::Slot(*right))?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::Compute {
                dst,
                op,
                left,
                right,
            } => {
                let val = self.eval_compute(*op, *left, *right)?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::Not { dst, src } => {
                self.stage_slots(&[*src])?;
                let res = stdlib::eval_not(&self.scratch[0])?;
                self.finish_primitive(*dst, res)?;
            }
            Instr::Neg { dst, num, src } => {
                self.stage_slots(&[*src])?;
                let res = stdlib::eval_neg(
                    self.heap,
                    *num,
                    &self.scratch[0],
                    &mut FuelCounter(&mut self.fuel),
                )?;
                self.finish_primitive(*dst, res)?;
            }
            Instr::Primitive { dst, kind, args } => {
                self.stage_slots(args)?;
                let res = stdlib::eval_primitive(
                    self.heap,
                    *kind,
                    &self.scratch,
                    &mut FuelCounter(&mut self.fuel),
                )?;
                self.finish_primitive(*dst, res)?;
            }
            Instr::StringPredicate {
                dst,
                kind,
                needle,
                subject,
            } => {
                self.stage_slots(&[*needle, *subject])?;
                let res = stdlib::eval_string_predicate(
                    self.heap,
                    *kind,
                    &self.scratch[0],
                    &self.scratch[1],
                )?;
                self.finish_primitive(*dst, res)?;
            }
            Instr::HostIntrinsic { dst, kind, src } => {
                self.stage_slots(&[*src])?;
                let mut hooks = HostContext::new(&mut self.fuel, self.host_mode, &mut self.stdout)
                    .with_stderr(&mut self.stderr)
                    .with_program_args(&self.program_args)
                    .with_host_services(&mut self.host_services);
                if self.host_mode == HostMode::Deterministic || self.test_clock_mode {
                    hooks = hooks.with_virtual_now_ns(self.virtual_now_ns);
                }
                if let Some(rng_id) = self.seeded_random
                    && let Some(rng) = self.isaac_rngs.get_mut(&rng_id)
                {
                    hooks = hooks.with_seeded_rng(rng);
                }
                let res = stdlib::eval_intrinsic(self.heap, *kind, &self.scratch[0], &mut hooks)?;
                self.finish_primitive(*dst, res)?;
            }
            Instr::HostService { dst, op, args } => {
                self.stage_slots(args)?;
                let mut hooks = HostContext::new(&mut self.fuel, self.host_mode, &mut self.stdout)
                    .with_stderr(&mut self.stderr)
                    .with_program_args(&self.program_args)
                    .with_host_services(&mut self.host_services);
                if self.host_mode == HostMode::Deterministic || self.test_clock_mode {
                    hooks = hooks.with_virtual_now_ns(self.virtual_now_ns);
                }
                if let Some(rng_id) = self.seeded_random
                    && let Some(rng) = self.isaac_rngs.get_mut(&rng_id)
                {
                    hooks = hooks.with_seeded_rng(rng);
                }
                let res = stdlib::eval_host_service(self.heap, *op, &self.scratch, &mut hooks)?;
                self.finish_primitive(*dst, res)?;
            }
            Instr::Cli { dst, op, args } => self.exec_cli(*dst, *op, args)?,
            Instr::Construct {
                dst,
                tag,
                fields,
                dicts,
                rep,
                layout,
            } => {
                self.stage_slots(fields)?;
                self.stage_slots(dicts)?;
                let mut fields_owned = mem::take(&mut self.scratch);
                let dicts_owned = fields_owned.split_off(fields.len());
                let val = self.heap.construct_with_rep_and_layout(
                    &self.image.tags[*tag],
                    fields_owned,
                    dicts_owned,
                    *rep,
                    layout.clone(),
                )?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::DropReuse { dst, object } => {
                self.stage_slots(&[*object])?;
                let candidate = self.scratch.pop().expect("drop_reuse candidate");
                let token = self.heap.drop_reuse(candidate)?;
                self.store_raw(*dst, RegVal::Value(token));
            }
            Instr::Reuse {
                dst,
                object,
                tag,
                fields,
                dicts,
                layout,
            } => {
                self.stage_slots(&[*object])?;
                self.stage_slots(fields)?;
                self.stage_slots(dicts)?;
                let candidate = self.scratch.remove(0);
                let mut fields_owned = mem::take(&mut self.scratch);
                let dicts_owned = fields_owned.split_off(fields.len());
                let val = self.heap.reuse_with_dicts_and_layout(
                    candidate,
                    &self.image.tags[*tag],
                    fields_owned,
                    dicts_owned,
                    layout.clone(),
                )?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::ReuseRecord {
                dst,
                object,
                fields,
            } => {
                self.stage_slots(&[*object])?;
                for (_, slot) in fields {
                    let val = self.take_value(*slot)?;
                    self.scratch.push(val);
                }
                let candidate = self.scratch.remove(0);
                let fields_owned = fields
                    .iter()
                    .map(|(name, _)| name.clone())
                    .zip(mem::take(&mut self.scratch))
                    .collect();
                let val = self.heap.reuse_record(candidate, fields_owned)?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::BoxAggregate { dst, value, layout } => {
                self.stage_slots(&[*value])?;
                let agg = self.scratch.pop().expect("aggregate");
                let boxed = self.heap.box_aggregate(agg, layout.clone())?;
                self.store_reg(*dst, RegVal::Value(boxed))?;
            }
            Instr::UnboxBorrow { dst, object } => {
                let receiver = self.peek_value(*object)?.representation();
                let val = self.heap.unbox_borrow(&receiver)?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::UnboxConsume { dst, object } => {
                self.stage_slots(&[*object])?;
                let boxed = self.scratch.pop().expect("boxed aggregate");
                let val = self.heap.unbox_consume(boxed)?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::Project { dst, object, field } => {
                let receiver = self.peek_value(*object)?.representation();
                let val = self.heap.project(&receiver, *field)?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::ProjectOwned { dst, object, field } => {
                self.stage_slots(&[*object])?;
                let val = self.heap.project(&self.scratch[0], *field)?;
                self.finish_primitive(*dst, val)?;
            }
            Instr::Record { dst, fields } => {
                let mut labels = BTreeSet::new();
                for (name, _) in fields {
                    if !labels.insert(name) {
                        return Err(Error::DuplicateField(name.clone()));
                    }
                }
                for (_, slot) in fields {
                    let val = self.take_value(*slot)?;
                    self.scratch.push(val);
                }
                let fields_owned = fields
                    .iter()
                    .map(|(name, _)| name.clone())
                    .zip(mem::take(&mut self.scratch))
                    .collect();
                let val = self.heap.record(fields_owned)?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::Field { dst, object, field } => {
                let receiver = self.peek_value(*object)?.representation();
                let val = self.heap.project_named(&receiver, field)?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::Update {
                dst,
                object,
                labels,
                values,
            } => {
                self.stage_slots(&[*object])?;
                self.stage_slots(values)?;
                let val =
                    patterns::update_labels(self.heap, &mut self.scratch, labels, &mut self.fuel)?;
                self.finish_primitive(*dst, val)?;
            }
            Instr::Match {
                dst,
                object,
                pattern,
            } => {
                let receiver = self.peek_value(*object)?.representation();
                let val = patterns::execute(
                    self.heap,
                    &receiver,
                    pattern,
                    &mut self.fuel,
                    &mut self.scratch,
                )?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::MatchFlag {
                dst,
                object,
                pattern,
            } => {
                let matched = self.eval_match_flag(*object, pattern)?;
                self.store_raw(*dst, RegVal::Value(Value::Bool(matched)));
            }
            Instr::MatchBranch {
                object,
                pattern,
                then_pc,
                else_pc,
            } => {
                let matched = self.eval_match_flag(*object, pattern)?;
                self.frame_mut().pc = if matched { *then_pc } else { *else_pc };
            }
            Instr::Closure {
                dst,
                code,
                captures,
            } => {
                if captures.is_empty() {
                    self.store_raw(*dst, RegVal::StaticClosure(*code));
                } else {
                    self.stage_slots(captures)?;
                    self.heap.admit_values(&self.scratch)?;
                    let captures_owned = mem::take(&mut self.scratch);
                    let val = self.heap.allocate(Data::Closure {
                        execution: self.execution,
                        code: self.image.ir_codes[*code],
                        captures: captures_owned,
                    });
                    self.store_raw(*dst, RegVal::Value(val));
                }
            }
            Instr::Task {
                dst,
                code,
                captures,
            } => self.exec_task(*dst, *code, captures)?,
            Instr::Dictionary { dst, entries } => self.exec_dictionary(*dst, entries)?,
            Instr::Method {
                dst,
                dictionary,
                field,
            } => self.exec_method(*dst, *dictionary, *field)?,
            Instr::Structure {
                dst,
                object,
                expected,
            } => self.exec_structure(*dst, *object, *expected)?,
            Instr::VarMake { dst, src } => {
                self.stage_slots(&[*src])?;
                let contents = self.scratch.pop().expect("var init");
                let root = self.heap.allocate(Data::Var {
                    execution: self.execution,
                    contents: Some(contents),
                });
                let id = root.object_id().expect("var object id");
                self.frame_mut().vars.push(root);
                self.store_raw(
                    *dst,
                    RegVal::Value(Value::Var(VarHandle {
                        id,
                        execution: self.execution,
                    })),
                );
            }
            Instr::VarGet { dst, var } => {
                self.stage_slots(&[*var])?;
                let Value::Var(handle) = &self.scratch[0] else {
                    return Err(Error::ExpectedVar);
                };
                let current = self
                    .heap
                    .var_contents(self.execution, handle)?
                    .representation();
                let val = self.heap.dup(&current)?;
                self.finish_primitive(*dst, val)?;
            }
            Instr::VarSet { dst, var, value } => {
                self.stage_slots(&[*var, *value])?;
                let handle = Self::expect_var(&self.scratch[0])?;
                let replacement = self.scratch.pop().expect("var replacement");
                let old = mem::replace(
                    self.heap.var_contents_mut(self.execution, &handle)?,
                    replacement,
                );
                self.scratch.push(old);
                self.finish_primitive(*dst, Value::Unit)?;
            }
            Instr::ReadCell { dst, var } => {
                let handle = Self::expect_var(self.peek_value(*var)?)?;
                let current = self
                    .heap
                    .var_contents(self.execution, &handle)?
                    .representation();
                let val = self.heap.dup(&current)?;
                self.store_reg(*dst, RegVal::Value(val))?;
            }
            Instr::WriteCell { var, value } => {
                self.stage_slots(&[*var, *value])?;
                let handle = Self::expect_var(&self.scratch[0])?;
                let replacement = self.scratch.pop().expect("cell replacement");
                let old = mem::replace(
                    self.heap.var_contents_mut(self.execution, &handle)?,
                    replacement,
                );
                self.scratch.push(old);
                self.release_operands()?;
            }
            Instr::UpdateCell { var, op, right } => {
                let handle = Self::expect_var(self.peek_value(*var)?)?;
                let left_rep = self
                    .heap
                    .var_contents(self.execution, &handle)?
                    .representation();
                let left_val = self.heap.dup(&left_rep)?;
                self.scratch.push(left_val);
                let right_val = self.read_source(*right)?;
                self.scratch.push(right_val);
                let _ = self.take_reg(*var)?;
                let result = stdlib::eval_binary(
                    self.heap,
                    *op,
                    &self.scratch[0],
                    &self.scratch[1],
                    &mut FuelCounter(&mut self.fuel),
                )?;
                let old =
                    mem::replace(self.heap.var_contents_mut(self.execution, &handle)?, result);
                self.scratch.push(old);
                self.release_operands()?;
            }
            other => self.step_control(other)?,
        }
        Ok(())
    }
}
