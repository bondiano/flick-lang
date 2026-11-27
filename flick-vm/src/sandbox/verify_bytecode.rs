//! Security verifier for tier-1 register bytecode ([`Prepared`]).
//!
//! Validates every structural invariant before tier-1 execution or future
//! native-region compilation:
//! - function / `ir_codes` / `code_indices` table consistency
//! - entry and global initializer indices (and zero-parameter initializers)
//! - register, local, and var/cell slot bounds (`< func.slots`)
//! - jump and branch target bounds (`< func.instructions.len()`)
//! - terminator presence at end of every function body
//! - tag, dictionary-field, loop, global, and interned operation indices
//! - static call, tail-call (`<= MAX_TAIL_ARGS`), loop, closure, and host-service arities
//! - handler frame invariants (operation IDs, unique op IDs per frame, return/error arms,
//!   non-overflowing invocation counts)

use std::collections::BTreeSet;
use std::{error, fmt};

use flick_core::ir::memory::{HostPrimitive, Intrinsic};

use super::verify_bytecode_instr::verify_instr;
use crate::tier1::opcode::{CompiledFunction, Instr, Prepared};

/// Structured error returned when tier-1 register bytecode fails verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BytecodeVerifyError {
    pub function: Option<usize>,
    pub pc: Option<usize>,
    pub detail: String,
}

impl BytecodeVerifyError {
    pub(super) fn module(detail: impl Into<String>) -> Self {
        Self {
            function: None,
            pc: None,
            detail: detail.into(),
        }
    }

    pub(super) fn func(function: usize, detail: impl Into<String>) -> Self {
        Self {
            function: Some(function),
            pc: None,
            detail: detail.into(),
        }
    }

    pub(super) fn at(function: usize, pc: usize, detail: impl Into<String>) -> Self {
        Self {
            function: Some(function),
            pc: Some(pc),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for BytecodeVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.function, self.pc) {
            (Some(func), Some(pc)) => write!(f, "fn#{func}@pc{pc}: {}", self.detail),
            (Some(func), None) => write!(f, "fn#{func}: {}", self.detail),
            _ => f.write_str(&self.detail),
        }
    }
}

impl error::Error for BytecodeVerifyError {}

/// Verifies every table, index, register/var slot bound, branch target, arity,
/// and handler-frame invariant of a tier-1 [`Prepared`] bytecode image.
///
/// # Errors
/// Returns [`BytecodeVerifyError`] identifying the offending function and PC on
/// the first violated invariant. Never panics on malformed input.
pub fn verify_bytecode(prepared: &Prepared) -> Result<(), BytecodeVerifyError> {
    verify_tables(prepared)?;
    for (func_idx, func) in prepared.functions.iter().enumerate() {
        verify_function(prepared, func_idx, func)?;
    }
    Ok(())
}

fn verify_tables(prepared: &Prepared) -> Result<(), BytecodeVerifyError> {
    let fn_count = prepared.functions.len();
    if fn_count == 0 {
        return Err(BytecodeVerifyError::module(
            "bytecode image contains no functions",
        ));
    }
    if prepared.ir_codes.len() != fn_count {
        return Err(BytecodeVerifyError::module(format!(
            "ir_codes length ({}) does not match functions length ({fn_count})",
            prepared.ir_codes.len()
        )));
    }
    for (&ir_code, &idx) in &prepared.code_indices {
        if idx >= fn_count {
            return Err(BytecodeVerifyError::module(format!(
                "code_indices[{ir_code}] = {idx} out of bounds (functions: {fn_count})"
            )));
        }
        if prepared.ir_codes[idx] != ir_code {
            return Err(BytecodeVerifyError::module(format!(
                "code_indices[{ir_code}] points to fn#{idx} whose ir_code is {}",
                prepared.ir_codes[idx]
            )));
        }
    }
    if prepared.globals.len() != prepared.global_names.len() {
        return Err(BytecodeVerifyError::module(format!(
            "globals length ({}) does not match global_names length ({})",
            prepared.globals.len(),
            prepared.global_names.len()
        )));
    }
    if prepared.globals.is_empty() {
        return Err(BytecodeVerifyError::module(
            "bytecode image contains no globals",
        ));
    }
    if prepared.entry >= prepared.globals.len() {
        return Err(BytecodeVerifyError::module(format!(
            "entry global index {} out of bounds (globals: {})",
            prepared.entry,
            prepared.globals.len()
        )));
    }
    for (g_idx, &init_fn) in prepared.globals.iter().enumerate() {
        let Some(func) = prepared.functions.get(init_fn) else {
            return Err(BytecodeVerifyError::module(format!(
                "global#{g_idx} initializer fn#{init_fn} out of bounds (functions: {fn_count})"
            )));
        };
        if !func.params.is_empty() {
            return Err(BytecodeVerifyError::module(format!(
                "global#{g_idx} initializer fn#{init_fn} expects {} params, must be 0",
                func.params.len()
            )));
        }
    }
    for (op_idx, op) in prepared.operations.iter().enumerate() {
        if let Some(fallback) = op.console_fallback
            && !matches!(
                fallback,
                Intrinsic::PrintLine
                    | Intrinsic::Host(HostPrimitive::StdoutWrite | HostPrimitive::StderrLine)
            )
        {
            return Err(BytecodeVerifyError::module(format!(
                "operation#{op_idx} has non-console fallback intrinsic {fallback:?}"
            )));
        }
    }
    Ok(())
}

fn verify_function(
    prepared: &Prepared,
    func_idx: usize,
    func: &CompiledFunction,
) -> Result<(), BytecodeVerifyError> {
    if func.instructions.is_empty() {
        return Err(BytecodeVerifyError::func(
            func_idx,
            "function has zero instructions",
        ));
    }
    if func.spans.len() != func.instructions.len() {
        return Err(BytecodeVerifyError::func(
            func_idx,
            format!(
                "spans length ({}) does not match instructions length ({})",
                func.spans.len(),
                func.instructions.len()
            ),
        ));
    }
    let mut seen_params = BTreeSet::new();
    for &param_slot in &func.params {
        if param_slot >= func.slots {
            return Err(BytecodeVerifyError::func(
                func_idx,
                format!(
                    "parameter slot {param_slot} out of bounds (slots: {})",
                    func.slots
                ),
            ));
        }
        if !seen_params.insert(param_slot) {
            return Err(BytecodeVerifyError::func(
                func_idx,
                format!("duplicate parameter slot {param_slot}"),
            ));
        }
    }

    for (pc, instr) in func.instructions.iter().enumerate() {
        verify_instr(prepared, func_idx, func, pc, instr)?;
    }

    let last_pc = func.instructions.len() - 1;
    if !is_terminator(&func.instructions[last_pc]) {
        return Err(BytecodeVerifyError::at(
            func_idx,
            last_pc,
            "function body does not end with a control-flow terminator",
        ));
    }
    Ok(())
}

const fn is_terminator(instr: &Instr) -> bool {
    matches!(
        instr,
        Instr::Return { .. }
            | Instr::ReturnUnit
            | Instr::Branch { .. }
            | Instr::CompareBranch { .. }
            | Instr::MatchBranch { .. }
            | Instr::Raise { .. }
            | Instr::Die { .. }
            | Instr::FailureRethrow { .. }
            | Instr::MatchFail
            | Instr::LoopExit { .. }
    )
}
