// Current canonical solutions, projected through the retained occurrence view.

impl Store {
    /// Follows bound variables at the top of `t`.
    pub fn resolve(&self, t: &Type) -> Type {
        self.head(t).into_owned()
    }

    /// The application spine with bound head variables observed. Borrowed
    /// spines retain their original children; only a projected solution owns
    /// new children.
    pub(in crate::types) fn spine<'a>(
        &'a self,
        ty: &'a Type,
    ) -> (Cow<'a, Type>, Vec<Cow<'a, Type>>) {
        let mut head = self.head(ty);
        let mut arguments = Vec::new();
        loop {
            match head {
                Cow::Borrowed(Type::App {
                    head: function,
                    arg,
                }) => {
                    arguments.push(Cow::Borrowed(arg.as_ref()));
                    head = self.head(function);
                }
                Cow::Owned(Type::App {
                    head: function,
                    arg,
                }) => {
                    arguments.push(Cow::Owned(*arg));
                    head = Cow::Owned(self.head(&function).into_owned());
                }
                _ => break,
            }
        }
        arguments.reverse();
        (head, arguments)
    }

    /// Observe current solutions without copying unchanged heads.
    /// Contextual solutions project lazily into this occurrence.
    pub fn head<'a>(&'a self, t: &'a Type) -> Cow<'a, Type> {
        let mut head = Cow::Borrowed(t);
        loop {
            match head.as_ref() {
                Type::Meta(meta) => {
                    if meta.lexical().is_some() {
                        let Ok(mappings) = self.lexical_mappings(meta) else {
                            return head;
                        };
                        let MetaState::Bound(solution) = &self.metas[meta.root() as usize] else {
                            return head;
                        };
                        head = Cow::Owned(lexical::transport_type(solution, &mappings));
                    } else if let Some(root) = meta.legacy_root() {
                        // Empty stores may display foreign legacy IDs.
                        if self
                            .meta_contexts
                            .get(root as usize)
                            .is_some_and(|context| {
                                context.canonical.len() != 0
                                    || !context.lexical.canonical.is_empty()
                            })
                        {
                            return head;
                        }
                        match self.metas.get(root as usize) {
                            Some(MetaState::Bound(next)) => head = Cow::Borrowed(next),
                            Some(MetaState::Open { .. }) | None => return head,
                        }
                    } else {
                        let Ok((root, view)) = self.occurrence(&head) else {
                            return head;
                        };
                        let MetaState::Bound(solution) = &self.metas[root as usize] else {
                            return head;
                        };
                        let Ok(projected) = view.transport_checked(solution) else {
                            return head;
                        };
                        head = Cow::Owned(projected);
                    }
                }
                Type::Skolem(s) => match self.equal(*s) {
                    Some(next) => head = Cow::Borrowed(next),
                    None => return head,
                },
                _ => return head,
            }
        }
    }
}
