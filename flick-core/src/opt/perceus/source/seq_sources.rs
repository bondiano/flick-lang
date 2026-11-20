impl SeqBuilder {
    fn seq_from(&self) -> Expr {
        let body = self.block(
            vec![Stmt::Var(Self::vname("i"), self.loc("n"))],
            self.at(ExprKind::ControlLoop {
                id: "$seq_from".into(),
                condition: None,
                iterable: false,
                body: Box::new(self.block(
                    vec![
                        Stmt::Expr(self.yield_(self.var("i"))),
                        Stmt::Assign(Self::vname("i"), self.add(self.var("i"), self.int(1))),
                    ],
                    self.unit(),
                )),
            }),
        );
        self.lam(&["n"], self.gen_lazy(body))
    }

    fn seq_range(&self, inclusive: bool) -> Expr {
        let cond = if inclusive {
            self.bin(BinOp::Le, self.var("i"), self.loc("end"))
        } else {
            self.bin(BinOp::Lt, self.var("i"), self.loc("end"))
        };
        let step_body = if inclusive {
            self.block(
                vec![Stmt::Expr(self.yield_(self.var("i")))],
                self.if_(
                    self.bin(BinOp::Eq, self.var("i"), self.loc("end")),
                    self.break_("$seq_range", self.unit()),
                    self.block(
                        vec![Stmt::Assign(
                            Self::vname("i"),
                            self.add(self.var("i"), self.int(1)),
                        )],
                        self.unit(),
                    ),
                ),
            )
        } else {
            self.block(
                vec![
                    Stmt::Expr(self.yield_(self.var("i"))),
                    Stmt::Assign(Self::vname("i"), self.add(self.var("i"), self.int(1))),
                ],
                self.unit(),
            )
        };
        let body = self.block(
            vec![Stmt::Var(Self::vname("i"), self.loc("start"))],
            self.at(ExprKind::ControlLoop {
                id: "$seq_range".into(),
                condition: Some(Box::new(cond)),
                iterable: false,
                body: Box::new(step_body),
            }),
        );
        self.lam(&["start", "end"], self.gen_lazy(body))
    }

    fn seq_range_by(&self) -> Expr {
        let pos_loop = self.at(ExprKind::Loop {
            condition: Box::new(self.bin(BinOp::Lt, self.var("i"), self.loc("until"))),
            body: Box::new(self.block(
                vec![
                    Stmt::Expr(self.yield_(self.var("i"))),
                    Stmt::Assign(Self::vname("i"), self.add(self.var("i"), self.loc("step"))),
                ],
                self.unit(),
            )),
        });
        let neg_loop = self.at(ExprKind::Loop {
            condition: Box::new(self.bin(BinOp::Gt, self.var("i"), self.loc("until"))),
            body: Box::new(self.block(
                vec![
                    Stmt::Expr(self.yield_(self.var("i"))),
                    Stmt::Assign(Self::vname("i"), self.add(self.var("i"), self.loc("step"))),
                ],
                self.unit(),
            )),
        });
        let body = self.block(
            vec![Stmt::Var(Self::vname("i"), self.loc("from"))],
            self.if_(
                self.bin(BinOp::Gt, self.loc("step"), self.int(0)),
                pos_loop,
                self.if_(
                    self.bin(BinOp::Lt, self.loc("step"), self.int(0)),
                    neg_loop,
                    self.unit(),
                ),
            ),
        );
        self.lam(&["from", "until", "step"], self.gen_lazy(body))
    }

    fn seq_iterate(&self) -> Expr {
        let body = self.block(
            vec![Stmt::Var(Self::vname("cur"), self.loc("x"))],
            self.at(ExprKind::ControlLoop {
                id: "$seq_iter".into(),
                condition: None,
                iterable: false,
                body: Box::new(self.block(
                    vec![
                        Stmt::Expr(self.yield_(self.var("cur"))),
                        Stmt::Assign(Self::vname("cur"), self.app(self.loc("f"), self.var("cur"))),
                    ],
                    self.unit(),
                )),
            }),
        );
        self.lam(&["f", "x"], self.gen_lazy(body))
    }

    fn seq_repeat(&self) -> Expr {
        let body = self.at(ExprKind::ControlLoop {
            id: "$seq_repeat".into(),
            condition: None,
            iterable: false,
            body: Box::new(self.yield_(self.loc("x"))),
        });
        self.lam(&["x"], self.gen_lazy(body))
    }

    fn seq_cycle(&self) -> Expr {
        let body = self.if_(
            self.bin(BinOp::Eq, self.loc("xs"), self.at(ExprKind::List(vec![]))),
            self.unit(),
            self.at(ExprKind::ControlLoop {
                id: "$seq_cycle".into(),
                condition: None,
                iterable: false,
                body: Box::new(self.each("x", self.loc("xs"), self.yield_(self.loc("x")))),
            }),
        );
        self.lam(&["xs"], self.gen_lazy(body))
    }

    fn seq_unfold(&self) -> Expr {
        let matched = self.at(ExprKind::Match(
            Box::new(self.app(self.loc("f"), self.var("state"))),
            vec![
                Arm {
                    pat: Self::some_pat(Pat::Tuple(vec![
                        Pat::Bind(Self::vname("val")),
                        Pat::Bind(Self::vname("next")),
                    ])),
                    guard: None,
                    body: self.block(
                        vec![
                            Stmt::Expr(self.yield_(self.loc("val"))),
                            Stmt::Assign(Self::vname("state"), self.loc("next")),
                        ],
                        self.unit(),
                    ),
                },
                Arm {
                    pat: Pat::Wild,
                    guard: None,
                    body: self.break_("$seq_unfold", self.unit()),
                },
            ],
        ));
        let body = self.block(
            vec![Stmt::Var(Self::vname("state"), self.loc("seed"))],
            self.at(ExprKind::ControlLoop {
                id: "$seq_unfold".into(),
                condition: None,
                iterable: false,
                body: Box::new(matched),
            }),
        );
        self.lam(&["f", "seed"], self.gen_lazy(body))
    }
}
