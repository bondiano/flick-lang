//! Audited await, Lazy, Scope finalizer, Resource, and Exit foreign adapters.

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::{Code, Env, Intrinsic, Local, Lower, LowerError, Operation, Span};

pub(super) fn has_standard_shape(value: &Foreign) -> bool {
    value.catch.is_none()
        && value.adapter.words.is_empty()
        && value.adapter.optional.is_empty()
        && value.adapter.owned.is_empty()
        && !value.adapter.result.nullable
}

impl Lower<'_> {
    pub(super) fn lazy_scope_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let name = value.binding.name.as_str();
        match (from, name, value.mode, value.dicts, value.arity) {
            ("flick:rt", "awaitEff", ForeignMode::Effect, 0, 1) => {
                let b = ExprBuilder::new(span, "$awaitEff");
                let function = b.lam(&["task"], b.await_(b.loc("task")));
                Some(self.expr(&function, &Env::new(), block))
            }
            // `Prelude.lazy_make : NoControl r => (Eff r e a)^c ->{} Lazy r e a`
            // (`NoControl` is an erased marker class with 0 runtime dictionaries.)
            ("flick:rt", "lazyMake", ForeignMode::Effect, 0, 1) => {
                Some(self.intrinsic(Intrinsic::LazyMake, false, vec![], block, span))
            }
            // `Prelude.lazy_make_local : NoControl r => (Eff r e a)^c ->{} Lazy r e a`
            ("flick:rt", "lazyMakeLocal", ForeignMode::Effect, 0, 1) => {
                Some(self.intrinsic(Intrinsic::LazyMakeLocal, false, vec![], block, span))
            }
            // `Prelude.lazy_force` / `Prelude.lazy_read`
            ("flick:rt", "lazyForce", ForeignMode::Effect, 0, 1) => {
                Some(self.intrinsic(Intrinsic::LazyForce, false, vec![], block, span))
            }
            // `Prelude.lazy_done : a ->{} Lazy r e a` -> `fun v -> LazyMake(Task(return v))`
            ("flick:rt", "lazyDone", ForeignMode::Effect, 0, 1) => {
                Some(self.lazy_done(block, span))
            }
            ("flick:Scope", "flickAddFinalizer", ForeignMode::Effect, 0, 1) => {
                Some(self.intrinsic(Intrinsic::ScopeAddFinalizer, false, vec![], block, span))
            }
            ("flick:Scope", "flickAddFinalizerExit", ForeignMode::Effect, 0, 1)
            | ("flick:Effect", "flickAddFinalizer", ForeignMode::Effect, 0, 1) => {
                Some(self.intrinsic(Intrinsic::ScopeAddFinalizerExit, false, vec![], block, span))
            }
            ("flick:Scope", "flickAddClose", ForeignMode::Effect, 0, 1) => {
                Some(self.intrinsic(Intrinsic::ScopeAddClose, false, vec![], block, span))
            }
            ("flick:rt", "acquireRelease", ForeignMode::Effect, 0, 2) => {
                Some(self.intrinsic(Intrinsic::AcquireRelease, false, vec![], block, span))
            }
            ("flick:rt", "bracket", ForeignMode::Effect, 0, 3) => {
                Some(self.intrinsic(Intrinsic::Bracket, false, vec![], block, span))
            }
            ("flick:Effect", "uninterruptible", ForeignMode::Effect, 0, 1) => {
                Some(self.intrinsic(Intrinsic::Uninterruptible, false, vec![], block, span))
            }
            ("flick:Exit", "flickIsSuccess", ForeignMode::Pure, 0, 1) => {
                Some(self.intrinsic(Intrinsic::ExitIsSuccess, false, vec![], block, span))
            }
            ("flick:Exit", "flickIsFailure", ForeignMode::Pure, 0, 1) => {
                Some(self.intrinsic(Intrinsic::ExitIsFailure, false, vec![], block, span))
            }
            ("flick:Exit", "flickHasInterrupts", ForeignMode::Pure, 0, 1) => {
                Some(self.intrinsic(Intrinsic::ExitHasInterrupts, false, vec![], block, span))
            }
            _ => None,
        }
    }

    fn lazy_done(&mut self, block: &mut Code, span: Span) -> Result<Local, LowerError> {
        let task_code = self.reserve(span)?;
        let mut task_fn = Code::new(vec![0, 1]);
        let captured = task_fn.emit(
            Operation::Project {
                object: 0,
                field: 0,
            },
            span,
        )?;
        self.finish(task_code, &task_fn.finish(captured))?;

        let outer_code = self.reserve(span)?;
        let mut outer_fn = Code::new(vec![0, 1]);
        let task = outer_fn.emit(
            Operation::Task {
                code: task_code,
                captures: vec![1],
            },
            span,
        )?;
        let lazy = outer_fn.emit(
            Operation::Intrinsic {
                kind: Intrinsic::LazyMake,
                args: vec![task],
            },
            span,
        )?;
        self.finish(outer_code, &outer_fn.finish(lazy))?;
        block.emit(
            Operation::Closure {
                code: outer_code,
                captures: vec![],
            },
            span,
        )
    }

    /// Emits a curried closure accepting `dicts + intrinsic_args` unary calls.
    /// When `keep_first_dict` is true, the first dictionary argument is passed
    /// as the first operand of `kind` (e.g. `Record.project`'s label list);
    /// all other dictionaries are dropped.
    pub(super) fn curried_intrinsic(
        &mut self,
        kind: Intrinsic,
        dicts: usize,
        keep_first_dict: bool,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let forwarded_dicts = usize::from(keep_first_dict);
        let value_arity = kind.arity().saturating_sub(forwarded_dicts);
        let total = dicts
            .checked_add(value_arity)
            .ok_or(LowerError::Limit(span))?;
        self.curried_intrinsic_stage(kind, dicts, keep_first_dict, total, vec![], block, span)
    }

    #[allow(clippy::too_many_arguments)]
    fn curried_intrinsic_stage(
        &mut self,
        kind: Intrinsic,
        dicts: usize,
        keep_first_dict: bool,
        total: usize,
        collected: Vec<Local>,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        if collected.len() == total {
            let mut args = Vec::with_capacity(kind.arity());
            if keep_first_dict && !collected.is_empty() {
                args.push(collected[0]);
            }
            args.extend_from_slice(&collected[dicts..]);
            return block.emit(Operation::Intrinsic { kind, args }, span);
        }
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Vec::with_capacity(collected.len() + 1);
        for field in 0..collected.len() {
            inside.push(function.emit(Operation::Project { object: 0, field }, span)?);
        }
        inside.push(1);
        let result = self.curried_intrinsic_stage(
            kind,
            dicts,
            keep_first_dict,
            total,
            inside,
            &mut function,
            span,
        )?;
        self.finish(code, &function.finish(result))?;
        block.emit(
            Operation::Closure {
                code,
                captures: collected,
            },
            span,
        )
    }
}
