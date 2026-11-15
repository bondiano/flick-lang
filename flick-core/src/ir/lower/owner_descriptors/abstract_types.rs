#[derive(PartialEq, Eq)]
struct AbstractAliasKey {
    arity: usize,
    rows: Vec<(Vec<usize>, usize)>,
}

impl Build<'_> {
    fn abstract_alias_key(&self, alias: usize) -> AbstractAliasKey {
        AbstractAliasKey {
            arity: self.templates.aliases[alias].params.len(),
            rows: self
                .certificate()
                .aliases
                .iter()
                .filter(|row| row.alias == alias)
                .map(|row| (row.arguments.clone(), row.value))
                .collect(),
        }
    }
    fn abstract_named(&self, kind: u8, name: usize, held: bool, constructor: u8) -> Expr {
        let mut value = self.abstract_blank();
        value.head.kind = kind;
        value.head.name = name;
        value.head.held = held;
        value.constructor = constructor;
        self.abstract_id(&value)
    }
    fn abstract_name(&self, kind: u8, text: Expr, held: Expr) -> Expr {
        let mut arms: Vec<_> = self
            .certificate()
            .names
            .iter()
            .enumerate()
            .map(|(id, name)| {
                let yes = self.abstract_named(kind, id, kind == 1, 1);
                let no = self.abstract_named(kind, id, false, 1);
                (
                    Pat::Lit(Lit::String(name.clone())),
                    self.choose(held.clone(), yes, no),
                )
            })
            .collect();
        let id = self.certificate().names.len();
        arms.push((
            Pat::Wild,
            self.choose(
                held,
                self.abstract_named(kind, id, kind == 1, 1),
                self.abstract_named(kind, id, false, 1),
            ),
        ));
        self.case(text, arms)
    }
    fn abstract_opaque(&self, productive: bool) -> Expr {
        let mut value = self.abstract_blank();
        value.productive = productive;
        self.abstract_id(&value)
    }
    fn abstract_copy(&self, state: Expr, constructor: bool) -> Expr {
        self.case(
            state,
            self.certificate()
                .values
                .iter()
                .enumerate()
                .map(|(id, source)| {
                    let mut value = self.abstract_blank();
                    value.productive = source.productive;
                    if constructor {
                        value.constructor = source.constructor;
                    }
                    (
                        Pat::Lit(Lit::Int(i32::try_from(id).expect("abstract owner index"))),
                        self.abstract_id(&value),
                    )
                })
                .collect(),
        )
    }
    fn abstract_type(&mut self, desc: &Expr) -> Expr {
        let spine = self.call(Op::Spine, desc.clone(), self.list(Vec::new()), self.unit());
        let head = self.name();
        let arguments = self.name();
        let result = self.nodes_grouped(
            self.local(&head),
            |this, node| {
                let OwnerTypeKind::Con { module, name } = &node.kind else {
                    return None;
                };
                let alias = this
                    .templates
                    .aliases
                    .iter()
                    .position(|alias| alias.module == *module && alias.name == *name)?;
                Some(this.abstract_alias_key(alias))
            },
            |this, id, node, env, current| {
                if let OwnerTypeKind::Con { module, name } = &node.kind
                    && let Some(alias) = this
                        .templates
                        .aliases
                        .iter()
                        .position(|a| a.module == *module && a.name == *name)
                {
                    return this.abstract_alias(alias, current, this.local(&arguments));
                }
                let state = this.abstract_head(id, node, env);
                this.call(Op::AbstractApplyAll, current, state, this.local(&arguments))
            },
        );
        self.unpack(spine, &[&head, &arguments], result)
    }
    fn abstract_head(&mut self, id: usize, node: &OwnerTypeExpr, env: Expr) -> Expr {
        match &node.kind {
            OwnerTypeKind::Con { module, name } => {
                let label = format!("{module}.{name}");
                let id = self
                    .certificate()
                    .names
                    .binary_search(&label)
                    .unwrap_or(self.certificate().names.len());
                let constructor = if lang::is(Lang::Eff, module, name) {
                    2
                } else if lang::is(Lang::Errors, module, name) {
                    5
                } else {
                    1
                };
                self.abstract_named(1, id, module == "$held", constructor)
            }
            OwnerTypeKind::Var(name) => {
                let id = self
                    .certificate()
                    .names
                    .binary_search(name)
                    .unwrap_or(self.certificate().names.len());
                self.abstract_named(2, id, false, 1)
            }
            OwnerTypeKind::Atom {
                atom: Some(name), ..
            } => {
                let id = self
                    .certificate()
                    .names
                    .binary_search(name)
                    .unwrap_or(self.certificate().names.len());
                self.abstract_named(3, id, false, 1)
            }
            OwnerTypeKind::DynamicVar => {
                self.abstract_name(2, self.pure(env, self.string("$name")), self.boolean(false))
            }
            OwnerTypeKind::DynamicCon => {
                let module = self.pure(env.clone(), self.string("$module"));
                let label = self.binary(
                    BinOp::Concat,
                    self.binary(BinOp::Concat, module.clone(), self.string(".")),
                    self.pure(env, self.string("$name")),
                );
                self.abstract_name(1, label, self.same(module, self.string("$held")))
            }
            OwnerTypeKind::Symbol(_) => {
                self.abstract_named(4, self.certificate().names.len(), false, 1)
            }
            OwnerTypeKind::Fun { .. } => self.abstract_opaque(true),
            OwnerTypeKind::Captured { .. } | OwnerTypeKind::Bounded => self.abstract_copy(
                self.observe(Op::Abstract, self.child(node.children[0], &env)),
                true,
            ),
            OwnerTypeKind::Forall { vars, constraints } => {
                let masked = self.environment(
                    vars.iter()
                        .map(|var| (var.name.clone(), self.unit()))
                        .collect(),
                    Some(env),
                );
                self.bind(masked, |this, env| {
                    let mut dictionaries = this.boolean(false);
                    for &constraint in &node.children[..*constraints] {
                        dictionaries = this.binary(
                            BinOp::Or,
                            dictionaries,
                            this.observe(Op::Dictionary, this.child(constraint, &env)),
                        );
                    }
                    let body = this.abstract_copy(
                        this.observe(Op::Abstract, this.child(node.children[*constraints], &env)),
                        false,
                    );
                    this.choose(dictionaries, this.abstract_opaque(true), body)
                })
            }
            OwnerTypeKind::Row { .. } | OwnerTypeKind::Union | OwnerTypeKind::Without(_) => {
                self.abstract_row(id, env)
            }
            _ => self.abstract_opaque(false),
        }
    }
    fn abstract_alias(&mut self, alias: usize, desc: Expr, arguments: Expr) -> Expr {
        let params = self.templates.aliases[alias].params.len();
        let rows: Vec<_> = self
            .certificate()
            .aliases
            .iter()
            .filter(|row| row.alias == alias)
            .cloned()
            .collect();
        let bindings: Vec<_> = (0..params).map(|_| self.name()).collect();
        let rest = self.name();
        let constant = rows
            .first()
            .filter(|first| rows.iter().all(|row| row.value == first.value))
            .map(|row| self.int(row.value));
        let state = if let Some(value) = constant {
            value
        } else {
            let args = self.tuple(
                bindings
                    .iter()
                    .map(|name| self.observe(Op::Abstract, self.local(name)))
                    .collect(),
            );
            let mut arms: Vec<_> = rows
                .iter()
                .map(|row| {
                    let pattern = Pat::Tuple(
                        row.arguments
                            .iter()
                            .map(|&id| {
                                Pat::Lit(Lit::Int(i32::try_from(id).expect("abstract owner index")))
                            })
                            .collect(),
                    );
                    (pattern, self.int(row.value))
                })
                .collect();
            arms.push((Pat::Wild, self.impossible()));
            self.case(args, arms)
        };
        let value = self.call(Op::AbstractApplyAll, desc, state, self.local(&rest));
        self.case(
            arguments,
            vec![
                (
                    Pat::List(
                        bindings.into_iter().map(Pat::Bind).collect(),
                        Some(Box::new(Pat::Bind(rest))),
                    ),
                    value,
                ),
                (Pat::Wild, self.impossible()),
            ],
        )
    }
}

impl Build<'_> {
    fn abstract_row(&mut self, id: usize, env: Expr) -> Expr {
        let desc = self.desc(id, env);
        let entries = self.name();
        let open = self.name();
        let initial = self.tuple(vec![
            self.boolean(false);
            self.certificate().labels.len() + 1
        ]);
        let bits = self.call(
            Op::AbstractEntries,
            desc.clone(),
            self.local(&entries),
            initial,
        );
        let ExprKind::Tuple(mut fields) = self.abstract_data(&self.abstract_blank()).kind else {
            unreachable!()
        };
        *fields.last_mut().expect("abstract row field") = bits;
        let packed = self.call(
            Op::AbstractPack,
            desc.clone(),
            self.tuple(fields),
            self.unit(),
        );
        self.unpack(self.observe(Op::Row, desc), &[&entries, &open], packed)
    }
    fn abstract_entries(&mut self, desc: Expr, entries: Expr, bits: Expr) -> Expr {
        let label = self.name();
        let value = self.name();
        let rest = self.name();
        let names: Vec<_> = (0..=self.certificate().labels.len())
            .map(|_| self.name())
            .collect();
        let labels = self.certificate().labels.clone();
        let updated: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let matches = if index < labels.len() {
                    self.same(self.local(&label), self.string(&labels[index]))
                } else {
                    let known = labels.iter().fold(self.boolean(false), |found, known| {
                        self.binary(
                            BinOp::Or,
                            found,
                            self.same(self.local(&label), self.string(known)),
                        )
                    });
                    self.e(ExprKind::Not(Box::new(known)))
                };
                let productive = self.bind(
                    self.observe(Op::Abstract, self.local(&value)),
                    |this, state| this.observe(Op::AbstractProductive, state),
                );
                self.binary(BinOp::Or, self.local(name), self.both(matches, productive))
            })
            .collect();
        let recur = self.call(
            Op::AbstractEntries,
            desc,
            self.local(&rest),
            self.tuple(updated),
        );
        let refs: Vec<_> = names.iter().map(String::as_str).collect();
        let next = self.unpack(bits.clone(), &refs, recur);
        self.case(
            entries,
            vec![
                (Pat::List(Vec::new(), None), bits),
                (
                    Pat::List(
                        vec![Pat::Tuple(vec![Pat::Bind(label), Pat::Bind(value)])],
                        Some(Box::new(Pat::Bind(rest))),
                    ),
                    next,
                ),
            ],
        )
    }
}
