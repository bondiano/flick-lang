//! Arm suffixes are direct helpers, not copies of a source continuation.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::Arm;

use super::{
    Block, CandidateShape, Code, CodeId, Env, Exit, Expr, Function, Intrinsic, Local, Lower,
    LowerError, Operation, Param, Pat, Span, Stage, ValueRep, free, lookup, memory, reuse,
};

#[derive(Clone, Copy)]
pub(super) enum MatchMode {
    Ordinary,
    RecoverySelection,
}

struct Suffix {
    code: CodeId,
    names: Vec<String>,
    mode: MatchMode,
}

impl Suffix {
    fn call(
        &self,
        scrutinee: Local,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let mut args = vec![scrutinee];
        args.extend(
            self.names
                .iter()
                .map(|name| lookup(env, name, span))
                .collect::<Result<Vec<_>, _>>()?,
        );
        block.emit(
            Operation::Call {
                code: self.code,
                args,
            },
            span,
        )
    }
}

impl Lower<'_> {
    pub(super) fn matching(
        &mut self,
        value: &Expr,
        arms: &[Arm],
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let scrutinee = self.expr(value, env, block)?;
        self.matching_local(scrutinee, arms, env, block, span, MatchMode::Ordinary)
    }

    pub(super) fn matching_local(
        &mut self,
        scrutinee: Local,
        arms: &[Arm],
        env: &Env,
        block: &mut Code,
        span: Span,
        mode: MatchMode,
    ) -> Result<Local, LowerError> {
        let code = self.reserve(span)?;
        let mut terminal = Code::new(vec![0]);
        let operation = match mode {
            MatchMode::Ordinary => Operation::Intrinsic {
                kind: Intrinsic::MatchFail,
                args: vec![],
            },
            MatchMode::RecoverySelection => Operation::RecoveryResult { selected: None },
        };
        let failed = terminal.emit(operation, span)?;
        self.finish(code, &terminal.finish(failed))?;
        let mut suffix = Suffix {
            code,
            names: vec![],
            mode,
        };
        let scrutinee_rep = if arms
            .iter()
            .any(|arm| self.aggregates.pattern_is_aggregate(&arm.pat))
        {
            ValueRep::Aggregate
        } else {
            ValueRep::Cell
        };
        let scrutinee_shape = if scrutinee_rep.is_aggregate() {
            CandidateShape::Never
        } else {
            reuse::arms_scrutinee_shape(arms.iter().map(|arm| &arm.pat))
        };
        for arm in arms.iter().rev() {
            let bindings = memory::pattern_bindings(&arm.pat)
                .map_err(|error| LowerError::Pattern { error, span })?;
            let mut needed = free(&arm.body);
            if let Some(guard) = &arm.guard {
                needed.extend(free(guard));
            }
            for name in &bindings {
                needed.remove(name);
            }
            needed.extend(suffix.names.iter().cloned());
            suffix = self.match_arm(
                arm,
                needed,
                &bindings,
                &suffix,
                scrutinee_shape,
                scrutinee_rep,
                span,
            )?;
        }
        suffix.call(scrutinee, env, block, span)
    }

    #[allow(clippy::too_many_arguments)]
    fn match_arm(
        &mut self,
        arm: &Arm,
        needed: BTreeSet<String>,
        bindings: &[String],
        suffix: &Suffix,
        scrutinee_shape: CandidateShape,
        scrutinee_rep: ValueRep,
        span: Span,
    ) -> Result<Suffix, LowerError> {
        let names: Vec<_> = needed.into_iter().collect();
        let params = (0..=names.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let outer: Env = names
            .iter()
            .cloned()
            .zip(params.iter().copied().skip(1))
            .collect();
        let mut block = Code::new(params.clone());
        let matched = block.emit(
            Operation::Match {
                object: 0,
                pattern: Box::new(arm.pat.clone()),
                rep: scrutinee_rep,
            },
            span,
        )?;
        let condition = block.emit(
            Operation::Project {
                object: matched,
                field: 0,
                rep: ValueRep::Cell,
            },
            span,
        )?;
        let mut success = block.successor();
        let mut inside = outer.clone();
        for (index, name) in bindings.iter().enumerate() {
            let field = index.checked_add(1).ok_or(LowerError::Limit(span))?;
            let value = success.emit(
                Operation::Project {
                    object: matched,
                    field,
                    rep: ValueRep::Cell,
                },
                span,
            )?;
            inside.insert(name.clone(), value);
        }
        let yes = self.guarded_arm(arm, &inside, &outer, success, suffix, span)?;
        let mut no = block.successor();
        let fallback = suffix.call(0, &outer, &mut no, span)?;
        let code = self.reserve(span)?;
        let mut hints = BTreeMap::new();
        if scrutinee_shape != CandidateShape::Unknown {
            hints.insert(0, scrutinee_shape);
        }
        self.finish_with_hints(
            code,
            &Function {
                params,
                body: Block {
                    instructions: block.instructions,
                    exit: Exit::Branch {
                        condition,
                        then_: Box::new(yes),
                        else_: Box::new(no.finish(fallback).body),
                    },
                },
            },
            &hints,
        )?;
        Ok(Suffix {
            code,
            names,
            mode: suffix.mode,
        })
    }

    fn guarded_arm(
        &mut self,
        arm: &Arm,
        inside: &Env,
        outer: &Env,
        mut block: Code,
        suffix: &Suffix,
        span: Span,
    ) -> Result<Block, LowerError> {
        let Some(guard) = &arm.guard else {
            return self.matched_body_block(arm, inside, block, suffix.mode, span);
        };
        let condition = self.expr(guard, inside, &mut block)?;
        let yes = self.matched_body_block(arm, inside, block.successor(), suffix.mode, span)?;
        let mut no = block.successor();
        // The successful pattern may shadow an outer name. A false guard
        // resumes the suffix with the original environment, not its bindings.
        let fallback = suffix.call(0, outer, &mut no, span)?;
        Ok(Block {
            instructions: block.instructions,
            exit: Exit::Branch {
                condition,
                then_: Box::new(yes),
                else_: Box::new(no.finish(fallback).body),
            },
        })
    }
    fn matched_body_block(
        &mut self,
        arm: &Arm,
        inside: &Env,
        mut block: Code,
        mode: MatchMode,
        span: Span,
    ) -> Result<Block, LowerError> {
        match mode {
            MatchMode::Ordinary => self.expr_tail(&arm.body, inside, block),
            MatchMode::RecoverySelection => {
                let value = self.recovery_body(arm, inside, &mut block, span)?;
                Ok(block.finish(value).body)
            }
        }
    }

    #[inline(never)]
    fn recovery_body(
        &mut self,
        arm: &Arm,
        inside: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let selected = self.lambda(
            &[Param {
                pat: Pat::Wild,
                stage: Stage::Pure,
                ty: None,
                binder_types: Vec::new(),
            }],
            &arm.body,
            inside,
            None,
            block,
            span,
        )?;
        block.emit(
            Operation::RecoveryResult {
                selected: Some(selected),
            },
            span,
        )
    }
}
