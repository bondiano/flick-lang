impl Lower<'_> {
    pub(super) fn block(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let mark = self.scope.len();
        let statements: Vec<&Node> = node.nodes().collect();
        let siblings = (self.typing.siblings.get(&node.range()).cloned()).map(|f| (f, node.range()));
        let block = self.statements(&statements, span, siblings.as_ref());
        self.scope.truncate(mark);
        block
    }

    /// The statements of a block; a `let assert` takes the ones after it.
    /// With `siblings`, a function and the range of its block, the
    /// expression lines are the list it is applied to.
    fn statements(
        &mut self,
        statements: &[&Node],
        span: Span,
        siblings: Option<&(Ref, Range)>,
    ) -> Expr {
        let mut stmts = Vec::new();
        let mut result = None;
        let mut lines = Vec::new();
        for (i, statement) in statements.iter().enumerate() {
            let last = i + 1 == statements.len();
            match statement.kind {
                K::Signature => {}
                K::LetDecl if statement.has_token(K::AssertKw) => {
                    let rest = &statements[i + 1..];
                    let then = |l: &mut Self| l.statements(rest, span, siblings);
                    result = Some(self.let_assert(statement, span, then));
                    break;
                }
                K::LetDecl if statement.has_token(K::ElseKw) => {
                    let rest = &statements[i + 1..];
                    let then = |l: &mut Self| l.statements(rest, span, siblings);
                    result = Some(self.let_else(statement, span, then));
                    break;
                }
                K::LetDecl => self.local_let(statement, &mut stmts),
                K::AssignStmt => stmts.push(self.assign(statement)),
                K::DeferStmt if siblings.is_some() => stmts.push(Stmt::Expr(self.expr(statement))),
                _ if siblings.is_some() => lines.push(self.expr(statement)),
                _ if last => result = Some(self.expr(statement)),
                _ => stmts.push(Stmt::Expr(self.expr(statement))),
            }
        }
        if let Some((Ref::Value { module, name }, key)) = siblings
            && result.is_none()
        {
            let (module, name) = (module.clone(), name.clone());
            let function = Expr::new(
                ExprKind::Global {
                    module,
                    name,
                    type_args: Vec::new(),
                },
                span,
            );
            let app = ExprKind::App {
                func: Box::new(function),
                arg: Box::new(Expr::new(ExprKind::List(lines), span)),
                stage: self.call_stage(*key),
                arg_first: false,
            };
            let dicts = self.dictionaries(*key);
            result = Some(with_dicts(self.within(*key, Expr::new(app, span)), dicts));
        }
        let result = result.unwrap_or_else(|| {
            Expr::with_ty(
                ExprKind::Lit(Lit::Unit),
                span,
                super::types::lang_bare(Lang::Unit),
            )
        });
        if stmts.is_empty() {
            return result;
        }
        let ty = result.ty.clone();
        Expr::with_ty(ExprKind::Block(stmts, Box::new(result)), span, ty)
    }

    /// `let pattern = initializer else fallback` and the statements after it:
    /// `match initializer with | pattern -> rest | _ -> fallback`.
    fn let_else(
        &mut self,
        decl: &Node,
        span: Span,
        rest: impl FnOnce(&mut Self) -> Expr,
    ) -> Expr {
        let parts = let_parts(decl, self.text);
        let value = parts.body.map_or_else(
            || Expr::new(ExprKind::Lit(Lit::Unit), span),
            |b| self.expr(b),
        );
        let fallback = parts.fallback.map_or_else(
            || Expr::new(ExprKind::Lit(Lit::Unit), span),
            |b| self.expr(b),
        );
        let mut arms = Vec::new();
        if !self.typing.dead_arms.contains(&decl.range()) {
            let mark = self.scope.len();
            let (pat, binder_types) = parts
                .pattern
                .map_or((Pat::Wild, Vec::new()), |p| self.pattern_typed(p));
            let body = rest(self);
            self.scope.truncate(mark);
            arms.push(Arm {
                pat,
                binder_types,
                guard: None,
                body,
            });
        }
        arms.push(Arm {
            pat: Pat::Wild,
            binder_types: Vec::new(),
            guard: None,
            body: fallback,
        });
        Expr::with_ty(
            ExprKind::Match(Box::new(value), arms),
            span,
            self.ir_type(decl.range()),
        )
    }

    /// The statements of a local `let`: those of a `let lazy`, or one.
    pub(super) fn local_let(&mut self, decl: &Node, stmts: &mut Vec<Stmt>) {
        if self.typing.lazy_lets.contains(&decl.range()) {
            self.lazy_state(decl, stmts);
        } else {
            stmts.push(self.local_let_stmt(decl));
        }
    }

    fn local_let_stmt(&mut self, decl: &Node) -> Stmt {
        let parts = let_parts(decl, self.text);
        let Some(name) = parts.name.clone() else {
            let value = self.definition(decl);
            let (pat, binder_types) = parts
                .pattern
                .map_or((Pat::Wild, Vec::new()), |p| self.pattern_typed(p));
            return Stmt::let_typed(pat, binder_types, value);
        };
        let dicts = self.givens(decl);
        let binder = decl.nodes().find(|n| n.kind == K::VarPat);
        let at = binder.map_or(decl.range().0, |n| n.range().0);
        let binder_ty = binder.and_then(|n| self.ir_type(n.range()));
        let (type_params, dict_constraints, scheme_ty) = self.ir_local_scheme(decl.range());
        let dict_constraints = if dict_constraints.len() == dicts.len() {
            dict_constraints
        } else {
            Vec::new()
        };
        if parts.rec {
            let unique = self.bind(&name, at);
            #[cfg(not(target_arch = "wasm32"))]
            self.record_local_scheme(decl, &unique);
            let value = self.definition(decl);
            let ty = scheme_ty.or(binder_ty).or_else(|| value.ty.clone());
            return Stmt::Fun {
                name: unique,
                rec: true,
                type_params,
                dicts,
                dict_constraints,
                ty,
                value,
            };
        }
        let value = self.definition(decl);
        let unique = self.bind(&name, at);
        if decl.has_token(K::MutKw) {
            self.vars.insert(unique.clone());
            let ty = binder_ty.or_else(|| value.ty.clone());
            return Stmt::var(unique, ty, value);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.record_local_scheme(decl, &unique);
        if parts.params.is_empty() && dicts.is_empty() {
            let ty = binder_ty.or_else(|| value.ty.clone());
            let binder_types = ty.map(|t| vec![(unique.clone(), t)]).unwrap_or_default();
            Stmt::let_typed(Pat::Bind(unique), binder_types, value)
        } else {
            let ty = scheme_ty.or(binder_ty).or_else(|| value.ty.clone());
            Stmt::Fun {
                name: unique,
                rec: false,
                type_params,
                dicts,
                dict_constraints,
                ty,
                value,
            }
        }
    }

    /// `name = value` of the `let mut` variable `name` in scope.
    fn assign(&mut self, node: &Node) -> Stmt {
        let mut children = node.nodes();
        let target = children.next();
        let token = target.and_then(|t| t.child_tokens().find(|t| t.kind == K::Ident));
        let unique = token
            .and_then(|t| self.local(t.text(self.text), t.start))
            .map_or_else(
                || panic!("the checker resolved the target of an assignment"),
                str::to_owned,
            );
        let value = match children.next() {
            Some(value) => self.expr(value),
            None => Expr::new(ExprKind::Lit(Lit::Unit), self.span(node)),
        };
        let value = if self.typing.lazy_sets.contains(&node.range()) {
            super::lazy::written(value)
        } else {
            value
        };
        Stmt::Assign(unique, value)
    }
}
