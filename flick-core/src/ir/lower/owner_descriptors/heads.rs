#[derive(PartialEq, Eq)]
enum HeadKey {
    Alias(Vec<String>),
    Unchanged,
}

impl Build<'_> {
    fn alias_body(&mut self, desc: Expr) -> Expr {
        self.nodes_with_default(
            desc,
            |this, _| this.impossible(),
            |this, _, node, _| {
                let OwnerTypeKind::Con { module, name } = &node.kind else {
                    return None;
                };
                this.templates
                    .aliases
                    .iter()
                    .find(|alias| alias.module == *module && alias.name == *name)
                    .map(|alias| this.int(alias.body))
            },
        )
    }

    fn head(&mut self, desc: Expr, stack: Expr, _state: Expr) -> Expr {
        let reduced = self.call(Op::Spine, desc, stack, self.unit());
        let current = self.name();
        let args = self.name();
        let result = self.nodes_grouped(
            self.local(&current),
            |this, node| match &node.kind {
                OwnerTypeKind::Con { module, name } => {
                    if let Some(alias) = this
                        .templates
                        .aliases
                        .iter()
                        .find(|alias| alias.module == *module && alias.name == *name)
                    {
                        Some(HeadKey::Alias(alias.params.clone()))
                    } else if lang::is(Lang::Errors, module, name) {
                        None
                    } else {
                        Some(HeadKey::Unchanged)
                    }
                }
                OwnerTypeKind::Captured { .. } | OwnerTypeKind::Bounded => None,
                _ => Some(HeadKey::Unchanged),
            },
            |this, _, node, env, current| {
                let unchanged = this.tuple(vec![current.clone(), this.local(&args)]);
                let decremented = this.unit();
                match &node.kind {
                    OwnerTypeKind::Con { module, name } => {
                        let alias = this
                            .templates
                            .aliases
                            .iter()
                            .find(|alias| alias.module == *module && alias.name == *name);
                        if let Some(alias) = alias {
                            let bindings: Vec<_> =
                                alias.params.iter().map(|_| this.name()).collect();
                            let rest = this.name();
                            let environment = this.environment(
                                alias
                                    .params
                                    .iter()
                                    .zip(&bindings)
                                    .map(|(param, name)| (param.clone(), this.local(name)))
                                    .collect(),
                                None,
                            );
                            let next = this.call(
                                Op::Head,
                                this.tuple(vec![this.observe(Op::AliasBody, current), environment]),
                                this.local(&rest),
                                decremented,
                            );
                            let pattern = Pat::List(
                                bindings.into_iter().map(Pat::Bind).collect(),
                                Some(Box::new(Pat::Bind(rest))),
                            );
                            this.case(
                                this.local(&args),
                                vec![(pattern, next), (Pat::Wild, unchanged.clone())],
                            )
                        } else if lang::is(Lang::Errors, module, name) {
                            this.project_errors(this.local(&args), decremented, unchanged.clone())
                        } else {
                            unchanged
                        }
                    }
                    OwnerTypeKind::Captured { .. } | OwnerTypeKind::Bounded => this.call(
                        Op::Head,
                        this.child(node.children[0], &env),
                        this.local(&args),
                        this.unit(),
                    ),
                    _ => unchanged,
                }
            },
        );
        self.unpack(reduced, &[&current, &args], result)
    }

    fn project_errors(&mut self, args: Expr, state: Expr, unchanged: Expr) -> Expr {
        let row = self.name();
        let entries = self.name();
        let open = self.name();
        let label = self.name();
        let value = self.name();
        let never = self.call(
            Op::Head,
            self.desc(self.templates.never, self.root_env()),
            self.list(Vec::new()),
            state.clone(),
        );
        let single = self.call(Op::Head, self.local(&value), self.list(Vec::new()), state);
        let projected = self.case(
            self.local(&entries),
            vec![
                (Pat::List(Vec::new(), None), never),
                (
                    Pat::List(
                        vec![Pat::Tuple(vec![Pat::Bind(label), Pat::Bind(value)])],
                        None,
                    ),
                    single,
                ),
                (Pat::Wild, unchanged.clone()),
            ],
        );
        let result = self.choose(self.local(&open), unchanged.clone(), projected);
        let result = self.unpack(
            self.observe(Op::Row, self.local(&row)),
            &[&entries, &open],
            result,
        );
        self.case(
            args,
            vec![
                (Pat::List(vec![Pat::Bind(row)], None), result),
                (Pat::Wild, unchanged),
            ],
        )
    }

    fn dictionary(&mut self, desc: Expr) -> Expr {
        let spine = self.call(Op::Spine, desc, self.list(Vec::new()), self.unit());
        let head = self.name();
        let args = self.name();
        let result = self.nodes_with_default(
            self.local(&head),
            |this, _| this.boolean(true),
            |this, _, node, env| {
                let (module, name) = match &node.kind {
                    OwnerTypeKind::Con { module, name } => {
                        if [Lang::Outlives, Lang::Provided, Lang::Detached, Lang::TypeEq]
                            .into_iter()
                            .any(|class| lang::is(class, module, name))
                        {
                            return Some(this.boolean(false));
                        }
                        if module != "$held" {
                            return None;
                        }
                        let witness = this.dictionary_witness(this.local(&args));
                        return Some(this.e(ExprKind::Not(Box::new(witness))));
                    }
                    OwnerTypeKind::DynamicCon => (
                        this.pure(env.clone(), this.string("$module")),
                        this.pure(env, this.string("$name")),
                    ),
                    _ => return None,
                };
                let mut passes = this.boolean(true);
                for item in [Lang::Outlives, Lang::Provided, Lang::Detached, Lang::TypeEq] {
                    let Some(item) = lang::table().find(item) else {
                        continue;
                    };
                    let is_class = this.both(
                        this.same(module.clone(), this.string(&item.module)),
                        this.same(name.clone(), this.string(&item.name)),
                    );
                    passes = this.both(this.e(ExprKind::Not(Box::new(is_class))), passes);
                }
                let witness = this.dictionary_witness(this.local(&args));
                let witness = this.both(this.same(module, this.string("$held")), witness);
                Some(this.both(passes, this.e(ExprKind::Not(Box::new(witness)))))
            },
        );
        self.unpack(spine, &[&head, &args], result)
    }

    fn dictionary_witness(&mut self, arguments: Expr) -> Expr {
        let first = self.name();
        let second = self.name();
        self.case(
            arguments,
            vec![
                (
                    Pat::List(vec![Pat::Bind(first.clone()), Pat::Bind(second)], None),
                    self.observe(Op::IsSymbol, self.local(&first)),
                ),
                (Pat::Wild, self.boolean(false)),
            ],
        )
    }
}
