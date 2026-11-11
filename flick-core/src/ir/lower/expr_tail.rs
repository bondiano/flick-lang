impl Lower<'_> {
    fn binary(&mut self, node: &Node, children: &[&Node]) -> Expr {
        let span = self.span(node);
        let key = node.range();
        let op = node
            .child_tokens()
            .find(|t| t.kind.is_binary_op())
            .map(|t| t.kind);
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
                let app = ExprKind::App {
                    func: rhs,
                    arg: lhs,
                    stage: self.call_stage(key),
                    arg_first: true,
                };
                let dicts = self.dictionaries(key);
                return with_dicts(self.within(key, Expr::new(app, span)), dicts);
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

    pub(super) fn block(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let mark = self.scope.len();
        let statements: Vec<&Node> = node.nodes().collect();
        let block = self.statements(&statements, span);
        self.scope.truncate(mark);
        block
    }

    /// The statements of a block; a `let assert` takes the ones after it.
    pub(super) fn statements(&mut self, statements: &[&Node], span: Span) -> Expr {
        let mut stmts = Vec::new();
        let mut result = None;
        for (i, statement) in statements.iter().enumerate() {
            let last = i + 1 == statements.len();
            match statement.kind {
                K::Signature => {}
                K::LetDecl if statement.has_token(K::AssertKw) => {
                    let rest = &statements[i + 1..];
                    result = Some(self.let_assert(statement, span, |l| l.statements(rest, span)));
                    break;
                }
                K::LetDecl => stmts.push(self.local_let(statement)),
                K::AssignStmt => stmts.push(self.assign(statement)),
                _ if last => result = Some(self.expr(statement)),
                _ => stmts.push(Stmt::Expr(self.expr(statement))),
            }
        }
        let result = result.unwrap_or_else(|| Expr::new(ExprKind::Lit(Lit::Unit), span));
        if stmts.is_empty() {
            return result;
        }
        Expr::new(ExprKind::Block(stmts, Box::new(result)), span)
    }

    pub(super) fn local_let(&mut self, decl: &Node) -> Stmt {
        let parts = let_parts(decl, self.text);
        let Some(name) = parts.name.clone() else {
            let value = self.definition(decl);
            let pat = parts.pattern.map_or(Pat::Wild, |p| self.pattern(p));
            return Stmt::Let(pat, value);
        };
        let dicts = self.givens(decl);
        let at =
            (decl.nodes().find(|n| n.kind == K::VarPat)).map_or(decl.range().0, |n| n.range().0);
        if parts.rec {
            let unique = self.bind(&name, at);
            let value = self.definition(decl);
            return Stmt::Fun {
                name: unique,
                rec: true,
                dicts,
                value,
            };
        }
        let value = self.definition(decl);
        let unique = self.bind(&name, at);
        if decl.has_token(K::MutKw) {
            self.vars.insert(unique.clone());
            return Stmt::Var(unique, value);
        }
        if parts.params.is_empty() && dicts.is_empty() {
            Stmt::Let(Pat::Bind(unique), value)
        } else {
            Stmt::Fun {
                name: unique,
                rec: false,
                dicts,
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
        Stmt::Assign(unique, value)
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
        let mut guard = None;
        let mut body = None;
        for child in arm.nodes() {
            match child.kind {
                K::Guard => guard = child.nodes().next().map(|g| self.expr(g)),
                K::Block => body = Some(self.expr(child)),
                kind if is_pattern(kind) => {
                    let child = failed_pattern(child, self.text).unwrap_or(child);
                    pat = self.pattern(child);
                }
                _ => {}
            }
        }
        self.scope.truncate(mark);
        let span = self.span(arm);
        Arm {
            pat,
            guard,
            body: body.unwrap_or_else(|| Expr::new(ExprKind::Lit(Lit::Unit), span)),
        }
    }
}
