impl Lower<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "one exhaustive dispatch from compiler expression forms to ownership operations"
    )]
    fn expr(&mut self, expr: &Expr, env: &Env, block: &mut Code) -> Result<Local, LowerError> {
        let span = expr.span;
        match &expr.kind {
            ExprKind::Lit(value) => block.emit(Operation::Literal(value.clone()), span),
            ExprKind::ServiceTag(key) => block.emit(Operation::ServiceTag(key.clone()), span),
            ExprKind::Local(name) => lookup(env, name, span),
            ExprKind::Var(name) => {
                let handle = lookup(env, name, span)?;
                block.emit(Operation::VarGet(handle), span)
            }
            ExprKind::Label(name) => block.emit(Operation::Label(name.clone()), span),
            ExprKind::Unwrap(operand) => {
                let option = self.expr(operand, env, block)?;
                block.emit(Operation::UnwrapOption(option), span)
            }
            ExprKind::Loop { condition, body } => {
                self.loop_expr(condition, body, env, block, span)
            }
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                iterable,
            } => self.control_loop(id, condition.as_deref(), body, *iterable, env, block, span),
            ExprKind::LoopExit { id, value } => {
                self.loop_exit(id, value.as_deref(), env, block, span)
            }
            ExprKind::Generate {
                body,
                strict,
                sync,
                limit,
            } => self.generate(body, *strict, *sync, limit.as_deref(), env, block, span),
            ExprKind::Each {
                stop,
                pat,
                source,
                body,
            } => self.each(stop.as_deref(), pat, source, body, env, block, span),
            ExprKind::Yield(value) => self.yield_expr(value, env, block, span),
            ExprKind::Global { module, name } => {
                let name = Name::new(module, name);
                self.global(&name, Some(span))?;
                block.emit(Operation::Global(name), span)
            }
            ExprKind::Lambda { params, body } => self.lambda(params, body, env, None, block, span),
            ExprKind::Abstract(names, body) => {
                let params = abstract_params(names);
                self.lambda(&params, body, env, None, block, span)
            }
            ExprKind::App {
                func,
                arg,
                arg_first,
                ..
            } => {
                let (function, argument) = if *arg_first {
                    let argument = self.expr(arg, env, block)?;
                    (self.expr(func, env, block)?, argument)
                } else {
                    let function = self.expr(func, env, block)?;
                    (function, self.expr(arg, env, block)?)
                };
                block.emit(Operation::Apply { function, argument }, span)
            }
            ExprKind::Block(statements, result) => self.block(statements, result, env, block),
            ExprKind::If(condition, yes, no) => {
                self.conditional(condition, yes, no, env, block, span)
            }
            ExprKind::Match(value, arms) => self.matching(value, arms, env, block, span),
            ExprKind::Binary(BinOp::And, left, right) => {
                let no = Expr::new(ExprKind::Lit(Lit::Bool(false)), span);
                self.conditional(left, right, &no, env, block, span)
            }
            ExprKind::Binary(BinOp::Or, left, right) => {
                let yes = Expr::new(ExprKind::Lit(Lit::Bool(true)), span);
                self.conditional(left, &yes, right, env, block, span)
            }
            ExprKind::Binary(op, left, right) => {
                let left = self.expr(left, env, block)?;
                let right = self.expr(right, env, block)?;
                block.emit(
                    Operation::Binary {
                        op: *op,
                        left,
                        right,
                    },
                    span,
                )
            }
            ExprKind::Not(value) => {
                let value = self.expr(value, env, block)?;
                block.emit(Operation::Not(value), span)
            }
            ExprKind::Neg(num, value) => {
                let value = self.expr(value, env, block)?;
                block.emit(Operation::Neg { num: *num, value }, span)
            }
            ExprKind::Tuple(values) if values.is_empty() => {
                block.emit(Operation::Literal(Lit::Unit), span)
            }
            ExprKind::Tuple(values) | ExprKind::List(values) => {
                let tag = if matches!(expr.kind, ExprKind::Tuple(_)) {
                    "$tuple"
                } else {
                    "$list"
                };
                let fields = values
                    .iter()
                    .map(|value| self.expr(value, env, block))
                    .collect::<Result<_, _>>()?;
                block.emit(
                    Operation::Construct {
                        tag: tag.into(),
                        fields,
                    },
                    span,
                )
            }
            ExprKind::Record(fields) => {
                let mut fields: Vec<(String, _)> = fields
                    .iter()
                    .map(|(label, value)| Ok((label.clone(), self.expr(value, env, block)?)))
                    .collect::<Result<_, LowerError>>()?;
                flick_ir::sort_by_utf16(&mut fields, |f| &f.0);
                block.emit(Operation::Record { fields }, span)
            }
            ExprKind::Dictionary(fields) => {
                let entries = fields
                    .iter()
                    .map(|(label, value)| {
                        Ok((
                            label.clone(),
                            memory::DictionaryEntry::Value(self.expr(value, env, block)?),
                        ))
                    })
                    .collect::<Result<_, LowerError>>()?;
                block.emit(Operation::Dictionary { entries }, span)
            }
            ExprKind::Update(object, fields) => {
                let object = self.expr(object, env, block)?;
                let fields = fields
                    .iter()
                    .map(|(label, value)| Ok((label.clone(), self.expr(value, env, block)?)))
                    .collect::<Result<_, LowerError>>()?;
                block.emit(Operation::Update { object, fields }, span)
            }
            ExprKind::Field(object, field) => {
                let object = self.expr(object, env, block)?;
                block.emit(
                    Operation::Field {
                        object,
                        field: field.clone(),
                    },
                    span,
                )
            }
            ExprKind::Method { dict, name } => {
                let object = self.dictionary(dict, env, block, span)?;
                block.emit(
                    Operation::Method {
                        dictionary: object,
                        field: name.clone(),
                    },
                    span,
                )
            }
            ExprKind::Dicts(inner, dictionaries) => {
                if let ExprKind::Perform { effect, op, stages } = &inner.kind {
                    let dictionaries = dictionaries
                        .iter()
                        .map(|dictionary| self.dictionary(dictionary, env, block, span))
                        .collect::<Result<Vec<_>, _>>()?;
                    return self.perform(effect, op, stages, dictionaries, block, span);
                }
                let mut function = self.expr(inner, env, block)?;
                for dictionary in dictionaries {
                    let argument = self.dictionary(dictionary, env, block, span)?;
                    function = block.emit(Operation::Apply { function, argument }, span)?;
                }
                Ok(function)
            }
            ExprKind::Con(constructor) => self.constructor(constructor, vec![], block, span),
            ExprKind::Ignore(value) => {
                self.expr(value, env, block)?;
                block.emit(Operation::Literal(Lit::Unit), span)
            }
            ExprKind::Lift(value, _) => self.expr(value, env, block),
            ExprKind::Task(body) => self.task(body, env, block, span),
            ExprKind::Within(owner, call) => self.within(owner, call, env, block, span),
            ExprKind::ReadOwnerEvidence => block.emit(Operation::ReadOwnerEvidence, span),
            ExprKind::WithOwnerEvidence(owner, call) => {
                let owner = self.expr(owner, env, block)?;
                self.owner_evidence(owner, call, env, block, span)
            }
            ExprKind::Await(value) => {
                let value = self.expr(value, env, block)?;
                block.emit(Operation::Await(value), span)
            }
            ExprKind::Handle(handler) if handler.is_try() => {
                self.try_only(handler, env, block, span)
            }
            ExprKind::Handle(handler) => self.handler(handler, env, block, span),
            ExprKind::Perform { effect, op, stages } => {
                self.perform(effect, op, stages, vec![], block, span)
            }
        }
    }

    fn block(
        &mut self,
        statements: &[Stmt],
        result: &Expr,
        env: &Env,
        block: &mut Code,
    ) -> Result<Local, LowerError> {
        let mut env = env.clone();
        for statement in statements {
            match statement {
                Stmt::Let(pattern, value) => {
                    let local = self.expr(value, &env, block)?;
                    self.pattern(pattern, local, &mut env, block, value.span)?;
                }
                Stmt::Expr(value) => {
                    self.expr(value, &env, block)?;
                }
                Stmt::Fun {
                    name,
                    rec,
                    dicts,
                    value,
                } => {
                    let value = if dicts.is_empty() {
                        value.clone()
                    } else {
                        Expr::new(
                            ExprKind::Abstract(dicts.clone(), Box::new(value.clone())),
                            value.span,
                        )
                    };
                    let local = if *rec {
                        match &value.kind {
                            ExprKind::Lambda { params, body } => {
                                self.lambda(params, body, &env, Some(name), block, value.span)?
                            }
                            ExprKind::Abstract(names, body) => self.lambda(
                                &abstract_params(names),
                                body,
                                &env,
                                Some(name),
                                block,
                                value.span,
                            )?,
                            _ => return Err(unsupported("recursive nonfunction", value.span)),
                        }
                    } else {
                        self.expr(&value, &env, block)?
                    };
                    env.insert(name.clone(), local);
                }
                Stmt::Var(name, value) => {
                    let init = self.expr(value, &env, block)?;
                    let handle = block.emit(Operation::VarMake(init), value.span)?;
                    env.insert(name.clone(), handle);
                }
                Stmt::Assign(name, value) => {
                    let var = lookup(&env, name, value.span)?;
                    let replacement = self.expr(value, &env, block)?;
                    block.emit(
                        Operation::VarSet {
                            var,
                            value: replacement,
                        },
                        value.span,
                    )?;
                }
            }
        }
        self.expr(result, &env, block)
    }

    fn lambda(
        &mut self,
        params: &[Param],
        body: &Expr,
        env: &Env,
        recursive: Option<&str>,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let Some((first, rest)) = params.split_first() else {
            return Err(unsupported("zero-parameter lambda", span));
        };
        let whole = Expr::new(
            ExprKind::Lambda {
                params: params.to_vec(),
                body: Box::new(body.clone()),
            },
            span,
        );
        let mut names = free(&whole);
        if let Some(name) = recursive {
            names.remove(name);
        }
        let names: Vec<_> = names.into_iter().collect();
        let captures = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<_, _>>()?;
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Env::new();
        for (index, name) in names.into_iter().enumerate() {
            let value = function.emit(
                Operation::Project {
                    object: 0,
                    field: index,
                },
                span,
            )?;
            inside.insert(name, value);
        }
        if let Some(name) = recursive {
            inside.insert(name.to_owned(), 0);
        }
        self.pattern(&first.pat, 1, &mut inside, &mut function, span)?;
        let saved_for = self.active_for_loop.take();
        let result = if rest.is_empty() {
            if loops::contains_direct_unwrap(body) {
                self.function_boundary(body, &inside, &mut function, span)
            } else {
                self.expr(body, &inside, &mut function)
            }
        } else {
            // Later curried stages capture the OUTER recursive self. They
            // must not remap its source name to their own inner self token.
            self.lambda(rest, body, &inside, None, &mut function, span)
        };
        self.active_for_loop = saved_for;
        let result = result?;
        self.finish(code, &function.finish(result))?;
        block.emit(Operation::Closure { code, captures }, span)
    }

    fn task(
        &mut self,
        body: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let names: Vec<_> = free(body).into_iter().collect();
        let captures = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<_, _>>()?;
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Env::new();
        for (field, name) in names.into_iter().enumerate() {
            let value = function.emit(Operation::Project { object: 0, field }, span)?;
            inside.insert(name, value);
        }
        let saved_for = self.active_for_loop.take();
        let result = if loops::contains_direct_unwrap(body) {
            self.function_boundary(body, &inside, &mut function, span)
        } else {
            self.expr(body, &inside, &mut function)
        };
        self.active_for_loop = saved_for;
        let result = result?;
        self.finish(code, &function.finish(result))?;
        block.emit(Operation::Task { code, captures }, span)
    }

    fn within(
        &mut self,
        owner: &str,
        call: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let owner = lookup(env, owner, span)?;
        let names: Vec<_> = free(call).into_iter().collect();
        let args = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<Vec<_>, _>>()?;
        let params = (0..args.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let inside = names.into_iter().zip(params.iter().copied()).collect();
        let mut helper = Code::new(params);
        let result = self.expr(call, &inside, &mut helper)?;
        let code = self.reserve(span)?;
        self.finish(code, &helper.finish(result))?;
        block.emit(Operation::Within { owner, code, args }, span)
    }

    fn owner_evidence(
        &mut self,
        owner: Local,
        call: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let names: Vec<_> = free(call).into_iter().collect();
        let args = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<Vec<_>, _>>()?;
        let params = (0..args.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let inside = names.into_iter().zip(params.iter().copied()).collect();
        let mut helper = Code::new(params);
        let result = self.expr(call, &inside, &mut helper)?;
        let code = self.reserve(span)?;
        self.finish(code, &helper.finish(result))?;
        block.emit(Operation::WithOwnerEvidence { owner, code, args }, span)
    }
}
