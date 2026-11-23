impl Lower<'_> {
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
        self.eval_con_spine(expr, env, block, &mut dicts, &mut args)?;
        dicts.extend(args);
        self.constructor(con, dicts, block, span)
    }

    fn eval_con_spine(
        &mut self,
        expr: &Expr,
        env: &Env,
        block: &mut Code,
        dicts_out: &mut Vec<Local>,
        args_out: &mut Vec<Local>,
    ) -> Result<(), LowerError> {
        match &expr.kind {
            ExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                if *arg_first {
                    let value = self.expr(arg, env, block)?;
                    self.eval_con_spine(func, env, block, dicts_out, args_out)?;
                    args_out.push(value);
                } else {
                    self.eval_con_spine(func, env, block, dicts_out, args_out)?;
                    let value = self.expr(arg, env, block)?;
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
