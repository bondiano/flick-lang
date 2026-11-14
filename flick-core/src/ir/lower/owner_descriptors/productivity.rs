impl Build<'_> {
    fn productive_summary(&mut self, desc: Expr) -> Expr {
        self.bind(self.observe(Op::Abstract, desc), |this, state| {
            this.abstract_boolean_field(state, true)
        })
    }

    /// P alone can forward through an identity template without recursively
    /// reconstructing each argument's full raw-head/row summary. Keep this a
    /// tail call so deeply nested finite identity aliases are stack safe.
    fn productive(&mut self, desc: Expr) -> Expr {
        // Keep the full abstract-summary switch in one dispatcher arm,
        // rather than cloning it into every original syntax-node case.
        let fallback = self.observe(Op::ProductiveSummary, desc.clone());
        let spine = self.call(Op::Spine, desc, self.list(Vec::new()), self.unit());
        let head = self.name();
        let arguments = self.name();
        let result = self.nodes(self.local(&head), |this, _, node, _| {
            let OwnerTypeKind::Con { module, name } = &node.kind else {
                return fallback.clone();
            };
            let Some(alias) = this
                .templates
                .aliases
                .iter()
                .find(|alias| alias.module == *module && alias.name == *name)
            else {
                return fallback.clone();
            };
            let OwnerTypeKind::Var(parameter) = &this.templates.exprs[alias.body].kind else {
                return fallback.clone();
            };
            let Some(index) = alias.params.iter().position(|name| name == parameter) else {
                return fallback.clone();
            };
            let bindings: Vec<_> = alias.params.iter().map(|_| this.name()).collect();
            let forwarded = this.observe(Op::Productive, this.local(&bindings[index]));
            this.case(
                this.local(&arguments),
                vec![
                    (
                        Pat::List(bindings.into_iter().map(Pat::Bind).collect(), None),
                        forwarded,
                    ),
                    (Pat::Wild, fallback.clone()),
                ],
            )
        });
        self.unpack(spine, &[&head, &arguments], result)
    }
}
