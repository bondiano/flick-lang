impl Lower<'_> {
    fn ui_callable(&self, func: Expr, key: Range) -> Expr {
        let Some(Ref::Value { module, name }) = self.typing.through.get(&key).cloned() else {
            return func;
        };
        let span = func.span;
        let component = ExprKind::Global {
            module,
            name,
            type_args: Vec::new(),
        };
        Expr::new(
            ExprKind::App {
                func: Box::new(Expr::new(component, span)),
                arg: Box::new(func),
                stage: Stage::Pure,
                arg_first: false,
            },
            span,
        )
    }

    /// The stage of the call the application or pipe at `key` makes.
    pub(super) fn call_stage(&self, key: Range) -> Stage {
        if self.typing.runtime.contains(&key) {
            Stage::Runtime
        } else if self.typing.sync.contains(&key) {
            Stage::Sync
        } else {
            Stage::Pure
        }
    }

    /// `call`, acquiring into the owner the checker chose for it.
    pub(super) fn within(&mut self, key: Range, call: Expr) -> Expr {
        let Some(owner) = self.typing.owners.get(&key) else {
            return call;
        };
        let name = self.owner_name(*owner);
        let span = call.span;
        let ty = call.ty.clone();
        let mut wrapped = self.evidence_call(Expr::new(ExprKind::Local(name), span), call);
        if wrapped.ty.is_none() {
            wrapped.ty = ty;
        }
        wrapped
    }

    fn pack_module_expr(&mut self, node: &Node, span: Span) -> Expr {
        let fields = self
            .typing
            .packs
            .get(&node.range())
            .expect("the checker validated `pack`")
            .iter()
            .map(|(field, module, name)| {
                (
                    field.clone(),
                    Expr::new(
                        ExprKind::Global {
                            module: module.clone(),
                            name: name.clone(),
                            type_args: Vec::new(),
                        },
                        span,
                    ),
                )
            })
            .collect();
        Expr::new(ExprKind::Record(fields), span)
    }

    fn app_expr(&mut self, node: &Node, children: &[&Node], span: Span) -> Expr {
        if let Some(hole) = app_hole(node) {
            return self.app_hole(node, children, hole, span);
        }
        if let [callee, argument] = children
            && let Some(slots) = self.typing.dictionary_constructors.get(&callee.range())
        {
            let mut fields = self.fields(argument);
            for (written, runtime) in slots {
                if let Some((label, _)) = fields.iter_mut().find(|(label, _)| label == written) {
                    runtime.clone_into(label);
                }
            }
            Expr::new(ExprKind::Dictionary(fields), span)
        } else {
            self.apply(children[0], &children[1..])
        }
    }

    fn scope_expr(&mut self, node: &Node, body: &Node, span: Span) -> Expr {
        let mark = self.scope.len();
        let key = node.range();
        let inner = self.typing.uses_owner(Owner::Scope(key));
        let owner = match node.child_tokens().find(|t| t.kind == K::Ident) {
            Some(name) => Some(self.bind(name.text(self.text), name.start)),
            None => inner.then(|| self.hidden("$owner")),
        };
        if let Some(owner) = &owner {
            self.owners.insert(key, owner.clone());
        }
        let body = self.expr(body);
        self.scope.truncate(mark);
        Self::scope_run(owner, None, body, span)
    }

    fn unpack_module_expr(&mut self, node: &Node, children: &[&Node], span: Span) -> Expr {
        let Some(source) = children.first() else {
            self.unsupported(node);
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        let Some(body) = children.iter().find(|n| n.kind == K::Block) else {
            self.unsupported(node);
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        let Some(alias) = node.child_tokens().find(|t| t.kind == K::Con) else {
            self.unsupported(node);
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        let source = self.expr(source);
        let mark = self.scope.len();
        let mut statements = Vec::new();
        let opened = self.typing.existential_opens.contains(&node.range());
        let pair = if opened {
            let pair = self.bind("$existential", node.range().0);
            statements.push(Stmt::let_bind(Pat::Bind(pair.clone()), source.clone()));
            Some(pair)
        } else {
            None
        };
        let unique = self.bind(alias.text(self.text), alias.start);
        if let Some(pair) = &pair {
            let pair_value = Expr::new(ExprKind::Local(pair.clone()), span);
            let module = Expr::new(
                ExprKind::Field(Box::new(pair_value), "$module".to_owned()),
                span,
            );
            statements.push(Stmt::let_bind(Pat::Bind(unique.clone()), module));
            let mut tokens = node.child_tokens();
            let payload = tokens
                .find(|t| t.kind == K::Comma)
                .and_then(|_| tokens.find(|t| t.kind == K::Ident));
            if let Some(payload) = payload {
                let value = self.bind(payload.text(self.text), payload.start);
                let pair_value = Expr::new(ExprKind::Local(pair.clone()), span);
                let field = Expr::new(
                    ExprKind::Field(Box::new(pair_value), "$value".to_owned()),
                    span,
                );
                statements.push(Stmt::let_bind(Pat::Bind(value), field));
            }
        } else {
            statements.push(Stmt::let_bind(Pat::Bind(unique.clone()), source));
        }
        let body = self.expr(body);
        self.scope.truncate(mark);
        let result = if self.typing.existential_unpacks.contains(&node.range()) {
            Expr::new(
                ExprKind::Record(vec![
                    (
                        "$module".to_owned(),
                        Expr::new(ExprKind::Local(unique.clone()), span),
                    ),
                    ("$value".to_owned(), body),
                ]),
                span,
            )
        } else {
            body
        };
        let ty = result.ty.clone();
        Expr::with_ty(ExprKind::Block(statements, Box::new(result)), span, ty)
    }

    fn binary(&mut self, node: &Node, children: &[&Node]) -> Expr {
        let span = self.span(node);
        let key = node.range();
        let op = node
            .child_tokens()
            .find(|t| t.kind.is_binary_op())
            .map(|t| t.kind);
        match op {
            Some(K::LtPipe) => {
                return self.app_expr(node, &right_application(children[0], children[1]), span);
            }
            Some(K::OrElse) => return self.orelse(node, children[0], children[1], span),
            Some(K::GtGt | K::LtLt) => return self.composition(node),
            _ => {}
        }
        let lhs = self.boxed(children[0]);
        let rhs = self.expr(children[1]);
        let rhs = Box::new(if op == Some(K::PipeGt) {
            self.ui_callable(rhs, key)
        } else {
            rhs
        });
        if let (Some(kind @ (K::EqEq | K::NotEq)), Some(&identity)) =
            (op, self.typing.identity_eq.get(&key))
        {
            let op = if kind == K::EqEq {
                BinOp::Same
            } else {
                BinOp::NotSame
            };
            let tag = |e: Box<Expr>| {
                let span = e.span;
                Box::new(Expr::new(ExprKind::Field(e, "_tag".to_owned()), span))
            };
            let (lhs, rhs) = match identity {
                Identity::Value => (lhs, rhs),
                Identity::Tag => (tag(lhs), tag(rhs)),
            };
            return Expr::new(ExprKind::Binary(op, lhs, rhs), span);
        }
        if let Some(generic) = op.and_then(|op| self.generic_binary(key, op, &lhs, &rhs, span)) {
            return generic;
        }
        let op = match op {
            Some(K::PipeGt) => {
                let res_ty = self.ir_type(key).or_else(|| app_result_type(&rhs));
                let app = ExprKind::App {
                    func: rhs,
                    arg: lhs,
                    stage: self.call_stage(key),
                    arg_first: true,
                };
                let dicts = self.dictionaries(key);
                return with_dicts(self.within(key, Expr::with_ty(app, span, res_ty)), dicts);
            }
            Some(K::OrOr) => BinOp::Or,
            Some(K::AndAnd) => BinOp::And,
            Some(K::EqEq) => BinOp::Eq,
            Some(K::NotEq) => BinOp::NotEq,
            Some(K::Lt) => BinOp::Lt,
            Some(K::Le) => BinOp::Le,
            Some(K::Gt) => BinOp::Gt,
            Some(K::Ge) => BinOp::Ge,
            Some(K::PlusPlus) if self.typing.list_concat.contains(&key) => BinOp::ListConcat,
            Some(K::PlusPlus) => BinOp::Concat,
            k => BinOp::Arith(k.and_then(arith_of).unwrap_or(Arith::Pow), self.num(key)),
        };
        Expr::new(ExprKind::Binary(op, lhs, rhs), span)
    }

    /// `left orelse right` as `match left with | Some $orelse -> $orelse | None -> right`.
    fn orelse(&mut self, node: &Node, lhs: &Node, rhs: &Node, span: Span) -> Expr {
        let scrutinee = self.boxed(lhs);
        let inner_ty = scrutinee.ty.as_deref().and_then(|ty| match ty {
            flick_ir::IrType::Named { args, .. } if args.len() == 1 => Some(args[0].clone()),
            _ => None,
        });
        let fallback = self.expr(rhs);
        let value_name = self.hidden("$orelse");
        let op_key = node
            .child_tokens()
            .find(|t| t.kind == K::OrElse)
            .map(|t| (t.start, t.end));
        let value = Expr::with_ty(ExprKind::Local(value_name.clone()), span, inner_ty.clone());
        let value = match op_key {
            Some(key) => self.adapted(key, value),
            None => value,
        };
        let type_args: Vec<_> = inner_ty.iter().cloned().collect();
        let some_pat = Pat::Con(
            Constructor {
                fields: type_args.clone(),
                type_args: type_args.clone(),
                ..super::enums::native(Lang::Some, Lang::Option)
            },
            vec![Pat::Bind(value_name.clone())],
        );
        let none_pat = Pat::Con(
            Constructor {
                arity: 0,
                type_args,
                ..super::enums::native(Lang::None, Lang::Option)
            },
            Vec::new(),
        );
        let arms = vec![
            Arm {
                pat: some_pat,
                binder_types: inner_ty.map(|t| vec![(value_name, t)]).unwrap_or_default(),
                guard: None,
                body: value,
            },
            Arm {
                pat: none_pat,
                binder_types: Vec::new(),
                guard: None,
                body: fallback,
            },
        ];
        Expr::with_ty(
            ExprKind::Match(scrutinee, arms),
            span,
            self.ir_type(node.range()),
        )
    }

    /// `f >> g` as `{ let f' = f; let g' = g; fun x -> g' (f' x) }`, and
    /// `f << g` as the same closure applying `g'` first. A name or a `fun`,
    /// whose evaluation shows nothing and reads no variable, is used in
    /// place; a method may build its dictionary, so it is bound too.
    fn composition(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let Some((operands, operators)) = composition(node) else {
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        let entry = StageKey::Arrow {
            node: node.range(),
            index: 0,
        };
        self.register_owner_entry(entry);
        let mut stmts = Vec::new();
        let mut functions = Vec::new();
        for operand in &operands {
            let value = self.expr(operand);
            let in_place = matches!(
                value.kind,
                ExprKind::Local(_)
                    | ExprKind::Global { .. }
                    | ExprKind::Con(_)
                    | ExprKind::Lambda { .. }
            );
            if in_place {
                functions.push(value);
            } else {
                let name = self.hidden("$f");
                let at = value.span;
                let ty = value.ty.clone();
                let binders = ty.clone().map(|t| vec![(name.clone(), t)]).unwrap_or_default();
                stmts.push(Stmt::let_typed(Pat::Bind(name.clone()), binders, value));
                functions.push(Expr::with_ty(ExprKind::Local(name), at, ty));
            }
        }
        let x = self.hidden("$x");
        let x_ty = self.ir_type(param_key(&operators));
        let mut value = Expr::with_ty(ExprKind::Local(x.clone()), span, x_ty.clone());
        for i in applied_order(&operators) {
            let key = composed_key(&operands, &operators, i);
            let at = Span::new(self.file, key.0, key.1);
            let func = self.ui_callable(functions[i].clone(), key);
            let res_ty = self.ir_type(key).or_else(|| app_result_type(&func));
            let app = ExprKind::App {
                func: Box::new(func),
                arg: Box::new(self.adapted(key, value)),
                stage: self.call_stage(key),
                arg_first: false,
            };
            let app = Expr::with_ty(app, at, res_ty);
            let dicts = self.dictionaries(key);
            value = with_dicts(self.within(key, app), dicts);
        }
        let stage = self
            .typing
            .stages
            .get(&node.range())
            .and_then(|s| s.first());
        let x_binders = x_ty.clone().map(|t| vec![(x.clone(), t)]).unwrap_or_default();
        let params = vec![Param {
            pat: Pat::Bind(x),
            stage: stage.copied().unwrap_or(Stage::Pure),
            ty: x_ty,
            binder_types: x_binders,
        }];
        let body = {
            let value = self.adapted(result_key(&operators), value);
            self.owner_entry(entry, value)
        };
        let lam_ty = super::types::lambda_type(&params, &body);
        let lambda = Expr::with_ty(
            ExprKind::Lambda {
                params,
                body: Box::new(body),
            },
            span,
            lam_ty.clone(),
        );
        if stmts.is_empty() {
            lambda
        } else {
            Expr::with_ty(ExprKind::Block(stmts, Box::new(lambda)), span, lam_ty)
        }
    }

    /// `value` with the lift and the conversion the checker placed at `key`.
    fn adapted(&mut self, key: Range, value: Expr) -> Expr {
        let value = match self.typing.lifts.get(&key).cloned() {
            Some(lift) => {
                let span = value.span;
                let lifted = value.ty.as_ref().map(|t| apply_lift_type(t, &lift));
                Expr::with_ty(ExprKind::Lift(Box::new(value), lift), span, lifted)
            }
            None => value,
        };
        self.apply_coercion(key, value)
    }

    /// `label = value` fields of a record literal or update, in order.
    fn fields(&mut self, node: &Node) -> Vec<(String, Expr)> {
        node.nodes()
            .filter(|n| n.kind == K::RecordField)
            .filter_map(|field| {
                let label = field
                    .child_tokens()
                    .find(|t| matches!(t.kind, K::Ident | K::Con))?
                    .text(self.text)
                    .to_owned();
                let value = field.nodes().next()?;
                Some((label, self.expr(value)))
            })
            .collect()
    }

    /// Whether a `try` has arms for operations or for the value of its
    /// body, or the operand of a `provide` is `S impl`, a service and the
    /// record that handles it.
    fn handles_effects(&self, node: &Node) -> bool {
        let Some(first) = node.nodes().next() else {
            return false;
        };
        match node.kind {
            K::TryExpr => node.nodes().filter(|n| n.kind == K::MatchArm).any(|arm| {
                arm.has_token(K::ReturnKw)
                    || arm_pattern(arm).is_some_and(|p| operation_path(p, self.text).is_some())
            }),
            _ => {
                first.kind == K::AppExpr
                    && first.nodes().next().is_some_and(|head| {
                        matches!(
                            self.typing.names.get(&head.range()),
                            Some(Ref::ServiceTag(_))
                        )
                    })
            }
        }
    }

    fn arms(&mut self, node: &Node) -> Vec<Arm> {
        // an arm the type of the matched value rules out was not checked
        let live: Vec<&Node> = node
            .nodes()
            .filter(|n| n.kind == K::MatchArm && !self.typing.dead_arms.contains(&n.range()))
            .collect();
        live.into_iter().map(|arm| self.arm(arm)).collect()
    }

    /// `| pattern [when guard] -> body`.
    pub(super) fn arm(&mut self, arm: &Node) -> Arm {
        let mark = self.scope.len();
        let mut pat = Pat::Wild;
        let mut binder_types = Vec::new();
        let mut guard = None;
        let mut body = None;
        for child in arm.nodes() {
            match child.kind {
                K::Guard => guard = child.nodes().next().map(|g| self.expr(g)),
                K::Block => body = Some(self.expr(child)),
                kind if is_pattern(kind) => {
                    let child = failed_pattern(child, self.text).unwrap_or(child);
                    (pat, binder_types) = self.pattern_typed(child);
                }
                _ => {}
            }
        }
        self.scope.truncate(mark);
        let span = self.span(arm);
        Arm {
            pat,
            binder_types,
            guard,
            body: body.unwrap_or_else(|| Expr::new(ExprKind::Lit(Lit::Unit), span)),
        }
    }
}
