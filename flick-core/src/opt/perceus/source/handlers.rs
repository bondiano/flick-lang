//! Delayed handler bodies and operation saturation through unary owned code.
use flick_ir::{Handler, operation_arity};

use super::{
    Code, Env, Local, Lower, LowerError, Operation, Param, Pat, Span, Stage, ValueRep, memory,
};

impl Lower<'_> {
    pub(super) fn handler(
        &mut self,
        handler: &Handler,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let mut effects = Vec::new();
        for effect in &handler.effects {
            let mut operations = Vec::new();
            for op in &effect.operations {
                operations.push(memory::HandlerOperation {
                    name: op.name.clone(),
                    class: op.class,
                    dicts: op.dicts,
                    arity: op.arity,
                    function: self.expr(&op.function, env, block)?,
                });
            }
            effects.push(memory::HandlerEffect {
                effect: effect.effect.clone(),
                operations,
            });
        }
        let body = self.lambda(
            &[Param {
                pat: Pat::Wild,
                stage: Stage::Pure,
                ty: None,
                binder_types: Vec::new(),
            }],
            &handler.body,
            env,
            None,
            block,
            span,
        )?;
        let ret = handler
            .ret
            .as_ref()
            .map(|ret| self.expr(ret, env, block))
            .transpose()?;
        let errors = if handler.errors.is_empty() {
            None
        } else {
            Some(self.failure_handler(&handler.errors, env, block, span)?)
        };
        block.emit(
            Operation::Handle {
                body,
                effects,
                ret,
                errors,
            },
            span,
        )
    }

    pub(super) fn perform(
        &mut self,
        effect: &str,
        op: &str,
        stages: &[Stage],
        dicts: Vec<Local>,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let arity = operation_arity(stages);
        if arity == 0 {
            return block.emit(
                Operation::Perform {
                    effect: effect.into(),
                    op: op.into(),
                    dicts,
                    args: vec![],
                },
                span,
            );
        }
        let codes = (0..arity)
            .map(|_| self.reserve(span))
            .collect::<Result<Vec<_>, _>>()?;
        for (stage, &code) in codes.iter().enumerate() {
            let count = dicts
                .len()
                .checked_add(stage)
                .ok_or(LowerError::Limit(span))?;
            let mut function = Code::new(vec![0, 1]);
            let mut captured = (0..count)
                .map(|field| {
                    function.emit(
                        Operation::Project {
                            object: 0,
                            field,
                            rep: ValueRep::Cell,
                        },
                        span,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            captured.push(1);
            let value = if let Some(&next) = codes.get(stage + 1) {
                function.emit(
                    Operation::Closure {
                        code: next,
                        captures: captured,
                    },
                    span,
                )?
            } else {
                let args = captured.split_off(dicts.len());
                function.emit(
                    Operation::Perform {
                        effect: effect.into(),
                        op: op.into(),
                        dicts: captured,
                        args,
                    },
                    span,
                )?
            };
            self.finish(code, &function.finish(value))?;
        }
        block.emit(
            Operation::Closure {
                code: codes[0],
                captures: dicts,
            },
            span,
        )
    }
}
