//! Per-operation translators for effect handlers, `perform`, and synchronous scope finalizers.
//!
//! Kept as dedicated `translate_*` functions so formal per-operation refinement
//! proofs (`#980`) map 1-to-1 onto each memory-IR operation.

use flick_core::ir::memory::{HandlerEffect, Local};

use super::opcode::{CompiledHandlerOp, Instr};
use super::prepare::{Lowering, Tokens};

impl Lowering<'_> {
    /// Translates `Operation::Handle` into [`Instr::Handle`], consuming operands
    /// in exact `Operation::consumed()` order: `body`, operation functions,
    /// optional `ret`, optional `errors`.
    pub(super) fn translate_handle(
        &mut self,
        body: Local,
        effects: &[HandlerEffect],
        ret: Option<Local>,
        errors: Option<Local>,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        let body = tokens.take(body);
        let mut operations = Vec::new();
        for effect in effects {
            for op in &effect.operations {
                let op_id = self.intern_operation(&effect.effect, &op.name);
                operations.push(CompiledHandlerOp {
                    op_id,
                    class: op.class,
                    dicts: op.dicts,
                    arity: op.arity,
                    function: tokens.take(op.function),
                });
            }
        }
        let ret = ret.map(|local| tokens.take(local));
        let errors = errors.map(|local| tokens.take(local));
        Instr::Handle {
            dst,
            body,
            operations,
            ret,
            errors,
        }
    }

    /// Translates `Operation::Perform` into [`Instr::Perform`], consuming
    /// dictionaries before ordinary arguments.
    pub(super) fn translate_perform(
        &mut self,
        effect: &str,
        op_name: &str,
        dicts: &[Local],
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        let op_id = self.intern_operation(effect, op_name);
        Instr::Perform {
            dst,
            op_id,
            dicts: tokens.take_all(dicts),
            args: tokens.take_all(args),
        }
    }

    pub(super) fn translate_scope_run(body: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::ScopeRun {
            dst,
            body: tokens.take(body),
        }
    }

    pub(super) fn translate_scope_current(unit: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::ScopeCurrent {
            dst,
            unit: tokens.take(unit),
        }
    }

    pub(super) fn translate_scope_add_finalizer(
        task: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::ScopeAddFinalizer {
            dst,
            task: tokens.take(task),
        }
    }

    pub(super) fn translate_scope_add_finalizer_exit(
        func: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::ScopeAddFinalizerExit {
            dst,
            func: tokens.take(func),
        }
    }

    pub(super) fn translate_scope_add_close(func: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::ScopeAddClose {
            dst,
            func: tokens.take(func),
        }
    }

    pub(super) fn translate_acquire_release(
        acquire: Local,
        release: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::AcquireRelease {
            dst,
            acquire: tokens.take(acquire),
            release: tokens.take(release),
        }
    }

    pub(super) fn translate_bracket(
        acquire: Local,
        use_fn: Local,
        release: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Bracket {
            dst,
            acquire: tokens.take(acquire),
            use_fn: tokens.take(use_fn),
            release: tokens.take(release),
        }
    }

    pub(super) fn translate_uninterruptible(task: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::Uninterruptible {
            dst,
            task: tokens.take(task),
        }
    }
}
