//! Loops, labeled control exits, nonlocal `Option?`, and strict/lazy comprehensions.

use std::collections::BTreeSet;

use flick_ir::{Constructor, Rep};

use super::{
    Block, Code, Env, Exit, Expr, ExprKind, Function, Intrinsic, Lit, Local, Lower, LowerError,
    Operation, Pat, Span, ValueRep, free, lookup, memory,
};

pub(super) fn contains_direct_unwrap(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Unwrap(_) => true,
        ExprKind::Lambda { .. }
        | ExprKind::Abstract(..)
        | ExprKind::Task(_)
        | ExprKind::Generate { strict: false, .. } => false,
        _ => {
            let mut found = false;
            expr.each_child(&mut |child| {
                if !found && contains_direct_unwrap(child) {
                    found = true;
                }
            });
            found
        }
    }
}

impl Lower<'_> {
    pub(super) fn fresh_loop_id(&mut self) -> String {
        let id = self.loop_counter;
        self.loop_counter += 1;
        format!("$loop#{id}")
    }

    pub(super) fn function_boundary(
        &mut self,
        body: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let names: Vec<_> = free(body).into_iter().collect();
        let args = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<Vec<_>, _>>()?;
        let params = (0..args.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let inside = names.into_iter().zip(params.iter().copied()).collect();
        let saved = self.active_for_loop.take();
        let mut helper = Code::new(params);
        let result = self.expr(body, &inside, &mut helper);
        self.active_for_loop = saved;
        let result = result?;
        let code = self.reserve(span)?;
        self.finish(code, &helper.finish(result))?;
        block.emit(Operation::FunctionBoundary { code, args }, span)
    }

    pub(super) fn loop_expr(
        &mut self,
        condition: &Expr,
        body: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let id = self.fresh_loop_id();
        self.control_loop(&id, Some(condition), body, false, env, block, span)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn control_loop(
        &mut self,
        id: &str,
        condition: Option<&Expr>,
        body: &Expr,
        iterable: bool,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let mut needed = free(body);
        if let Some(cond) = condition {
            needed.extend(free(cond));
        }
        let names: Vec<_> = needed.into_iter().collect();
        let args = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<Vec<_>, _>>()?;
        let params = (0..args.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let inside = names.into_iter().zip(params.iter().copied()).collect();

        let saved_for = self.active_for_loop.take();
        let prev_iterable = if iterable {
            let cont_id = format!("{id}$continue");
            self.active_for_loop = Some(id.to_owned());
            self.iterable_loops.insert(id.to_owned(), cont_id)
        } else {
            None
        };
        let had_in_each = self.in_for_each_body.remove(id);

        let lowered = (|| -> Result<Function, LowerError> {
            let mut helper = Code::new(params.clone());
            if let Some(cond) = condition {
                let cond_local = self.expr(cond, &inside, &mut helper)?;
                let mut then_ = helper.successor();
                let then_res = self.expr(body, &inside, &mut then_)?;
                let mut else_ = helper.successor();
                let unit = else_.emit(Operation::Literal(Lit::Unit), span)?;
                let exit = else_.emit(
                    Operation::LoopExit {
                        id: id.to_owned(),
                        value: Some(unit),
                    },
                    span,
                )?;
                Ok(Function {
                    params,
                    body: Block {
                        instructions: helper.instructions,
                        exit: Exit::Branch {
                            condition: cond_local,
                            then_: Box::new(then_.finish(then_res).body),
                            else_: Box::new(else_.finish(exit).body),
                        },
                    },
                })
            } else if iterable {
                let _ = self.expr(body, &inside, &mut helper)?;
                let unit = helper.emit(Operation::Literal(Lit::Unit), span)?;
                let exit = helper.emit(
                    Operation::LoopExit {
                        id: id.to_owned(),
                        value: Some(unit),
                    },
                    span,
                )?;
                Ok(helper.finish(exit))
            } else {
                let res = self.expr(body, &inside, &mut helper)?;
                Ok(helper.finish(res))
            }
        })();

        self.active_for_loop = saved_for;
        if had_in_each {
            self.in_for_each_body.insert(id.to_owned());
        } else {
            self.in_for_each_body.remove(id);
        }
        if iterable {
            if let Some(prev) = prev_iterable {
                self.iterable_loops.insert(id.to_owned(), prev);
            } else {
                self.iterable_loops.remove(id);
            }
        }

        let function = lowered?;
        let code = self.reserve(span)?;
        self.finish(code, &function)?;
        block.emit(
            Operation::Loop {
                id: id.to_owned(),
                code,
                args,
            },
            span,
        )
    }

    pub(super) fn loop_exit(
        &mut self,
        id: &str,
        value: Option<&Expr>,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let value_local = value.map(|expr| self.expr(expr, env, block)).transpose()?;
        if value_local.is_none()
            && let Some(cont_id) = self.iterable_loops.get(id).cloned()
        {
            return if self.in_for_each_body.contains(id) {
                block.emit(
                    Operation::LoopExit {
                        id: cont_id,
                        value: None,
                    },
                    span,
                )
            } else {
                // `continue` executed while evaluating the outermost `for` source
                // before any `Each` loop is active exits the enclosing `for` loop.
                let unit = block.emit(Operation::Literal(Lit::Unit), span)?;
                block.emit(
                    Operation::LoopExit {
                        id: id.to_owned(),
                        value: Some(unit),
                    },
                    span,
                )
            };
        }
        block.emit(
            Operation::LoopExit {
                id: id.to_owned(),
                value: value_local,
            },
            span,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn generate(
        &mut self,
        body: &Expr,
        strict: bool,
        sync: bool,
        limit: Option<&Expr>,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let saved_for = self.active_for_loop.take();
        let names: Vec<_> = free(body).into_iter().collect();
        if strict && limit.is_none() {
            let args = names
                .iter()
                .map(|name| lookup(env, name, span))
                .collect::<Result<Vec<_>, _>>()?;
            let params = (0..args.len())
                .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
                .collect::<Result<Vec<_>, _>>()?;
            let inside = names.into_iter().zip(params.iter().copied()).collect();
            let mut helper = Code::new(params);
            let res = self.expr(body, &inside, &mut helper);
            self.active_for_loop = saved_for;
            let res = res?;
            let code = self.reserve(span)?;
            self.finish(code, &helper.finish(res))?;
            return block.emit(Operation::GenerateStrict { code, args }, span);
        }
        let captures = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<Vec<_>, _>>()?;
        let limit_local = limit.map(|expr| self.expr(expr, env, block)).transpose()?;
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Env::new();
        for (field, name) in names.into_iter().enumerate() {
            let value = function.emit(
                Operation::Project {
                    object: 0,
                    field,
                    rep: ValueRep::Cell,
                },
                span,
            )?;
            inside.insert(name, value);
        }
        let res = self.expr(body, &inside, &mut function);
        self.active_for_loop = saved_for;
        let res = res?;
        self.finish(code, &function.finish(res))?;
        let seq = block.emit(
            Operation::GenerateLazy {
                code,
                captures,
                limit: limit_local,
                sync,
            },
            span,
        )?;
        if strict {
            block.emit(
                Operation::Intrinsic {
                    kind: Intrinsic::SeqToList,
                    args: vec![seq],
                },
                span,
            )
        } else {
            Ok(seq)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn each(
        &mut self,
        stop: Option<&Expr>,
        pat: &Pat,
        source: &Expr,
        body: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let source_local = self.expr(source, env, block)?;
        let iter_local = block.emit(Operation::SeqOpen(source_local), span)?;
        let owning_for = self.active_for_loop.clone();
        let loop_id = owning_for
            .as_ref()
            .and_then(|for_id| self.iterable_loops.get(for_id).cloned())
            .unwrap_or_else(|| self.fresh_loop_id());

        let bindings =
            memory::pattern_bindings(pat).map_err(|error| LowerError::Pattern { error, span })?;
        let mut needed: BTreeSet<String> = free(body);
        for name in &bindings {
            needed.remove(name);
        }
        if let Some(stop_expr) = stop {
            needed.extend(free(stop_expr));
        }
        let names: Vec<_> = needed.into_iter().collect();
        let mut args = vec![iter_local];
        for name in &names {
            args.push(lookup(env, name, span)?);
        }
        let params = (0..args.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let inside: Env = names
            .into_iter()
            .zip(params.iter().copied().skip(1))
            .collect();

        let helper_body = if let Some(stop_expr) = stop {
            let mut helper = Code::new(params.clone());
            let stop_cond = self.expr(stop_expr, &inside, &mut helper)?;
            let mut on_stop = helper.successor();
            let unit = on_stop.emit(Operation::Literal(Lit::Unit), span)?;
            let exit = on_stop.emit(
                Operation::LoopExit {
                    id: loop_id.clone(),
                    value: Some(unit),
                },
                span,
            )?;
            let on_continue = self.each_step_block(
                helper.successor(),
                pat,
                &bindings,
                body,
                &inside,
                owning_for.as_deref(),
                &loop_id,
                span,
            )?;
            Block {
                instructions: helper.instructions,
                exit: Exit::Branch {
                    condition: stop_cond,
                    then_: Box::new(on_stop.finish(exit).body),
                    else_: Box::new(on_continue),
                },
            }
        } else {
            self.each_step_block(
                Code::new(params.clone()),
                pat,
                &bindings,
                body,
                &inside,
                owning_for.as_deref(),
                &loop_id,
                span,
            )?
        };

        let code = self.reserve(span)?;
        self.finish(
            code,
            &Function {
                params,
                body: helper_body,
            },
        )?;
        block.emit(
            Operation::Loop {
                id: loop_id,
                code,
                args,
            },
            span,
        )
    }

    fn emit_match_and_flag(
        code: &mut Code,
        object: Local,
        pattern: Pat,
        rep: ValueRep,
        span: Span,
    ) -> Result<(Local, Local), LowerError> {
        let matched = code.emit(
            Operation::Match {
                object,
                pattern: Box::new(pattern),
                rep,
            },
            span,
        )?;
        let flag = code.emit(
            Operation::Project {
                object: matched,
                field: 0,
                rep: ValueRep::Cell,
            },
            span,
        )?;
        Ok((matched, flag))
    }

    #[allow(clippy::too_many_arguments)]
    fn each_step_block(
        &mut self,
        mut step: Code,
        pat: &Pat,
        bindings: &[String],
        body: &Expr,
        inside: &Env,
        owning_for: Option<&str>,
        loop_id: &str,
        span: Span,
    ) -> Result<Block, LowerError> {
        let next = step.emit(Operation::SeqNext(0), span)?;
        let some_pat = Pat::Con(
            Constructor {
                module: "Prelude".into(),
                ty: "Option".into(),
                name: "Some".into(),
                arity: 1,
                dicts: 0,
                rep: Rep::Tagged,
                fields: Vec::new(),
                type_args: Vec::new(),
            },
            vec![Pat::Bind("$each.elem".into())],
        );
        let (matched, is_some) =
            Self::emit_match_and_flag(&mut step, next, some_pat, ValueRep::Cell, span)?;

        let mut on_some = step.successor();
        let elem = on_some.emit(
            Operation::Project {
                object: matched,
                field: 1,
                rep: ValueRep::Cell,
            },
            span,
        )?;
        let elem_rep = if self.aggregates.pattern_is_aggregate(pat) {
            ValueRep::Aggregate
        } else {
            ValueRep::Cell
        };
        let (pat_matched, pat_ok) =
            Self::emit_match_and_flag(&mut on_some, elem, pat.clone(), elem_rep, span)?;
        let mut on_pat_ok = on_some.successor();
        let mut body_env = inside.clone();
        for (idx, name) in bindings.iter().enumerate() {
            let field = idx.checked_add(1).ok_or(LowerError::Limit(span))?;
            let bound = on_pat_ok.emit(
                Operation::Project {
                    object: pat_matched,
                    field,
                    rep: ValueRep::Cell,
                },
                span,
            )?;
            body_env.insert(name.clone(), bound);
        }
        let prev_in_each = owning_for.map(|for_id| self.in_for_each_body.insert(for_id.to_owned()));
        let body_res = self.expr(body, &body_env, &mut on_pat_ok);
        if let (Some(for_id), Some(true)) = (owning_for, prev_in_each) {
            self.in_for_each_body.remove(for_id);
        }
        let body_res = body_res?;
        let mut on_pat_skip = on_some.successor();
        let skip_unit = on_pat_skip.emit(Operation::Literal(Lit::Unit), span)?;

        let on_some_block = Block {
            instructions: on_some.instructions,
            exit: Exit::Branch {
                condition: pat_ok,
                then_: Box::new(on_pat_ok.finish(body_res).body),
                else_: Box::new(on_pat_skip.finish(skip_unit).body),
            },
        };

        let mut on_none = step.successor();
        let unit = on_none.emit(Operation::Literal(Lit::Unit), span)?;
        let exit = on_none.emit(
            Operation::LoopExit {
                id: loop_id.to_owned(),
                value: Some(unit),
            },
            span,
        )?;

        Ok(Block {
            instructions: step.instructions,
            exit: Exit::Branch {
                condition: is_some,
                then_: Box::new(on_some_block),
                else_: Box::new(on_none.finish(exit).body),
            },
        })
    }

    pub(super) fn yield_expr(
        &mut self,
        value: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let local = self.expr(value, env, block)?;
        block.emit(Operation::Yield(local), span)
    }
}
