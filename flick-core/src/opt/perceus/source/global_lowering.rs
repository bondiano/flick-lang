impl Lower<'_> {
    fn reserve(&mut self, span: Span) -> Result<CodeId, LowerError> {
        let code = self.next;
        self.next = self.next.checked_add(1).ok_or(LowerError::Limit(span))?;
        Ok(code)
    }

    fn finish(&mut self, code: CodeId, function: &Function) -> Result<(), LowerError> {
        let function = insert(function).map_err(|error| LowerError::Ownership { code, error })?;
        self.program.functions.insert(code, function);
        Ok(())
    }

    fn global(&mut self, name: &Name, span: Option<Span>) -> Result<(), LowerError> {
        if self.initializers.contains_key(name) {
            return Ok(());
        }
        let item = *self
            .sources
            .get(name)
            .ok_or_else(|| LowerError::UnknownGlobal {
                name: name.clone(),
                span,
            })?;
        let code = self.reserve(item.span)?;
        // Register before visiting the body: recursive functions refer to
        // a global symbol, never capture their own global root.
        self.initializers.insert(name.clone(), code);
        self.program.globals.push(Global {
            name: name.clone(),
            initializer: code,
        });
        let mut block = Code::new(vec![]);
        let value = match &item.kind {
            ItemKind::Value { dicts, body, .. } => {
                if dicts.is_empty() {
                    self.expr(body, &Env::new(), &mut block)?
                } else {
                    let body = Expr::new(
                        ExprKind::Abstract(dicts.clone(), Box::new(body.clone())),
                        body.span,
                    );
                    self.expr(&body, &Env::new(), &mut block)?
                }
            }
            ItemKind::Foreign(value) => self.foreign(value, name, item.span, &mut block)?,
            ItemKind::Instance { info, methods } => {
                self.instance(info, methods, &[], &mut block, item.span)?
            }
            ItemKind::Destructure { .. } => {
                return Err(unsupported("top-level destructuring", item.span));
            }
            ItemKind::Constructor(_) | ItemKind::Service { .. } => {
                unreachable!("only value definitions are indexed")
            }
        };
        self.finish(code, &block.finish(value))
    }

    fn foreign(
        &mut self,
        value: &Foreign,
        name: &Name,
        span: Span,
        block: &mut Code,
    ) -> Result<Local, LowerError> {
        if app_fail::is_binding(value) {
            return self.app_fail(value, name, span, block);
        } else if let Some(result) = self.adapted_foreign(value, name, span, block) {
            return result;
        }
        if value.binding.from.as_deref() == Some("flick:rt") && value.binding.name == "listShow" {
            if !list_show::supports(value) {
                return Err(unsupported("listShow ownership adapter", span));
            }
            return self.expr(&list_show::expression(span), &Env::new(), block);
        }
        if value.binding.from.as_deref() == Some("flick:rt") && value.binding.name == "fromResult" {
            if !failures::supports(value) {
                return Err(unsupported("fromResult ownership adapter", span));
            }
            return self.intrinsic(Intrinsic::FromResult, false, vec![], block, span);
        }
        let kind = foreign_intrinsic(value).ok_or_else(|| {
            unsupported(
                format!(
                    "foreign binding {:?}:{}",
                    value.binding.from, value.binding.name
                ),
                span,
            )
        })?;
        let adapter = &value.adapter;
        let internal_owned = matches!(
            kind,
            Intrinsic::RefMake
                | Intrinsic::RefGet
                | Intrinsic::RefSet
                | Intrinsic::RefModify
                | Intrinsic::ArenaAlloc
                | Intrinsic::ArenaGet
                | Intrinsic::ScopeRun
        );
        let callback = match kind {
            Intrinsic::RefModify => Some(1),
            Intrinsic::ScopeRun => Some(0),
            _ => None,
        };
        let callbacks_valid = adapter.callbacks.iter().enumerate().all(|(index, arity)| {
            *arity
                == if callback == Some(index) {
                    Some(1)
                } else {
                    None
                }
        }) && (callback
            .is_none_or(|index| adapter.callbacks.get(index) == Some(&Some(1))));
        // These are internal language operations, not a host export of
        // mutable data. Ref contents stay immutable owned values; writes
        // replace slots, and reads retain before returning. Defensive host
        // copies are therefore discharged by the intrinsic's ownership
        // contract, not erased for arbitrary foreign functions.
        let copies_valid = if internal_owned {
            adapter
                .copies
                .callbacks
                .iter()
                .enumerate()
                .all(|(index, copy)| {
                    copy.as_ref()
                        .is_none_or(|(arguments, result)| match (kind, index) {
                            // The quantified Scope callback's copy metadata has
                            // no data parameters: its sole argument is an opaque
                            // owner, supplied directly by this intrinsic. Unary
                            // calling convention is checked independently above.
                            (Intrinsic::ScopeRun, 0) => arguments.is_empty() && !result,
                            (Intrinsic::RefModify, 1) => arguments.len() == 1,
                            _ => false,
                        })
                })
        } else {
            !adapter.copies.result
                && (kind == Intrinsic::Show || !adapter.copies.args.contains(&true))
                && !adapter
                    .copies
                    .callbacks
                    .iter()
                    .flatten()
                    .any(|(args, result)| *result || args.contains(&true))
        };
        if value.dicts != 0
            || value.stages.len() != kind.arity()
            || value.arity != kind.arity()
            || value.catch.is_some()
            || adapter.receiver
            || adapter.cancel
            || !adapter.optional.is_empty()
            || adapter.variadic
            || adapter.result.nullable
            || !callbacks_valid
            || !copies_valid
            || !matches!(adapter.result_shape, Shape::Data)
            || adapter.shapes.iter().enumerate().any(|(index, shape)| {
                if kind == Intrinsic::ScopeRun && index == 0 {
                    !matches!(shape, Shape::Fun { args, result, runtime: true, .. }
                        if args == &[Shape::Data] && matches!(result.as_ref(), Shape::Data))
                } else {
                    !matches!(shape, Shape::Data)
                }
            })
        {
            return Err(unsupported("foreign ownership adapter", span));
        }
        let unit = adapter.result.unit && !matches!(kind, Intrinsic::PrintLine | Intrinsic::RefSet);
        self.intrinsic(kind, unit, vec![], block, span)
    }

    fn intrinsic(
        &mut self,
        kind: Intrinsic,
        unit: bool,
        arguments: Vec<Local>,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        if arguments.len() == kind.arity() {
            let result = block.emit(
                Operation::Intrinsic {
                    kind,
                    args: arguments,
                },
                span,
            )?;
            return if unit {
                block.emit(Operation::Literal(Lit::Unit), span)
            } else {
                Ok(result)
            };
        }
        let code = self.reserve(span)?;
        let mut function = Code::new(vec![0, 1]);
        let mut inside = Vec::new();
        for field in 0..arguments.len() {
            inside.push(function.emit(Operation::Project { object: 0, field }, span)?);
        }
        inside.push(1);
        let result = self.intrinsic(kind, unit, inside, &mut function, span)?;
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
