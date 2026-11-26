impl Lower<'_> {
    fn try_fallback_boundary_call(
        &mut self,
        expr: &Expr,
        env: &Env,
        block: &mut Code,
        borrow_return: bool,
    ) -> Result<Option<Local>, LowerError> {
        if self.aggregates.fallback_targets.is_empty() && self.higher_rank_locals.is_empty() {
            return Ok(None);
        }
        let mut args = Vec::new();
        let mut head = expr;
        while let ExprKind::App { func, arg, .. } = &head.kind {
            args.push(arg.as_ref());
            head = func;
        }
        if args.is_empty() {
            return Ok(None);
        }
        args.reverse();
        let target = match &head.kind {
            ExprKind::Global { module, name, .. } => {
                self.aggregates.fallback_target(module, name).cloned()
            }
            ExprKind::Dicts(inner, _) => match &inner.kind {
                ExprKind::Global { module, name, .. } => {
                    self.aggregates.fallback_target(module, name).cloned()
                }
                ExprKind::Local(name) => self.higher_rank_locals.get(name).cloned(),
                _ => None,
            },
            ExprKind::Local(name) => self.higher_rank_locals.get(name).cloned(),
            _ => None,
        };
        let Some(target) = target else {
            return Ok(None);
        };
        if args.len() != target.poly_params.len() {
            return Ok(None);
        }
        let any_arg_boxes = args.iter().enumerate().any(|(idx, arg)| {
            target.poly_params.get(idx).copied().unwrap_or(false)
                && arg
                    .ty
                    .as_ref()
                    .is_some_and(|ty| self.aggregates.type_is_aggregate(ty))
        });
        let unbox_return = target.poly_return
            && expr
                .ty
                .as_ref()
                .is_some_and(|ty| self.aggregates.type_is_aggregate(ty));
        if !any_arg_boxes && !unbox_return {
            return Ok(None);
        }
        let result = self.lower_fallback_app_spine(expr, args.len(), &target, env, block)?;
        if unbox_return {
            let op = if borrow_return {
                Operation::UnboxBorrow { object: result }
            } else {
                Operation::UnboxConsume { object: result }
            };
            return Ok(Some(block.emit(op, expr.span)?));
        }
        Ok(Some(result))
    }

    fn lower_fallback_app_spine(
        &mut self,
        expr: &Expr,
        remaining_args: usize,
        target: &FallbackTarget,
        env: &Env,
        block: &mut Code,
    ) -> Result<Local, LowerError> {
        if remaining_args == 0 {
            return self.expr(expr, env, block);
        }
        let ExprKind::App {
            func,
            arg,
            arg_first,
            ..
        } = &expr.kind
        else {
            unreachable!("verified fallback application spine");
        };
        let arg_index = remaining_args - 1;
        let box_arg = target.poly_params.get(arg_index).copied().unwrap_or(false)
            && arg
                .ty
                .as_ref()
                .is_some_and(|ty| self.aggregates.type_is_aggregate(ty));
        let (function, argument) = if *arg_first {
            let mut argument = self.expr(arg, env, block)?;
            if box_arg {
                let layout = arg
                    .ty
                    .as_ref()
                    .and_then(|ty| self.aggregates.cell_layout_for_type(ty));
                argument = block.emit(
                    Operation::BoxAggregate {
                        value: argument,
                        layout,
                    },
                    arg.span,
                )?;
            }
            let function = self.lower_fallback_app_spine(func, arg_index, target, env, block)?;
            (function, argument)
        } else {
            let function = self.lower_fallback_app_spine(func, arg_index, target, env, block)?;
            let mut argument = self.expr(arg, env, block)?;
            if box_arg {
                let layout = arg
                    .ty
                    .as_ref()
                    .and_then(|ty| self.aggregates.cell_layout_for_type(ty));
                argument = block.emit(
                    Operation::BoxAggregate {
                        value: argument,
                        layout,
                    },
                    arg.span,
                )?;
            }
            (function, argument)
        };
        block.emit(Operation::Apply { function, argument }, expr.span)
    }

    fn saturated_constructor_expr(
        &mut self,
        con: &Constructor,
        expr: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let mut dicts = Vec::with_capacity(con.dicts);
        let mut args = Vec::with_capacity(con.arity);
        let box_gadt_fields = con.dicts > 0 && !self.current_in_fallback;
        self.eval_con_spine(expr, env, block, &mut dicts, &mut args, box_gadt_fields)?;
        dicts.extend(args);
        self.constructor(con, dicts, expr.ty.as_ref(), block, span)
    }

    fn eval_con_spine(
        &mut self,
        expr: &Expr,
        env: &Env,
        block: &mut Code,
        dicts_out: &mut Vec<Local>,
        args_out: &mut Vec<Local>,
        box_gadt_fields: bool,
    ) -> Result<(), LowerError> {
        match &expr.kind {
            ExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                if *arg_first {
                    let mut value = self.expr(arg, env, block)?;
                    if box_gadt_fields
                        && let Some(arg_ty) = &arg.ty
                        && self.aggregates.type_is_aggregate(arg_ty)
                    {
                        let layout = self.aggregates.cell_layout_for_type(arg_ty);
                        value = block.emit(Operation::BoxAggregate { value, layout }, arg.span)?;
                    }
                    self.eval_con_spine(func, env, block, dicts_out, args_out, box_gadt_fields)?;
                    args_out.push(value);
                } else {
                    self.eval_con_spine(func, env, block, dicts_out, args_out, box_gadt_fields)?;
                    let mut value = self.expr(arg, env, block)?;
                    if box_gadt_fields
                        && let Some(arg_ty) = &arg.ty
                        && self.aggregates.type_is_aggregate(arg_ty)
                    {
                        let layout = self.aggregates.cell_layout_for_type(arg_ty);
                        value = block.emit(Operation::BoxAggregate { value, layout }, arg.span)?;
                    }
                    args_out.push(value);
                }
                Ok(())
            }
            ExprKind::Dicts(_, dictionaries) => {
                for dict in dictionaries {
                    dicts_out.push(self.dictionary(dict, env, block, expr.span)?);
                }
                Ok(())
            }
            ExprKind::Con(_) => Ok(()),
            _ => unreachable!("saturated_constructor verified spine"),
        }
    }

    pub(super) fn expr_tail(
        &mut self,
        expr: &Expr,
        env: &Env,
        mut block: Code,
    ) -> Result<Block, LowerError> {
        match &expr.kind {
            ExprKind::If(condition, yes, no) => {
                let condition = self.expr(condition, env, &mut block)?;
                let then_ = self.expr_tail(yes, env, block.successor())?;
                let else_ = self.expr_tail(no, env, block.successor())?;
                Ok(Block {
                    instructions: block.instructions,
                    exit: Exit::Branch {
                        condition,
                        then_: Box::new(then_),
                        else_: Box::new(else_),
                    },
                })
            }
            ExprKind::Block(statements, result) => {
                let env = self.block_statements(statements, env, &mut block)?;
                self.expr_tail(result, &env, block)
            }
            ExprKind::Lift(inner, _) => self.expr_tail(inner, env, block),
            _ => {
                let value = self.expr(expr, env, &mut block)?;
                Ok(block.finish(value).body)
            }
        }
    }
}

fn saturated_constructor(mut expr: &Expr) -> Option<&Constructor> {
    let mut args = 0usize;
    while let ExprKind::App { func, .. } = &expr.kind {
        args += 1;
        expr = func;
    }
    match &expr.kind {
        ExprKind::Con(con) if args > 0 && con.dicts == 0 && args == con.arity => Some(con),
        ExprKind::Dicts(inner, dicts) => match &inner.kind {
            ExprKind::Con(con) if dicts.len() == con.dicts && args == con.arity => Some(con),
            _ => None,
        },
        _ => None,
    }
}
