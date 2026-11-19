impl Build<'_> {
    fn adapt(&mut self, desc: Expr, raw: &Expr) -> Expr {
        let head = self.call(Op::Head, desc.clone(), self.list(Vec::new()), self.unit());
        let current = self.name();
        let args = self.name();
        let result = self.nodes(self.local(&current), |this, _, node, env, _| {
            match &node.kind {
                OwnerTypeKind::Fun { runtime, stage, .. } => {
                    let arg = this.name();
                    let stage = *stage;
                    let call = this.app(raw.clone(), this.local(&arg), stage);
                    let services = runtime.then(|| this.child(node.children[1], &env));
                    let result = this.child(*node.children.last().expect("arrow result"), &env);
                    let body = this.execute(call, services, result);
                    this.lambda(Pat::Bind(arg), stage, body)
                }
                OwnerTypeKind::Con { module, name } if lang::is(Lang::Eff, module, name) => {
                    let services = this.name();
                    let errors = this.name();
                    let result = this.name();
                    let call = this.e(ExprKind::Await(Box::new(raw.clone())));
                    let body = this.execute(call, Some(this.local(&services)), this.local(&result));
                    let task = this.e(ExprKind::Task(Box::new(body)));
                    this.case(
                        this.local(&args),
                        vec![
                            (
                                Pat::List(
                                    vec![Pat::Bind(services), Pat::Bind(errors), Pat::Bind(result)],
                                    None,
                                ),
                                task,
                            ),
                            (Pat::Wild, raw.clone()),
                        ],
                    )
                }
                OwnerTypeKind::Forall { vars, constraints } => this.adapt_forall(
                    raw,
                    node,
                    env,
                    vars.iter().map(|v| v.name.clone()).collect(),
                    *constraints,
                ),
                _ => raw.clone(),
            }
        });
        let adapted = self.unpack(head, &[&current, &args], result);
        self.choose(self.observe(Op::Productive, desc), adapted, raw.clone())
    }

    fn execute(&mut self, call: Expr, services: Option<Expr>, result: Expr) -> Expr {
        let call = match services {
            Some(services) => {
                let owner = self.name();
                let raw = self.e(ExprKind::Within(owner.clone(), Box::new(call.clone())));
                let activated = self.block(
                    vec![Stmt::Let(
                        Pat::Bind(owner),
                        self.e(ExprKind::ReadOwnerEvidence),
                    )],
                    raw,
                );
                self.choose(self.observe(Op::Scope, services), activated, call)
            }
            None => call,
        };
        self.bind(call, |this, value| this.observe_adapt(result, value))
    }

    fn adapt_forall(
        &mut self,
        raw: &Expr,
        node: &OwnerTypeExpr,
        env: Expr,
        vars: Vec<String>,
        constraints: usize,
    ) -> Expr {
        let masked = self.environment(
            vars.into_iter().map(|name| (name, self.unit())).collect(),
            Some(env),
        );
        self.bind(masked, |this, env| {
            let mut count = this.int(0);
            for &constraint in &node.children[..constraints] {
                let passes = this.observe(Op::Dictionary, this.child(constraint, &env));
                let add = this.choose(passes, this.int(1), this.int(0));
                count = this.binary(BinOp::Arith(Arith::Add, Num::Int), count, add);
            }
            let result = this.child(node.children[constraints], &env);
            let arms = (0..=constraints)
                .map(|count| {
                    let names: Vec<_> = (0..count).map(|_| this.name()).collect();
                    let value = if names.is_empty() {
                        raw.clone()
                    } else {
                        this.e(ExprKind::Dicts(
                            Box::new(raw.clone()),
                            names.iter().cloned().map(Dict::Given).collect(),
                        ))
                    };
                    let adapted = this.observe_adapt(result.clone(), value);
                    let body = if names.is_empty() {
                        adapted
                    } else {
                        this.e(ExprKind::Abstract(names, Box::new(adapted)))
                    };
                    (
                        Pat::Lit(Lit::Int(
                            i32::try_from(count).expect("written constraint count"),
                        )),
                        body,
                    )
                })
                .collect();
            this.case(count, arms)
        })
    }
}
