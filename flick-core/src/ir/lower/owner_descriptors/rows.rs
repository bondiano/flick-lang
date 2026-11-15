#[derive(PartialEq, Eq)]
enum AtomKey {
    Named(String, bool),
    Unit,
}

impl Build<'_> {
    fn atom(&mut self, desc: Expr, mode: &Expr) -> Expr {
        let observed = self.choose(
            self.same(mode.clone(), self.int(2)),
            self.tuple(vec![
                self.observe(Op::Resolve, desc.clone()),
                self.list(Vec::new()),
            ]),
            self.call(Op::Spine, desc, self.list(Vec::new()), self.unit()),
        );
        self.bind(observed, |this, spine| {
            let head = this.name();
            let stack = this.name();
            let result = this.nodes_grouped(
                this.local(&head),
                |_, node| match &node.kind {
                    OwnerTypeKind::Con { module, name } => {
                        Some(AtomKey::Named(format!("{module}.{name}"), true))
                    }
                    OwnerTypeKind::Var(name)
                    | OwnerTypeKind::Atom {
                        atom: Some(name), ..
                    } => Some(AtomKey::Named(name.clone(), false)),
                    OwnerTypeKind::DynamicCon | OwnerTypeKind::DynamicVar => None,
                    _ => Some(AtomKey::Unit),
                },
                |this, _, node, env, _| {
                    let (name, constructor) = match &node.kind {
                        OwnerTypeKind::Con { module, name } => {
                            (this.string(&format!("{module}.{name}")), true)
                        }
                        OwnerTypeKind::DynamicCon => (
                            this.binary(
                                BinOp::Concat,
                                this.binary(
                                    BinOp::Concat,
                                    this.pure(env.clone(), this.string("$module")),
                                    this.string("."),
                                ),
                                this.pure(env, this.string("$name")),
                            ),
                            true,
                        ),
                        OwnerTypeKind::Var(name)
                        | OwnerTypeKind::Atom {
                            atom: Some(name), ..
                        } => (this.string(name), false),
                        OwnerTypeKind::DynamicVar => (this.pure(env, this.string("$name")), false),
                        _ => return this.unit(),
                    };
                    if constructor {
                        return this.choose(
                            this.same(mode.clone(), this.int(2)),
                            this.unit(),
                            name,
                        );
                    }
                    this.case(
                        mode.clone(),
                        vec![
                            (
                                Pat::Lit(Lit::Int(0)),
                                this.binary(BinOp::Concat, this.string("~"), name.clone()),
                            ),
                            (
                                Pat::Lit(Lit::Int(1)),
                                this.binary(BinOp::Concat, this.string("@"), name.clone()),
                            ),
                            (
                                Pat::Wild,
                                this.binary(BinOp::Concat, this.string("^"), name),
                            ),
                        ],
                    )
                },
            );
            this.unpack(spine, &[&head, &stack], result)
        })
    }

    fn row(&mut self, desc: Expr) -> Expr {
        self.bind(self.observe(Op::Resolve, desc), |this, resolved| {
            this.nodes_with_default(
                resolved,
                |this, _| this.tuple(vec![this.list(Vec::new()), this.boolean(true)]),
                |this, id, node, env| {
                    let current = this.desc(id, env.clone());
                    Some(match &node.kind {
                        OwnerTypeKind::Row { labels, tail } => {
                            let mut entries = Vec::new();
                            for (index, label) in labels.iter().enumerate() {
                                let value = this.child(node.children[index], &env);
                                let mode = match label {
                                    Elem::Error(_) => Some(0),
                                    Elem::Service(_) => Some(1),
                                    Elem::Capture(_) => Some(2),
                                    _ => None,
                                };
                                let label_expr = match mode {
                                    Some(mode) => {
                                        let observed = this.call(
                                            Op::Atom,
                                            value.clone(),
                                            this.int(mode),
                                            this.unit(),
                                        );
                                        this.bind(observed, |this, atom| {
                                            this.choose(
                                                this.binary(BinOp::Same, atom.clone(), this.unit()),
                                                this.string(&label.to_string()),
                                                atom,
                                            )
                                        })
                                    }
                                    None => this.string(&label.to_string()),
                                };
                                entries.push(this.tuple(vec![label_expr, value]));
                            }
                            let own = this.list(entries);
                            if *tail {
                                let tail = this.observe(
                                    Op::Row,
                                    this.child(*node.children.last().expect("row tail"), &env),
                                );
                                this.merge_row_results(
                                    current,
                                    this.tuple(vec![own, this.boolean(false)]),
                                    tail,
                                )
                            } else {
                                let entries =
                                    this.call(Op::Merge, current, own, this.list(Vec::new()));
                                this.tuple(vec![entries, this.boolean(false)])
                            }
                        }
                        OwnerTypeKind::Union => this.merge_row_results(
                            current,
                            this.observe(Op::Row, this.child(node.children[0], &env)),
                            this.observe(Op::Row, this.child(node.children[1], &env)),
                        ),
                        OwnerTypeKind::Without(gone) => {
                            let row = this.observe(Op::Row, this.child(node.children[0], &env));
                            let entries = this.name();
                            let open = this.name();
                            let filtered = this.call(
                                Op::Filter,
                                current,
                                this.local(&entries),
                                this.list(
                                    gone.iter()
                                        .map(|label| this.string(&label.to_string()))
                                        .collect(),
                                ),
                            );
                            this.unpack(
                                row,
                                &[&entries, &open],
                                this.tuple(vec![filtered, this.local(&open)]),
                            )
                        }
                        _ => return None,
                    })
                },
            )
        })
    }

    fn merge_row_results(&mut self, desc: Expr, left: Expr, right: Expr) -> Expr {
        let a = self.name();
        let ao = self.name();
        let b = self.name();
        let bo = self.name();
        let entries = self.call(Op::Merge, desc, self.local(&a), self.local(&b));
        let open = self.binary(BinOp::Or, self.local(&ao), self.local(&bo));
        self.unpack(
            left,
            &[&a, &ao],
            self.unpack(right, &[&b, &bo], self.tuple(vec![entries, open])),
        )
    }

    fn row_list(&mut self, op: Op, desc: Expr, x: Expr, y: Expr) -> Expr {
        let head = self.name();
        let tail = self.name();
        let pattern = Pat::List(
            vec![Pat::Bind(head.clone())],
            Some(Box::new(Pat::Bind(tail.clone()))),
        );
        let (empty, body, input) = match op {
            Op::Merge => {
                let inserted = self.call(Op::Insert, desc.clone(), self.local(&head), y.clone());
                (
                    y,
                    self.call(Op::Merge, desc, self.local(&tail), inserted),
                    x,
                )
            }
            Op::Insert => {
                let same = self.entry_equal(x.clone(), self.local(&head));
                let rest = self.call(Op::Insert, desc, x.clone(), self.local(&tail));
                (
                    self.list(vec![x]),
                    self.choose(same, y.clone(), self.prepend(self.local(&head), rest)),
                    y,
                )
            }
            Op::Filter => {
                let label = self.name();
                let value = self.name();
                let rest = self.call(Op::Filter, desc.clone(), self.local(&tail), y.clone());
                let found = self.call(Op::HasLabel, desc, y, self.local(&label));
                let selected =
                    self.choose(found, rest.clone(), self.prepend(self.local(&head), rest));
                (
                    self.list(Vec::new()),
                    self.unpack(self.local(&head), &[&label, &value], selected),
                    x,
                )
            }
            Op::HasLabel => (
                self.boolean(false),
                self.binary(
                    BinOp::Or,
                    self.same(self.local(&head), y.clone()),
                    self.call(op, desc, self.local(&tail), y),
                ),
                x,
            ),
            Op::HasEntry => {
                let label = self.name();
                let value = self.name();
                let body = self.binary(
                    BinOp::Or,
                    self.same(self.local(&label), y.clone()),
                    self.call(op, desc, self.local(&tail), y),
                );
                (
                    self.boolean(false),
                    self.unpack(self.local(&head), &[&label, &value], body),
                    x,
                )
            }
            _ => unreachable!("row-list operation"),
        };
        self.case(
            input,
            vec![(Pat::List(Vec::new(), None), empty), (pattern, body)],
        )
    }

    fn entry_equal(&mut self, left: Expr, right: Expr) -> Expr {
        let a = self.name();
        let av = self.name();
        let b = self.name();
        let bv = self.name();
        let equal = self.both(
            self.same(self.local(&a), self.local(&b)),
            self.call(Op::Equal, self.local(&av), self.local(&bv), self.unit()),
        );
        self.unpack(left, &[&a, &av], self.unpack(right, &[&b, &bv], equal))
    }

    fn scope(&mut self, desc: Expr) -> Expr {
        let row = self.observe(Op::Row, desc.clone());
        let entries = self.name();
        let open = self.name();
        let found = self.call(
            Op::HasEntry,
            desc,
            self.local(&entries),
            self.string(lang::label(Lang::Scope).expect("Scope service label")),
        );
        self.unpack(row, &[&entries, &open], found)
    }
}
