//! Per-operation translators for tasks, generators, sequences, owners, arenas,
//! refs, lazy cells, layers, and cooperative async intrinsics.
//!
//! Kept as dedicated `translate_*` functions so formal per-operation refinement
//! proofs (`#980`) map 1-to-1 onto each memory-IR operation.

use flick_core::ir::memory::{AsyncIntrinsic, CodeId, LayerIntrinsic, Local};

use super::opcode::Instr;
use super::prepare::{Lowering, Tokens};

impl Lowering<'_> {
    pub(super) fn translate_task(
        &mut self,
        fn_code: CodeId,
        captures: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::Task {
            dst,
            code: self.codes.intern(fn_code),
            captures: tokens.take_all(captures),
        }
    }

    pub(super) fn translate_await(task: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::Await {
            dst,
            task: tokens.take(task),
        }
    }

    pub(super) fn translate_within(
        &mut self,
        owner: Local,
        fn_code: CodeId,
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        let owner = tokens.take(owner);
        Instr::Within {
            dst,
            owner,
            code: self.codes.intern(fn_code),
            args: tokens.take_all(args),
        }
    }

    pub(super) fn translate_with_owner_evidence(
        &mut self,
        owner: Local,
        fn_code: CodeId,
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        let owner = tokens.take(owner);
        Instr::WithOwnerEvidence {
            dst,
            owner,
            code: self.codes.intern(fn_code),
            args: tokens.take_all(args),
        }
    }

    pub(super) fn translate_generate_strict(
        &mut self,
        fn_code: CodeId,
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::GenerateStrict {
            dst,
            code: self.codes.intern(fn_code),
            args: tokens.take_all(args),
        }
    }

    pub(super) fn translate_generate_lazy(
        &mut self,
        fn_code: CodeId,
        captures: &[Local],
        limit: Option<Local>,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        let captures = tokens.take_all(captures);
        let limit = limit.map(|local| tokens.take(local));
        Instr::GenerateLazy {
            dst,
            code: self.codes.intern(fn_code),
            captures,
            limit,
        }
    }

    pub(super) fn translate_yield(value: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::Yield {
            dst,
            value: tokens.take(value),
        }
    }

    pub(super) fn translate_seq_open(source: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::SeqOpen {
            dst,
            source: tokens.take(source),
        }
    }

    pub(super) fn translate_seq_next(iterator: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::SeqNext {
            dst,
            iterator: tokens.take(iterator),
        }
    }

    pub(super) fn translate_seq_of_list(list: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::SeqOfList {
            dst,
            list: tokens.take(list),
        }
    }

    pub(super) fn translate_seq_to_list(seq: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::SeqToList {
            dst,
            seq: tokens.take(seq),
        }
    }

    pub(super) fn translate_ref_make(src: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::RefMake {
            dst,
            src: tokens.take(src),
        }
    }

    pub(super) fn translate_ref_get(cell: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::RefGet {
            dst,
            cell: tokens.take(cell),
        }
    }

    pub(super) fn translate_ref_set(
        cell: Local,
        value: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::RefSet {
            dst,
            cell: tokens.take(cell),
            value: tokens.take(value),
        }
    }

    pub(super) fn translate_ref_modify(
        cell: Local,
        func: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::RefModify {
            dst,
            cell: tokens.take(cell),
            func: tokens.take(func),
        }
    }

    pub(super) fn translate_ref_same(
        left: Local,
        right: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::RefSame {
            dst,
            left: tokens.take(left),
            right: tokens.take(right),
        }
    }

    pub(super) fn translate_arena_make(unit: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::ArenaMake {
            dst,
            unit: tokens.take(unit),
        }
    }

    pub(super) fn translate_arena_alloc(
        arena: Local,
        value: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::ArenaAlloc {
            dst,
            arena: tokens.take(arena),
            value: tokens.take(value),
        }
    }

    pub(super) fn translate_arena_get(view: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::ArenaGet {
            dst,
            view: tokens.take(view),
        }
    }

    pub(super) fn translate_lazy_make(
        task: Local,
        local: bool,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::LazyMake {
            dst,
            task: tokens.take(task),
            local,
        }
    }

    pub(super) fn translate_lazy_force(lazy: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::LazyForce {
            dst,
            lazy: tokens.take(lazy),
        }
    }

    pub(super) fn translate_effect_exit(task: Local, dst: usize, tokens: &mut Tokens) -> Instr {
        Instr::EffectExit {
            dst,
            task: tokens.take(task),
        }
    }

    pub(super) fn translate_effect_catch_defect(
        handler: Local,
        task: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::EffectCatchDefect {
            dst,
            handler: tokens.take(handler),
            task: tokens.take(task),
        }
    }

    pub(super) fn translate_layer_succeed(
        tag: Local,
        implementation: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::LayerSucceed {
            dst,
            tag: tokens.take(tag),
            implementation: tokens.take(implementation),
        }
    }

    pub(super) fn translate_layer_merge(
        left: Local,
        right: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::LayerMerge {
            dst,
            left: tokens.take(left),
            right: tokens.take(right),
        }
    }

    pub(super) fn translate_layer_run(
        layer: Local,
        body: Local,
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::LayerRun {
            dst,
            layer: tokens.take(layer),
            body: tokens.take(body),
        }
    }

    pub(super) fn translate_async_op(
        op: AsyncIntrinsic,
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::AsyncOp {
            dst,
            op,
            args: tokens.take_all(args),
        }
    }

    pub(super) fn translate_layer_op(
        op: LayerIntrinsic,
        args: &[Local],
        dst: usize,
        tokens: &mut Tokens,
    ) -> Instr {
        Instr::LayerOp {
            dst,
            op,
            args: tokens.take_all(args),
        }
    }
}
