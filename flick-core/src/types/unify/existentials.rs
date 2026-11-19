impl Store {
    /// `t` with the solutions of its bound variables closed.
    ///
    /// Solutions are canonical in their issued witness context: closing an
    /// enclosing binder must also close solutions that were unknown when the
    /// annotation was first constructed.
    fn zonk_exists(&self, t: &Type) -> Type {
        let Type::Exists {
            scope,
            binder,
            contract,
            body,
        } = t
        else {
            return t.clone();
        };
        let mut witnesses = HashSet::new();
        body.visit(&mut |ty| {
            if let Type::Meta(meta) = ty
                && let Some(meta) = meta.contextual()
                && let Some(witness) = meta.renaming().target().witness(scope.identity())
            {
                witnesses.insert(witness.to_owned());
            }
        });
        let mut body = self.zonk(body);
        for witness in witnesses {
            body = super::module_contract::close_existential_type(&body, &witness, binder);
        }
        Type::Exists {
            scope: *scope,
            binder: binder.clone(),
            contract: Box::new(self.zonk(contract)),
            body: Box::new(body),
        }
    }
}
