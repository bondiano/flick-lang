impl Checker<'_> {
    /// Reads the metadata lines of `decl` into `info`; the retention, if
    /// written.
    fn read_meta(&mut self, decl: &Node, scheme: &Type) -> PendingForeign {
        let mut pending = PendingForeign {
            decl: decl.clone(),
            scheme: scheme.clone(),
            info: ForeignInfo::default(),
            retention: None,
            arity: Vec::new(),
            cancel: Vec::new(),
            catch: Vec::new(),
            span: self.span(decl),
            name: super::toplevel::foreign_name(decl, self.text)
                .unwrap_or_default()
                .into(),
            unknown: None,
        };
        let pure = foreign_words(decl)
            .0
            .is_some_and(|word| word.text(self.text) == "pure");
        let info = &mut pending.info;
        let mut retention = None;
        let meta: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::ForeignMeta)
            .flat_map(Node::nodes)
            .collect();
        for item in &meta {
            let Some(name) = item.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let args: Vec<&Node> = item.nodes().collect();
            let word = |i: usize| args.get(i).map(|n| n.text(self.text).trim().to_owned());
            let number = |i: usize| word(i).and_then(|w| w.parse::<usize>().ok());
            match name.text(self.text) {
                "arity" => {
                    info.arity = number(0);
                    pending.arity.push(((*item).clone(), info.arity));
                }
                "this" => info.adapter.receiver = true,
                "reviewed" if !word(0).is_some_and(|w| w.starts_with('"')) => self.bad_meta(
                    item,
                    "`reviewed` takes the package and fingerprint the binding was checked against, as a string",
                ),
                "nullable" => info.adapter.result.nullable = true,
                "variadic" => info.adapter.variadic = true,
                "optional" => match number(0) {
                    Some(i) => info.adapter.optional.push(i),
                    None => self.bad_meta(item, "`optional` takes the position of an `Option` argument, from 0"),
                },
                "cancel" => match word(0).as_deref() {
                    Some("signal") => {
                        info.adapter.cancel = true;
                        pending.cancel.push((*item).clone());
                    }
                    _ => self.bad_meta(item, "write `cancel signal`"),
                },
                "retention" => match word(0).as_deref() {
                    Some("call") => retention = Some(Retention::Call),
                    Some("owner") => retention = Some(Retention::Owner),
                    _ => self.bad_meta(item, "write `retention call` or `retention owner`"),
                },
                "catch" => match args.first() {
                    Some(mapping) if !pure => {
                        let error = self.infer_catch_mapping(mapping);
                        pending.catch.push(((*item).clone(), (*mapping).clone(), error));
                        info.catch = Some(mapping.range());
                    }
                    Some(_) => self.bad_meta(item, "a `foreign pure` call cannot throw: `catch` needs another mode"),
                    None => self.bad_meta(item, "`catch` takes the function that maps a JS exception to an error"),
                },
                _ => {}
            }
        }
        pending.retention = retention;
        pending
    }

    /// Checks the metadata in `info` against the parameters of the signature.
    fn check_meta(&mut self, info: &ForeignInfo, params: &[Type], body: &Type, span: Span) {
        let arity = info.js_arity();
        let stages = &info.stages;
        if arity == 0
            && !stages.is_empty()
            && !params.first().is_some_and(|p| p.is_lang(Lang::Unit))
        {
            self.error(
                codes::F001,
                span,
                "`arity 0` drops the first argument, which must be `()`".to_owned(),
            );
        }
        if info.adapter.receiver && arity == 0 {
            self.error(
                codes::F001,
                span,
                "`this` needs an argument to be the receiver".to_owned(),
            );
        }
        for &i in &info.adapter.optional {
            if !params
                .get(i)
                .is_some_and(|p| self.whnf(p).spine().0.is_lang(Lang::Option))
                || i >= arity
            {
                self.error(
                    codes::F001,
                    span,
                    format!("`optional {i}` names an argument of type `Option a`"),
                );
            }
        }
        if info.adapter.variadic
            && (arity == 0 || !self.whnf(&params[arity - 1]).spine().0.is_lang(Lang::List))
        {
            self.error(
                codes::F001,
                span,
                "`variadic` spreads a last argument of type `List a`".to_owned(),
            );
        }
        if info.adapter.result.nullable
            && !self
                .call_result(body, arity)
                .is_some_and(|r| self.whnf(&r).spine().0.is_lang(Lang::Option))
        {
            self.error(
                codes::F001,
                span,
                "`nullable` makes the result an `Option a`".to_owned(),
            );
        }
    }
}
