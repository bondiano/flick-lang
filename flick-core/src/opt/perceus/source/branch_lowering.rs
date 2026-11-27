impl Lower<'_> {
    fn conditional(
        &mut self,
        condition: &Expr,
        yes: &Expr,
        no: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let condition = self.expr(condition, env, block)?;
        let mut names = free(yes);
        names.extend(free(no));
        let names: Vec<_> = names.into_iter().collect();
        let mut args = vec![condition];
        args.extend(
            names
                .iter()
                .map(|name| lookup(env, name, span))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let params: Vec<_> = (0..args.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<_, _>>()?;
        let inside = names
            .into_iter()
            .zip(params.iter().copied().skip(1))
            .collect();
        let mut then_ = Code::new(params.clone());
        let result = self.expr(yes, &inside, &mut then_)?;
        let then_ = then_.finish(result).body;
        let mut else_ = Code::new(params.clone());
        let result = self.expr(no, &inside, &mut else_)?;
        let else_ = else_.finish(result).body;
        let code = self.reserve(span)?;
        let body = Code::new(params.clone()).branch(0, then_, else_);
        self.finish(code, &SpannedFunction { params, body })?;
        block.emit(Operation::Call { code, args }, span)
    }

    fn pattern(
        &mut self,
        pattern: &Pat,
        value: Local,
        env: &mut Env,
        block: &mut Code,
        span: Span,
    ) -> Result<(), LowerError> {
        match pattern {
            Pat::Wild | Pat::Lit(Lit::Unit) => {}
            Pat::Bind(name) => {
                env.insert(name.clone(), value);
            }
            Pat::Tuple(items) => {
                let rep = if !items.is_empty() && self.aggregates.is_aggregate_tag("$tuple") {
                    ValueRep::Aggregate
                } else {
                    ValueRep::Cell
                };
                for (field, pattern) in items.iter().enumerate() {
                    let value = block.emit(
                        Operation::Project {
                            object: value,
                            field,
                            rep,
                        },
                        span,
                    )?;
                    self.pattern(pattern, value, env, block, span)?;
                }
            }
            Pat::Record(fields) => {
                for (field, pattern) in fields {
                    let value = block.emit(
                        Operation::Field {
                            object: value,
                            field: field.clone(),
                        },
                        span,
                    )?;
                    self.pattern(pattern, value, env, block, span)?;
                }
            }
            _ => {
                let names = memory::pattern_bindings(pattern)
                    .map_err(|error| LowerError::Pattern { error, span })?;
                let result = Expr::new(
                    ExprKind::Tuple(
                        names
                            .iter()
                            .map(|name| Expr::new(ExprKind::Local(name.clone()), span))
                            .collect(),
                    ),
                    span,
                );
                let arm = Arm {
                    pat: pattern.clone(),
                    binder_types: Vec::new(),
                    guard: None,
                    body: result,
                };
                let fields = self.matching_local(
                    value,
                    None,
                    &[arm],
                    &Env::new(),
                    block,
                    span,
                    matches::MatchMode::Ordinary,
                )?;
                let rep = if !names.is_empty() && self.aggregates.is_aggregate_tag("$tuple") {
                    ValueRep::Aggregate
                } else {
                    ValueRep::Cell
                };
                for (index, name) in names.into_iter().enumerate() {
                    let field = block.emit(
                        Operation::Project {
                            object: fields,
                            field: index,
                            rep,
                        },
                        span,
                    )?;
                    env.insert(name, field);
                }
            }
        }
        Ok(())
    }

    fn dictionary(
        &mut self,
        dict: &Dict,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        if let Dict::Structural { class, fields } = dict {
            return self.structural(class, fields, env, block, span);
        }
        let expr = dictionary(dict, span)?;
        self.expr(&expr, env, block)
    }

    fn constructor(
        &mut self,
        constructor: &Constructor,
        arguments: Vec<Local>,
        expr_ty: Option<&TypeRef>,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let total = constructor
            .arity
            .checked_add(constructor.dicts)
            .filter(|total| u32::try_from(*total).is_ok())
            .ok_or(LowerError::Limit(span))?;
        if arguments.len() == total {
            match &constructor.rep {
                flick_ir::Rep::Literal(flick_ir::EnumLit::Int(n)) => {
                    return block.emit(Operation::Literal(Lit::Int(*n)), span);
                }
                flick_ir::Rep::Literal(flick_ir::EnumLit::String(s)) => {
                    return block.emit(Operation::Literal(Lit::String(s.clone())), span);
                }
                flick_ir::Rep::Unboxed | flick_ir::Rep::Typeof(_)
                    if constructor.arity == 1 && constructor.dicts == 0 =>
                {
                    return Ok(arguments[0]);
                }
                _ => {}
            }
            // Evidence is applied first by ExprKind::Dicts, but stored
            // after ordinary fields, matching checked constructor patterns.
            let fields = arguments[constructor.dicts..].to_vec();
            let dicts = arguments[..constructor.dicts].to_vec();
            let tag = format!(
                "{}.{}.{}",
                constructor.module, constructor.ty, constructor.name
            );
            let rep = self
                .aggregates
                .constructor_rep(constructor, expr_ty, self.current_in_fallback);
            let layout = if rep.is_cell() {
                self.aggregates
                    .constructor_cell_layout(constructor, expr_ty, &tag)
            } else {
                None
            };
            let value = if constructor.dicts == 0 {
                Operation::Construct {
                    tag,
                    fields,
                    rep,
                    layout,
                }
            } else {
                Operation::ConstructWithDicts {
                    tag,
                    fields,
                    dicts,
                    rep,
                    layout,
                }
            };
            return block.emit(value, span);
        }
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Vec::new();
        for field in 0..arguments.len() {
            inside.push(function.emit(
                Operation::Project {
                    object: 0,
                    field,
                    rep: ValueRep::Cell,
                },
                span,
            )?);
        }
        inside.push(1);
        let result = self.constructor(constructor, inside, expr_ty, &mut function, span)?;
        self.finish(code, &function.finish(result))?;
        block.emit(
            Operation::Closure {
                code,
                captures: arguments,
            },
            span,
        )
    }
}
