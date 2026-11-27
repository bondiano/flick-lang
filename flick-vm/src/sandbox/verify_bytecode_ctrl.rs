//! Control-flow, call, loop, and effect-handler instruction verification for
//! [`super::verify_bytecode`].

use std::collections::BTreeSet;

use flick_core::types::OpClass;

use super::verify_bytecode::BytecodeVerifyError;
use super::verify_bytecode_instr::{check_slot, check_source};
use crate::tier1::opcode::{CompiledHandlerOp, Instr, MAX_TAIL_ARGS, Prepared};

fn check_pc(
    func_idx: usize,
    pc: usize,
    len: usize,
    target: usize,
    role: &str,
) -> Result<(), BytecodeVerifyError> {
    if target >= len {
        return Err(BytecodeVerifyError::at(
            func_idx,
            pc,
            format!("{role} target pc {target} out of bounds (instructions: {len})"),
        ));
    }
    Ok(())
}

fn check_call_target(
    prepared: &Prepared,
    func_idx: usize,
    pc: usize,
    code: usize,
    arg_count: usize,
    role: &str,
) -> Result<(), BytecodeVerifyError> {
    let Some(target) = prepared.functions.get(code) else {
        return Err(BytecodeVerifyError::at(
            func_idx,
            pc,
            format!(
                "{role} target fn#{code} out of bounds (functions: {})",
                prepared.functions.len()
            ),
        ));
    };
    if target.params.len() != arg_count {
        return Err(BytecodeVerifyError::at(
            func_idx,
            pc,
            format!(
                "{role} arity mismatch for fn#{code}: expected {} args, got {arg_count}",
                target.params.len()
            ),
        ));
    }
    Ok(())
}

#[expect(clippy::too_many_arguments, reason = "validates all Handle fields")]
fn check_handler(
    prepared: &Prepared,
    func_idx: usize,
    pc: usize,
    slots: usize,
    dst: usize,
    body: usize,
    operations: &[CompiledHandlerOp],
    ret: Option<usize>,
    errors: Option<usize>,
) -> Result<(), BytecodeVerifyError> {
    check_slot(func_idx, pc, slots, dst, "dst")?;
    check_slot(func_idx, pc, slots, body, "handler body")?;
    if let Some(r) = ret {
        check_slot(func_idx, pc, slots, r, "handler return arm")?;
    }
    if let Some(e) = errors {
        check_slot(func_idx, pc, slots, e, "handler error arm")?;
    }
    let mut seen_ops = BTreeSet::new();
    for hop in operations {
        if hop.op_id >= prepared.operations.len() {
            return Err(BytecodeVerifyError::at(
                func_idx,
                pc,
                format!(
                    "handler op_id {} out of bounds (operations: {})",
                    hop.op_id,
                    prepared.operations.len()
                ),
            ));
        }
        if !seen_ops.insert(hop.op_id) {
            return Err(BytecodeVerifyError::at(
                func_idx,
                pc,
                format!("duplicate handler arm for op_id {}", hop.op_id),
            ));
        }
        check_slot(func_idx, pc, slots, hop.function, "handler op function")?;
        let extra = usize::from(matches!(hop.class, OpClass::Control));
        if hop
            .dicts
            .checked_add(hop.arity)
            .and_then(|n| n.checked_add(extra))
            .is_none()
        {
            return Err(BytecodeVerifyError::at(
                func_idx,
                pc,
                format!(
                    "handler op_id {} invocation argument count overflow",
                    hop.op_id
                ),
            ));
        }
    }
    Ok(())
}

pub(super) fn verify_control_and_call_instr(
    prepared: &Prepared,
    func_idx: usize,
    pc: usize,
    slots: usize,
    len: usize,
    instr: &Instr,
) -> Result<(), BytecodeVerifyError> {
    match instr {
        Instr::HostService { dst, op, args } => {
            check_slot(func_idx, pc, slots, *dst, "dst")?;
            if args.len() != op.arity() {
                return Err(BytecodeVerifyError::at(
                    func_idx,
                    pc,
                    format!(
                        "host service {op:?} expects {} args, got {}",
                        op.arity(),
                        args.len()
                    ),
                ));
            }
            for &arg in args {
                check_slot(func_idx, pc, slots, arg, "host service arg")?;
            }
            Ok(())
        }
        Instr::Closure {
            dst,
            code,
            captures,
        } => {
            check_slot(func_idx, pc, slots, *dst, "dst")?;
            check_call_target(prepared, func_idx, pc, *code, 2, "closure")?;
            for &slot in captures {
                check_slot(func_idx, pc, slots, slot, "closure capture")?;
            }
            Ok(())
        }
        Instr::Call { dst, code, args } | Instr::FunctionBoundary { dst, code, args } => {
            check_slot(func_idx, pc, slots, *dst, "dst")?;
            check_call_target(prepared, func_idx, pc, *code, args.len(), "call")?;
            for &arg in args {
                check_slot(func_idx, pc, slots, arg, "call arg")?;
            }
            Ok(())
        }
        Instr::TailCall { dst, code, args } => {
            check_slot(func_idx, pc, slots, *dst, "dst")?;
            if args.len() > MAX_TAIL_ARGS {
                return Err(BytecodeVerifyError::at(
                    func_idx,
                    pc,
                    format!(
                        "tail call has {} args, exceeding MAX_TAIL_ARGS ({MAX_TAIL_ARGS})",
                        args.len()
                    ),
                ));
            }
            check_call_target(prepared, func_idx, pc, *code, args.len(), "tail call")?;
            for &arg in args {
                check_slot(func_idx, pc, slots, arg, "tail call arg")?;
            }
            Ok(())
        }
        Instr::Loop {
            dst,
            id,
            code,
            args,
        } => {
            check_slot(func_idx, pc, slots, *dst, "dst")?;
            if *id >= prepared.loops.len() {
                return Err(BytecodeVerifyError::at(
                    func_idx,
                    pc,
                    format!(
                        "loop index {id} out of bounds (loops: {})",
                        prepared.loops.len()
                    ),
                ));
            }
            check_call_target(prepared, func_idx, pc, *code, args.len(), "loop")?;
            for &arg in args {
                check_slot(func_idx, pc, slots, arg, "loop arg")?;
            }
            Ok(())
        }
        Instr::LoopExit { id, value } => {
            if *id >= prepared.loops.len() {
                return Err(BytecodeVerifyError::at(
                    func_idx,
                    pc,
                    format!(
                        "loop exit index {id} out of bounds (loops: {})",
                        prepared.loops.len()
                    ),
                ));
            }
            if let Some(slot) = value {
                check_slot(func_idx, pc, slots, *slot, "loop exit value")?;
            }
            Ok(())
        }
        _ => verify_branch_and_effect_instr(prepared, func_idx, pc, slots, len, instr),
    }
}

fn verify_branch_and_effect_instr(
    prepared: &Prepared,
    func_idx: usize,
    pc: usize,
    slots: usize,
    len: usize,
    instr: &Instr,
) -> Result<(), BytecodeVerifyError> {
    match instr {
        Instr::Handle {
            dst,
            body,
            operations,
            ret,
            errors,
        } => check_handler(
            prepared, func_idx, pc, slots, *dst, *body, operations, *ret, *errors,
        ),
        Instr::Perform {
            dst,
            op_id,
            dicts,
            args,
        } => {
            check_slot(func_idx, pc, slots, *dst, "dst")?;
            if *op_id >= prepared.operations.len() {
                return Err(BytecodeVerifyError::at(
                    func_idx,
                    pc,
                    format!(
                        "perform op_id {op_id} out of bounds (operations: {})",
                        prepared.operations.len()
                    ),
                ));
            }
            for &slot in dicts.iter().chain(args.iter()) {
                check_slot(func_idx, pc, slots, slot, "perform operand")?;
            }
            Ok(())
        }
        Instr::MatchBranch {
            object,
            then_pc,
            else_pc,
            ..
        } => {
            check_slot(func_idx, pc, slots, *object, "match object")?;
            check_pc(func_idx, pc, len, *then_pc, "then")?;
            check_pc(func_idx, pc, len, *else_pc, "else")
        }
        Instr::Branch {
            condition,
            then_pc,
            else_pc,
        } => {
            check_slot(func_idx, pc, slots, *condition, "branch condition")?;
            check_pc(func_idx, pc, len, *then_pc, "then")?;
            check_pc(func_idx, pc, len, *else_pc, "else")
        }
        Instr::CompareBranch {
            left,
            right,
            then_pc,
            else_pc,
            ..
        } => {
            check_source(func_idx, pc, slots, *left, "compare left")?;
            check_source(func_idx, pc, slots, *right, "compare right")?;
            check_pc(func_idx, pc, len, *then_pc, "then")?;
            check_pc(func_idx, pc, len, *else_pc, "else")
        }
        _ => Ok(()),
    }
}
