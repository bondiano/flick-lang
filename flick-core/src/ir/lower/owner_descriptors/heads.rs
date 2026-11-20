impl Build<'_> {
    fn head(&mut self, desc: Expr, stack: Expr, _state: Expr) -> Expr {
        let reduced = self.call(Op::Spine, desc, stack, self.unit());
        let current = self.name();
        let args = self.name();
        let result = self.nodes(self.local(&current), |this, _, node, env, current| {
            let unchanged = this.tuple(vec![current.clone(), this.local(&args)]);
            let decremented = this.unit();
            match &node.kind {
                OwnerTypeKind::Con { module, name } => {
                    let alias = this
                        .templates
                        .aliases
                        .iter()
                        .position(|alias| alias.module == *module && alias.name == *name);
                    if let Some(alias) = alias {
                        this.call(
                            Op::HeadAlias,
                            current,
                            this.local(&args),
                            this.tuple(vec![
                                this.int(this.head_groups[alias]),
                                this.int(this.templates.aliases[alias].body),
                            ]),
                        )
                    } else if lang::is(Lang::Errors, module, name) {
                        this.project_errors(this.local(&args), decremented, unchanged.clone())
                    } else {
                        unchanged.clone()
                    }
                }
                OwnerTypeKind::Captured { .. } | OwnerTypeKind::Bounded => this.call(
                    Op::Head,
                    this.child(node.children[0], &env),
                    this.local(&args),
                    this.unit(),
                ),
                _ => unchanged.clone(),
            }
        });
        self.unpack(reduced, &[&current, &args], result)
    }

    // Alias bodies differ, but their parameter environments depend only on the
    // formal names. Build that environment once per distinct parameter list.
    fn head_alias(&mut self, desc: &Expr, arguments: &Expr, data: Expr) -> Expr {
        let group = self.name();
        let body = self.name();
        let representatives = self.head_representatives.clone();
        let mut arms: Vec<_> = representatives
            .into_iter()
            .enumerate()
            .map(|(group, alias)| {
                let params = self.templates.aliases[alias].params.clone();
                let bindings: Vec<_> = params.iter().map(|_| self.name()).collect();
                let rest = self.name();
                let environment = self.environment(
                    params
                        .into_iter()
                        .zip(&bindings)
                        .map(|(param, name)| (param, self.local(name)))
                        .collect(),
                    None,
                );
                let next = self.call(
                    Op::Head,
                    self.tuple(vec![self.local(&body), environment]),
                    self.local(&rest),
                    self.unit(),
                );
                let unchanged = self.tuple(vec![desc.clone(), arguments.clone()]);
                (
                    Pat::Lit(Lit::Int(
                        i32::try_from(group).expect("alias environment group"),
                    )),
                    self.case(
                        arguments.clone(),
                        vec![
                            (
                                Pat::List(
                                    bindings.into_iter().map(Pat::Bind).collect(),
                                    Some(Box::new(Pat::Bind(rest))),
                                ),
                                next,
                            ),
                            (Pat::Wild, unchanged),
                        ],
                    ),
                )
            })
            .collect();
        arms.push((Pat::Wild, self.impossible()));
        self.unpack(data, &[&group, &body], self.case(self.local(&group), arms))
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
        let result = self.nodes(self.local(&head), |this, _, node, env, _| {
            let (module, name) = match &node.kind {
                OwnerTypeKind::Con { module, name } => {
                    if [
                        Lang::Outlives,
                        Lang::Provided,
                        Lang::Detached,
                        Lang::NoControl,
                        Lang::TypeEq,
                    ]
                        .into_iter()
                        .filter_map(|item| lang::table().find(item))
                        .any(|item| item.module == *module && item.name == *name)
                    {
                        return this.boolean(false);
                    }
                    if module != "$held" {
                        return this.boolean(true);
                    }
                    (this.string(module), this.string(name))
                }
                OwnerTypeKind::DynamicCon => (
                    this.pure(env.clone(), this.string("$module")),
                    this.pure(env, this.string("$name")),
                ),
                _ => return this.boolean(true),
            };
            let mut passes = this.boolean(true);
            for item in [
                Lang::Outlives,
                Lang::Provided,
                Lang::Detached,
                Lang::NoControl,
                Lang::TypeEq,
            ] {
                let Some(item) = lang::table().find(item) else {
                    continue;
                };
                let is_class = this.both(
                    this.same(module.clone(), this.string(&item.module)),
                    this.same(name.clone(), this.string(&item.name)),
                );
                passes = this.both(this.e(ExprKind::Not(Box::new(is_class))), passes);
            }
            let first = this.name();
            let second = this.name();
            let witness = this.case(
                this.local(&args),
                vec![
                    (
                        Pat::List(vec![Pat::Bind(first.clone()), Pat::Bind(second)], None),
                        this.observe(Op::IsSymbol, this.local(&first)),
                    ),
                    (Pat::Wild, this.boolean(false)),
                ],
            );
            let witness = this.both(this.same(module, this.string("$held")), witness);
            this.both(passes, this.e(ExprKind::Not(Box::new(witness))))
        });
        self.unpack(spine, &[&head, &args], result)
    }
}
