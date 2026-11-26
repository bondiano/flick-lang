impl Lower<'_> {
    fn within(
        &mut self,
        owner: &str,
        call: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let owner = lookup(env, owner, span)?;
        let names: Vec<_> = free(call).into_iter().collect();
        let args = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<Vec<_>, _>>()?;
        let params = (0..args.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let inside = names.into_iter().zip(params.iter().copied()).collect();
        let mut helper = Code::new(params);
        let result = self.expr(call, &inside, &mut helper)?;
        let code = self.reserve(span)?;
        self.finish(code, &helper.finish(result))?;
        block.emit(Operation::Within { owner, code, args }, span)
    }

    fn owner_evidence(
        &mut self,
        owner: Local,
        call: &Expr,
        env: &Env,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let names: Vec<_> = free(call).into_iter().collect();
        let args = names
            .iter()
            .map(|name| lookup(env, name, span))
            .collect::<Result<Vec<_>, _>>()?;
        let params = (0..args.len())
            .map(|index| u32::try_from(index).map_err(|_| LowerError::Limit(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let inside = names.into_iter().zip(params.iter().copied()).collect();
        let mut helper = Code::new(params);
        let result = self.expr(call, &inside, &mut helper)?;
        let code = self.reserve(span)?;
        self.finish(code, &helper.finish(result))?;
        block.emit(Operation::WithOwnerEvidence { owner, code, args }, span)
    }
}
