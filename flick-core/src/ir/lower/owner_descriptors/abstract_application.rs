impl Build<'_> {
    fn certificate(&self) -> &OwnerProductivity {
        self.templates
            .productivity
            .as_ref()
            .expect("recursive adapter certificate")
    }
    fn abstract_data(&self, value: &OwnerAbstract) -> Expr {
        self.tuple(vec![
            self.boolean(value.productive),
            self.int(usize::from(value.head.kind)),
            self.int(value.head.name),
            self.int(usize::from(value.head.arguments)),
            self.boolean(value.head.first_symbol),
            self.boolean(value.head.held),
            self.int(usize::from(value.constructor)),
            self.tuple(value.row.iter().map(|&bit| self.boolean(bit)).collect()),
        ])
    }
    fn abstract_pattern(value: &OwnerAbstract) -> Pat {
        let integer = |value: usize| {
            Pat::Lit(Lit::Int(
                i32::try_from(value).expect("abstract owner field"),
            ))
        };
        Pat::Tuple(vec![
            Pat::Lit(Lit::Bool(value.productive)),
            integer(usize::from(value.head.kind)),
            integer(value.head.name),
            integer(usize::from(value.head.arguments)),
            Pat::Lit(Lit::Bool(value.head.first_symbol)),
            Pat::Lit(Lit::Bool(value.head.held)),
            integer(usize::from(value.constructor)),
            Pat::Tuple(
                value
                    .row
                    .iter()
                    .map(|&bit| Pat::Lit(Lit::Bool(bit)))
                    .collect(),
            ),
        ])
    }
    fn impossible(&self) -> Expr {
        self.pure(self.local(&self.impossible), self.unit())
    }
    fn abstract_pack(&self, value: Expr) -> Expr {
        let mut arms: Vec<_> = self
            .certificate()
            .values
            .iter()
            .enumerate()
            .map(|(id, value)| (Self::abstract_pattern(value), self.int(id)))
            .collect();
        arms.push((Pat::Wild, self.impossible()));
        self.case(value, arms)
    }
    fn abstract_id(&self, value: &OwnerAbstract) -> Expr {
        let id = self
            .certificate()
            .values
            .iter()
            .position(|known| known == value)
            .expect("certificate includes every reachable abstract observation");
        self.int(id)
    }
    fn abstract_blank(&self) -> OwnerAbstract {
        OwnerAbstract {
            productive: false,
            head: OwnerRawHead {
                kind: 0,
                name: self.certificate().names.len(),
                arguments: 0,
                first_symbol: false,
                held: false,
            },
            row: vec![false; self.certificate().labels.len() + 1],
            constructor: 1,
        }
    }
    fn abstract_boolean_field(&self, state: Expr, productive: bool) -> Expr {
        self.case(
            state,
            self.certificate()
                .values
                .iter()
                .enumerate()
                .map(|(id, value)| {
                    let bit = if productive {
                        value.productive
                    } else {
                        value.row.iter().any(|bit| *bit)
                    };
                    (
                        Pat::Lit(Lit::Int(i32::try_from(id).expect("abstract owner index"))),
                        self.boolean(bit),
                    )
                })
                .collect(),
        )
    }
    fn abstract_apply_all(&mut self, desc: Expr, function: Expr, arguments: Expr) -> Expr {
        let head = self.name();
        let tail = self.name();
        let next = self.call(
            Op::AbstractApply,
            desc.clone(),
            function.clone(),
            self.local(&head),
        );
        let recur = self.call(Op::AbstractApplyAll, desc, next, self.local(&tail));
        self.case(
            arguments,
            vec![
                (Pat::List(Vec::new(), None), function),
                (
                    Pat::List(vec![Pat::Bind(head)], Some(Box::new(Pat::Bind(tail)))),
                    recur,
                ),
            ],
        )
    }
    fn abstract_apply(&mut self, function: Expr, argument: &Expr) -> Expr {
        let values = self.certificate().values.clone();
        let arms = values
            .iter()
            .enumerate()
            .map(|(id, source)| {
                let symbol = if source.head.held && source.head.arguments == 0 {
                    self.observe(Op::IsSymbol, argument.clone())
                } else {
                    self.boolean(source.head.first_symbol)
                };
                let productive = if source.constructor == 5 {
                    self.bind(
                        self.observe(Op::Abstract, argument.clone()),
                        |this, value| this.observe(Op::AbstractRowOccupied, value),
                    )
                } else {
                    self.boolean(source.constructor == 4)
                };
                let branch = |this: &Self, productive: bool, symbol: bool| {
                    let mut value = this.abstract_blank();
                    value.head = source.head.clone();
                    if value.head.held && value.head.arguments == 0 {
                        value.head.first_symbol = symbol;
                    }
                    value.head.arguments = (value.head.arguments + 1).min(3);
                    match source.constructor {
                        0 => value.constructor = 0,
                        2 => value.constructor = 3,
                        3 => value.constructor = 4,
                        4 => value.productive = true,
                        5 => value.productive = productive,
                        _ => {}
                    }
                    this.abstract_id(&value)
                };
                let yes = self.choose(
                    symbol.clone(),
                    branch(self, true, true),
                    branch(self, true, false),
                );
                let no = self.choose(
                    symbol,
                    branch(self, false, true),
                    branch(self, false, false),
                );
                (
                    Pat::Lit(Lit::Int(i32::try_from(id).expect("abstract owner index"))),
                    self.choose(productive, yes, no),
                )
            })
            .collect();
        self.case(function, arms)
    }
}
