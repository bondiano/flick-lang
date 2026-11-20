impl SeqBuilder {
    fn seq_append(&self) -> Expr {
        let body = self.block(
            vec![Stmt::Expr(self.each(
                "x",
                self.loc("a"),
                self.yield_(self.loc("x")),
            ))],
            self.each("x", self.loc("b"), self.yield_(self.loc("x"))),
        );
        self.lam(&["a", "b"], self.gen_lazy(body))
    }

    fn seq_map(&self) -> Expr {
        let body = self.each(
            "x",
            self.loc("s"),
            self.yield_(self.app(self.loc("f"), self.loc("x"))),
        );
        self.lam(&["f", "s"], self.gen_lazy(body))
    }

    fn seq_filter(&self) -> Expr {
        let body = self.each(
            "x",
            self.loc("s"),
            self.if_(
                self.app(self.loc("p"), self.loc("x")),
                self.yield_(self.loc("x")),
                self.unit(),
            ),
        );
        self.lam(&["p", "s"], self.gen_lazy(body))
    }

    fn seq_filter_map(&self) -> Expr {
        let body = self.each(
            "x",
            self.loc("s"),
            self.at(ExprKind::Match(
                Box::new(self.app(self.loc("f"), self.loc("x"))),
                vec![
                    Arm {
                        pat: Self::some_pat(Pat::Bind(Self::vname("y"))),
                        guard: None,
                        body: self.yield_(self.loc("y")),
                    },
                    Arm {
                        pat: Pat::Wild,
                        guard: None,
                        body: self.unit(),
                    },
                ],
            )),
        );
        self.lam(&["f", "s"], self.gen_lazy(body))
    }

    fn seq_flat_map(&self) -> Expr {
        let body = self.each(
            "x",
            self.loc("s"),
            self.each(
                "y",
                self.app(self.loc("f"), self.loc("x")),
                self.yield_(self.loc("y")),
            ),
        );
        self.lam(&["f", "s"], self.gen_lazy(body))
    }

    fn seq_take(&self) -> Expr {
        let loop_body = self.block(
            vec![
                Stmt::Expr(self.yield_(self.loc("x"))),
                Stmt::Assign(Self::vname("rem"), self.sub(self.var("rem"), self.int(1))),
            ],
            self.if_(
                self.bin(BinOp::Le, self.var("rem"), self.int(0)),
                self.break_("$seq_take", self.unit()),
                self.unit(),
            ),
        );
        let body = self.block(
            vec![Stmt::Var(Self::vname("rem"), self.loc("n"))],
            self.if_(
                self.bin(BinOp::Le, self.var("rem"), self.int(0)),
                self.unit(),
                self.for_ctrl("$seq_take", "x", self.loc("s"), loop_body),
            ),
        );
        self.lam(&["n", "s"], self.gen_lazy(body))
    }

    fn seq_take_while(&self) -> Expr {
        let body = self.for_ctrl(
            "$seq_tw",
            "x",
            self.loc("s"),
            self.if_(
                self.app(self.loc("p"), self.loc("x")),
                self.yield_(self.loc("x")),
                self.break_("$seq_tw", self.unit()),
            ),
        );
        self.lam(&["p", "s"], self.gen_lazy(body))
    }

    fn seq_drop(&self) -> Expr {
        let body = self.block(
            vec![Stmt::Var(Self::vname("rem"), self.loc("n"))],
            self.each(
                "x",
                self.loc("s"),
                self.if_(
                    self.bin(BinOp::Gt, self.var("rem"), self.int(0)),
                    self.block(
                        vec![Stmt::Assign(
                            Self::vname("rem"),
                            self.sub(self.var("rem"), self.int(1)),
                        )],
                        self.unit(),
                    ),
                    self.yield_(self.loc("x")),
                ),
            ),
        );
        self.lam(&["n", "s"], self.gen_lazy(body))
    }

    fn seq_drop_while(&self) -> Expr {
        let body = self.block(
            vec![Stmt::Var(Self::vname("dropping"), self.bool(true))],
            self.each(
                "x",
                self.loc("s"),
                self.if_(
                    self.var("dropping"),
                    self.if_(
                        self.app(self.loc("p"), self.loc("x")),
                        self.unit(),
                        self.block(
                            vec![Stmt::Assign(Self::vname("dropping"), self.bool(false))],
                            self.yield_(self.loc("x")),
                        ),
                    ),
                    self.yield_(self.loc("x")),
                ),
            ),
        );
        self.lam(&["p", "s"], self.gen_lazy(body))
    }

    fn seq_enumerate(&self) -> Expr {
        let body = self.block(
            vec![Stmt::Var(Self::vname("idx"), self.int(0))],
            self.each(
                "x",
                self.loc("s"),
                self.block(
                    vec![
                        Stmt::Let(Pat::Bind(Self::vname("i")), self.var("idx")),
                        Stmt::Assign(Self::vname("idx"), self.add(self.var("idx"), self.int(1))),
                    ],
                    self.yield_(self.at(ExprKind::Tuple(vec![self.loc("i"), self.loc("x")]))),
                ),
            ),
        );
        self.lam(&["s"], self.gen_lazy(body))
    }

    fn seq_scan(&self) -> Expr {
        let body = self.block(
            vec![
                Stmt::Var(Self::vname("acc"), self.loc("init")),
                Stmt::Expr(self.yield_(self.var("acc"))),
            ],
            self.each(
                "x",
                self.loc("s"),
                self.block(
                    vec![Stmt::Assign(
                        Self::vname("acc"),
                        self.app2(self.loc("f"), self.var("acc"), self.loc("x")),
                    )],
                    self.yield_(self.var("acc")),
                ),
            ),
        );
        self.lam(&["f", "init", "s"], self.gen_lazy(body))
    }

    fn seq_chunks(&self) -> Expr {
        let iter_body = self.block(
            vec![
                Stmt::Assign(
                    Self::vname("buf"),
                    self.bin(
                        BinOp::ListConcat,
                        self.var("buf"),
                        self.at(ExprKind::List(vec![self.loc("x")])),
                    ),
                ),
                Stmt::Assign(
                    Self::vname("count"),
                    self.add(self.var("count"), self.int(1)),
                ),
            ],
            self.if_(
                self.bin(BinOp::Eq, self.var("count"), self.loc("n")),
                self.block(
                    vec![
                        Stmt::Expr(self.yield_(self.var("buf"))),
                        Stmt::Assign(Self::vname("buf"), self.at(ExprKind::List(vec![]))),
                        Stmt::Assign(Self::vname("count"), self.int(0)),
                    ],
                    self.unit(),
                ),
                self.unit(),
            ),
        );
        let body = self.if_(
            self.bin(BinOp::Le, self.loc("n"), self.int(0)),
            self.unit(),
            self.block(
                vec![
                    Stmt::Var(Self::vname("buf"), self.at(ExprKind::List(vec![]))),
                    Stmt::Var(Self::vname("count"), self.int(0)),
                    Stmt::Expr(self.each("x", self.loc("s"), iter_body)),
                ],
                self.if_(
                    self.bin(BinOp::Gt, self.var("count"), self.int(0)),
                    self.yield_(self.var("buf")),
                    self.unit(),
                ),
            ),
        );
        self.lam(&["n", "s"], self.gen_lazy(body))
    }

    fn seq_windows(&self) -> Expr {
        let drop_head = self.at(ExprKind::Match(
            Box::new(self.var("buf")),
            vec![
                Arm {
                    pat: Pat::List(
                        vec![Pat::Wild],
                        Some(Box::new(Pat::Bind(Self::vname("rest")))),
                    ),
                    guard: None,
                    body: self.loc("rest"),
                },
                Arm {
                    pat: Pat::Wild,
                    guard: None,
                    body: self.at(ExprKind::List(vec![])),
                },
            ],
        ));
        let iter_body = self.block(
            vec![
                Stmt::Assign(
                    Self::vname("buf"),
                    self.bin(
                        BinOp::ListConcat,
                        self.var("buf"),
                        self.at(ExprKind::List(vec![self.loc("x")])),
                    ),
                ),
                Stmt::Assign(
                    Self::vname("count"),
                    self.add(self.var("count"), self.int(1)),
                ),
            ],
            self.if_(
                self.bin(BinOp::Eq, self.var("count"), self.loc("n")),
                self.block(
                    vec![
                        Stmt::Expr(self.yield_(self.var("buf"))),
                        Stmt::Assign(Self::vname("buf"), drop_head),
                        Stmt::Assign(
                            Self::vname("count"),
                            self.sub(self.var("count"), self.int(1)),
                        ),
                    ],
                    self.unit(),
                ),
                self.unit(),
            ),
        );
        let body = self.if_(
            self.bin(BinOp::Le, self.loc("n"), self.int(0)),
            self.unit(),
            self.block(
                vec![
                    Stmt::Var(Self::vname("buf"), self.at(ExprKind::List(vec![]))),
                    Stmt::Var(Self::vname("count"), self.int(0)),
                ],
                self.each("x", self.loc("s"), iter_body),
            ),
        );
        self.lam(&["n", "s"], self.gen_lazy(body))
    }

    fn seq_fold(&self, _effectful: bool) -> Expr {
        let body = self.block(
            vec![
                Stmt::Var(Self::vname("acc"), self.loc("init")),
                Stmt::Expr(self.each(
                    "x",
                    self.loc("s"),
                    self.block(
                        vec![Stmt::Assign(
                            Self::vname("acc"),
                            self.app2(self.loc("f"), self.var("acc"), self.loc("x")),
                        )],
                        self.unit(),
                    ),
                )),
            ],
            self.var("acc"),
        );
        self.lam(&["f", "init", "s"], body)
    }

    fn seq_quantifier(&self, is_any: bool) -> Expr {
        let init_val = !is_any;
        let stop_val = is_any;
        let pred = self.app(self.loc("p"), self.loc("x"));
        let hit = if is_any {
            pred
        } else {
            self.at(ExprKind::Not(Box::new(pred)))
        };
        let body = self.block(
            vec![
                Stmt::Var(Self::vname("found"), self.bool(init_val)),
                Stmt::Expr(self.for_ctrl(
                    "$seq_quant",
                    "x",
                    self.loc("s"),
                    self.if_(
                        hit,
                        self.block(
                            vec![Stmt::Assign(Self::vname("found"), self.bool(stop_val))],
                            self.break_("$seq_quant", self.unit()),
                        ),
                        self.unit(),
                    ),
                )),
            ],
            self.var("found"),
        );
        self.lam(&["p", "s"], body)
    }

    fn seq_find(&self) -> Expr {
        let body = self.block(
            vec![
                Stmt::Var(Self::vname("found"), self.none()),
                Stmt::Expr(self.for_ctrl(
                    "$seq_find",
                    "x",
                    self.loc("s"),
                    self.if_(
                        self.app(self.loc("p"), self.loc("x")),
                        self.block(
                            vec![Stmt::Assign(Self::vname("found"), self.some(self.loc("x")))],
                            self.break_("$seq_find", self.unit()),
                        ),
                        self.unit(),
                    ),
                )),
            ],
            self.var("found"),
        );
        self.lam(&["p", "s"], body)
    }

    fn seq_first(&self) -> Expr {
        let body = self.block(
            vec![
                Stmt::Var(Self::vname("found"), self.none()),
                Stmt::Expr(self.for_ctrl(
                    "$seq_first",
                    "x",
                    self.loc("s"),
                    self.block(
                        vec![Stmt::Assign(Self::vname("found"), self.some(self.loc("x")))],
                        self.break_("$seq_first", self.unit()),
                    ),
                )),
            ],
            self.var("found"),
        );
        self.lam(&["s"], body)
    }

    fn seq_each_effect(&self) -> Expr {
        let body = self.each("x", self.loc("s"), self.app(self.loc("f"), self.loc("x")));
        self.lam(&["f", "s"], body)
    }

    fn list_map_effect(&self) -> Expr {
        let body = self.at(ExprKind::Generate {
            body: Box::new(self.each(
                "x",
                self.loc("xs"),
                self.yield_(self.app(self.loc("f"), self.loc("x"))),
            )),
            strict: true,
            sync: false,
            limit: None,
        });
        self.lam(&["f", "xs"], body)
    }
}
