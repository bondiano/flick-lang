impl Checker<'_> {
    /// The same declaration head observation, retaining only alias provenance
    /// needed to prove that a finite owner-adapter tree has no execution cycle.
    pub(in crate::types) fn owner_head(&self, ty: &Type) -> (Type, HashSet<(String, String)>) {
        let mut aliases = HashSet::new();
        let mut observe = |module: &str, name: &str| {
            aliases.insert((module.to_owned(), name.to_owned()));
        };
        let mut head = self
            .borrowed_head_observing(ty, false, &mut observe)
            .into_owned();
        for _ in 0..64 {
            match head {
                Type::Captured { ty, .. } | Type::Bounded { ty, .. } => {
                    head = self
                        .borrowed_head_observing(&ty, false, &mut observe)
                        .into_owned();
                }
                _ => break,
            }
        }
        (head, aliases)
    }

    /// Whole-type capture walks must see an opaque alias exposed through a
    /// transparent head or a fixed projection before ordinary head reduction
    /// unfolds it. Transparent aliases are acyclic; an opaque alias ends this
    /// observation, so no depth budget or growing instantiated-type key is used.
    pub(in crate::types) fn projected_alias_key(&self, ty: &Type) -> Option<(String, String)> {
        self.alias_key(&self.projected_alias_type(ty)?)
    }

    pub(in crate::types) fn projected_alias_type(&self, ty: &Type) -> Option<Type> {
        let mut ty = self.store.head(ty);
        loop {
            let (head, args) = self.store.spine(&ty);
            let arguments: Vec<_> = args.iter().map(AsRef::as_ref).collect();
            if let Some(projected) = self.project(&head, &arguments) {
                ty = Cow::Owned(self.store.resolve(&projected));
                continue;
            }
            let Type::Con { module, name } = head.as_ref() else {
                return None;
            };
            let Some(Form::Alias {
                params,
                body,
                opaque,
            }) = self
                .env
                .type_def(module, name)
                .map(|definition| &definition.form)
            else {
                return None;
            };
            if args.len() < params.len() {
                return None;
            }
            if *opaque {
                // Canonicalize the head, not merely the outermost meta:
                // callers must observe args hidden in a bound HK head too.
                return Some(Type::apps(
                    head.into_owned(),
                    args.into_iter().map(Cow::into_owned),
                ));
            }
            let substitutions: Vec<_> = params
                .iter()
                .cloned()
                .zip(args.iter().map(|arg| arg.as_ref().clone()))
                .collect();
            let result = body.substitute(&substitutions);
            ty = Cow::Owned(self.store.resolve(&Type::apps(
                result,
                args[params.len()..].iter().map(|arg| arg.as_ref().clone()),
            )));
        }
    }

    /// Keep an opaque value nominal when solving an unknown monotype. This
    /// is shared with subsumption, before it observes the representation.
    pub(in crate::types) fn unify_opaque_meta(&mut self, a: &Type, b: &Type) -> Option<Unified> {
        let (a, b) = (self.store.head(a), self.store.head(b));
        match (a.as_ref(), b.as_ref()) {
            (Type::Meta(meta), other) | (other, Type::Meta(meta)) => {
                let meta = *meta;
                if !self.store.is_numeric(meta)
                    && !self.store.is_capture_row(meta)
                    && !self.store.is_error_row(meta)
                    && let Some(nominal) = self.projected_alias_type(other)
                    && self.opaque_value_type(&nominal)
                {
                    // Opaque values are monotypes even when their hidden
                    // representation is polymorphic. Do not accidentally
                    // instantiate/unfold that representation to bind a meta.
                    return Some(self.bind(meta, &nominal));
                }
            }
            _ => {}
        }
        None
    }

    fn opaque_value_type(&self, ty: &Type) -> bool {
        let (head, args) = ty.spine();
        let Type::Con { module, name } = head else {
            return false;
        };
        let Some(definition) = self.env.type_def(module, name) else {
            return false;
        };
        let mut kind = self.kinds.resolve(&definition.kind);
        for _ in args {
            let Kind::Arrow(_, result) = kind else {
                return false;
            };
            kind = self.kinds.resolve(&result);
        }
        kind == Kind::Type
    }
}
