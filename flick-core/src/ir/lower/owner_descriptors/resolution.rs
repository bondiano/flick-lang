#[derive(PartialEq, Eq)]
enum ResolveKey {
    Variable(String),
    Unchanged,
}

impl Build<'_> {
    fn resolve(&mut self, desc: Expr) -> Expr {
        self.nodes_grouped(
            desc,
            |_, node| match &node.kind {
                OwnerTypeKind::Var(name) if name.ends_with("$Dict") => None,
                OwnerTypeKind::Var(name) => Some(ResolveKey::Variable(name.clone())),
                _ => Some(ResolveKey::Unchanged),
            },
            |this, id, node, env, current| {
                let OwnerTypeKind::Var(name) = &node.kind else {
                    return current;
                };
                let found = this.pure(env.clone(), this.string(name));
                this.bind(found, |this, found| {
                    let fallback = if let Some(base) = name.strip_suffix("$Dict") {
                        let identity = this.pure(env.clone(), this.string(base));
                        this.bind(identity, |this, identity| {
                            let derived = this.derived_dictionary(identity.clone(), id);
                            this.choose(
                                this.binary(BinOp::Same, identity, this.unit()),
                                this.desc(id, this.root_env()),
                                derived,
                            )
                        })
                    } else {
                        current
                    };
                    this.choose(
                        this.binary(BinOp::Same, found.clone(), this.unit()),
                        fallback,
                        this.observe(Op::Resolve, found),
                    )
                })
            },
        )
    }

    fn derived_dictionary(&mut self, desc: Expr, fallback: usize) -> Expr {
        let resolved = self.observe(Op::Resolve, desc);
        self.bind(resolved, |this, resolved| {
            this.nodes(resolved, |this, _, node, env| {
                let (target, fields) = match &node.kind {
                    OwnerTypeKind::Con { module, name } => (
                        this.templates.dynamic_con,
                        vec![
                            ("$module".to_owned(), this.string(module)),
                            ("$name".to_owned(), this.string(&format!("{name}$Dict"))),
                        ],
                    ),
                    OwnerTypeKind::Var(name) => (
                        this.templates.dynamic_var,
                        vec![("$name".to_owned(), this.string(&format!("{name}$Dict")))],
                    ),
                    OwnerTypeKind::DynamicCon | OwnerTypeKind::DynamicVar => {
                        let name = this.binary(
                            BinOp::Concat,
                            this.pure(env.clone(), this.string("$name")),
                            this.string("$Dict"),
                        );
                        let mut fields = vec![("$name".to_owned(), name)];
                        let target = if matches!(node.kind, OwnerTypeKind::DynamicCon) {
                            fields.push((
                                "$module".to_owned(),
                                this.pure(env, this.string("$module")),
                            ));
                            this.templates.dynamic_con
                        } else {
                            this.templates.dynamic_var
                        };
                        (target, fields)
                    }
                    _ => return this.desc(fallback, this.root_env()),
                };
                let environment = this.environment(fields, None);
                this.desc(target, environment)
            })
        })
    }

    fn spine(&mut self, desc: Expr, stack: &Expr) -> Expr {
        self.bind(self.observe(Op::Resolve, desc), |this, resolved| {
            this.nodes_with_default(
                resolved,
                |this, current| this.tuple(vec![current, stack.clone()]),
                |this, _, node, env| match node.kind {
                    OwnerTypeKind::App => Some(this.call(
                        Op::Spine,
                        this.child(node.children[0], &env),
                        this.prepend(this.child(node.children[1], &env), stack.clone()),
                        this.unit(),
                    )),
                    _ => None,
                },
            )
        })
    }

    fn metadata(&mut self, op: Op, desc: Expr) -> Expr {
        self.nodes(desc, |this, _, node, env| {
            if matches!(op, Op::Tag) {
                return match &node.kind {
                    OwnerTypeKind::Con { module, name } => this.tuple(vec![
                        this.string("con"),
                        this.string(module),
                        this.string(name),
                    ]),
                    OwnerTypeKind::Var(name) => {
                        this.tuple(vec![this.string("var"), this.string(name)])
                    }
                    OwnerTypeKind::DynamicCon => this.tuple(vec![
                        this.string("con"),
                        this.pure(env.clone(), this.string("$module")),
                        this.pure(env, this.string("$name")),
                    ]),
                    OwnerTypeKind::DynamicVar => this.tuple(vec![
                        this.string("var"),
                        this.pure(env, this.string("$name")),
                    ]),
                    OwnerTypeKind::Fun { runtime, sync, .. } => this.tuple(vec![
                        this.string("fun"),
                        this.boolean(*runtime),
                        this.boolean(*sync),
                    ]),
                    OwnerTypeKind::Captured { value, .. } => {
                        this.tuple(vec![this.string("captured"), this.boolean(*value)])
                    }
                    OwnerTypeKind::MetaView(_) => unreachable!("validated owner templates"),
                    other => this.tuple(vec![this.string(&format!("{other:?}"))]),
                };
            }
            let env = if let OwnerTypeKind::Forall { vars, .. } = &node.kind {
                this.environment(
                    vars.iter()
                        .map(|var| (var.name.clone(), this.unit()))
                        .collect(),
                    Some(env),
                )
            } else {
                env
            };
            this.list(
                node.children
                    .iter()
                    .map(|&id| this.child(id, &env))
                    .collect(),
            )
        })
    }

    fn equal(&mut self, left: Expr, right: Expr) -> Expr {
        self.bind(self.observe(Op::Resolve, left), |this, left| {
            this.bind(this.observe(Op::Resolve, right), |this, right| {
                let tags = this.same(
                    this.observe(Op::Tag, left.clone()),
                    this.observe(Op::Tag, right.clone()),
                );
                let children = this.call(
                    Op::EqualChildren,
                    left.clone(),
                    this.observe(Op::Children, left),
                    this.observe(Op::Children, right),
                );
                this.both(tags, children)
            })
        })
    }

    fn equal_children(&mut self, desc: Expr, left: Expr, right: Expr) -> Expr {
        let a = self.name();
        let ax = self.name();
        let b = self.name();
        let bx = self.name();
        let success = self.both(
            self.call(Op::Equal, self.local(&a), self.local(&b), self.unit()),
            self.call(Op::EqualChildren, desc, self.local(&ax), self.local(&bx)),
        );
        self.case(
            self.tuple(vec![left, right]),
            vec![
                (
                    Pat::Tuple(vec![
                        Pat::List(Vec::new(), None),
                        Pat::List(Vec::new(), None),
                    ]),
                    self.boolean(true),
                ),
                (
                    Pat::Tuple(vec![
                        Pat::List(vec![Pat::Bind(a)], Some(Box::new(Pat::Bind(ax)))),
                        Pat::List(vec![Pat::Bind(b)], Some(Box::new(Pat::Bind(bx)))),
                    ]),
                    success,
                ),
                (Pat::Wild, self.boolean(false)),
            ],
        )
    }
}
