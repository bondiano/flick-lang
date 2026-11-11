impl Checker<'_> {
    fn dictionary_type_name(&self, qualifier: &str) -> Option<(Type, Kind)> {
        if let Some((alias, member)) = qualifier.rsplit_once('.')
            && let Some(opened) = self.opened_modules.iter().rev().find(|m| m.alias == alias)
            && let Some((_, Type::Con { module, name }, kind)) =
                opened.types.iter().find(|(name, _, _)| name == member)
        {
            let mut result = kind;
            while let Kind::Arrow(_, inner) = result {
                result = inner;
            }
            if *result == Kind::Constraint {
                return Some((
                    Type::con(module, &format!("{name}$Dict")),
                    as_value_kind(kind.clone()),
                ));
            }
        }
        let local_class = self
            .contract_types
            .iter()
            .rev()
            .find(|(local, _, _)| local == qualifier)
            .map(|(_, ty, kind)| (ty.clone(), kind.clone()));
        let class = local_class.or_else(|| match self.env.find_type(qualifier) {
            Found::Entity(entity) => self
                .env
                .type_def(&entity.module, &entity.name)
                .map(|def| (Type::con(&entity.module, &entity.name), def.kind.clone())),
            _ => None,
        });
        match class? {
            (Type::Var(name), kind) => {
                Some((Type::Var(format!("{name}$Dict")), as_value_kind(kind)))
            }
            (Type::Con { module, name }, kind)
                if self
                    .env
                    .type_def(&module, &name)
                    .is_some_and(|def| matches!(def.form, Form::Class { .. })) =>
            {
                Some((
                    Type::con(&module, &format!("{name}$Dict")),
                    as_value_kind(kind),
                ))
            }
            _ => None,
        }
    }

    /// A type variable, `none`, or a type constructor by its (qualified) name.
    fn type_name(&mut self, node: &Node, free: Free) -> Option<(Type, Kind)> {
        let tokens: Vec<_> = node.child_tokens().filter(|t| t.kind != K::Dot).collect();
        let first = tokens.first()?;
        match first.kind {
            K::NoneKw => return Some((Type::empty_row(), self.kinds.fresh())),
            K::String => {
                let label = literal::string_value(first.text(self.text));
                return Some((Type::Symbol(label), Kind::Symbol));
            }
            K::Ident => {
                let name = first.text(self.text);
                if let Some(v) = self.cx.tyvars.iter().rev().find(|v| v.name == name) {
                    return Some((v.ty.clone(), v.kind.clone()));
                }
                let kind = self.kinds.fresh();
                let ty = match free {
                    Free::Quantify => Type::Var(name.to_owned()),
                    Free::Fresh => self.fresh(),
                    Free::Reject => {
                        self.error(
                            codes::T001,
                            self.span(node),
                            format!("unknown type variable `{name}`: declare it as a parameter"),
                        );
                        return None;
                    }
                };
                self.cx.tyvars.push(TyVar {
                    name: name.to_owned(),
                    ty: ty.clone(),
                    kind: kind.clone(),
                });
                return Some((ty, kind));
            }
            _ => {}
        }
        let parts: Vec<&str> = tokens.iter().map(|t| t.text(self.text)).collect();
        let (name, qualifier) = parts.split_last()?;
        let qualifier = qualifier.join(".");
        if *name == "Dict"
            && !qualifier.is_empty()
            && let Some(dictionary) = self.dictionary_type_name(&qualifier)
        {
            return Some(dictionary);
        }
        if let Some(opened) = self
            .opened_modules
            .iter()
            .rev()
            .find(|opened| opened.alias == qualifier)
        {
            if let Some((_, ty, kind)) = opened.types.iter().find(|(member, _, _)| member == name) {
                return Some((ty.clone(), kind.clone()));
            }
            self.error(
                codes::T001,
                self.span(node),
                format!("opened module `{qualifier}` has no type `{name}`"),
            );
            return None;
        }
        if qualifier.is_empty()
            && let Some((_, ty, kind)) = self
                .contract_types
                .iter()
                .rev()
                .find(|(local, _, _)| local == name)
        {
            return Some((ty.clone(), kind.clone()));
        }
        let (module, def) = self.find_type(node, &qualifier, name)?;
        let named = (module.clone(), (*name).to_owned());
        self.typing
            .type_names
            .insert((node.range().0, node.range().1), named);
        let ty = if let Some((_, service)) = self.service_module(&qualifier, name) {
            Type::con(&module, &format!("{service}.{name}"))
        } else {
            Type::con(&module, name)
        };
        match def.form {
            Form::Kind => {
                self.error(
                    codes::T002,
                    self.span(node),
                    format!("`{name}` is a kind, not a type"),
                );
                None
            }
            _ => Some((ty, def.kind)),
        }
    }

    /// The definition of a type-level name; `None` after a diagnostic or
    /// for a name from a module that could not be checked.
    pub fn find_type(
        &mut self,
        node: &Node,
        qualifier: &str,
        name: &str,
    ) -> Option<(String, super::env::TypeDef)> {
        let module = if qualifier.is_empty() {
            match self.env.find(Namespace::Type, name) {
                Found::Entity(e) => e.module,
                Found::Unknown => return None,
                Found::Missing => {
                    self.error_guess(
                        codes::T001,
                        self.name_span(node),
                        format!("unknown type `{name}`"),
                        |this| closest(name, this.env.names(Namespace::Type)).map(str::to_owned),
                    );
                    return None;
                }
            }
        } else if let Some((module, service)) = self.service_module(qualifier, name) {
            let def = self
                .env
                .type_def(&module, &format!("{service}.{name}"))?
                .clone();
            return Some((module, def));
        } else {
            let Some((_, interface)) = self.env.qualified_module(qualifier) else {
                self.error_guess(
                    codes::T001,
                    self.name_span(node),
                    format!("unknown module `{qualifier}`"),
                    |this| {
                        closest(qualifier, this.env.qualifier_names())
                            .map(|g| format!("{g}.{name}"))
                    },
                );
                return None;
            };
            let interface = interface?;
            let Some(export) = interface.find(Namespace::Type, name) else {
                self.error_guess(
                    codes::T001,
                    self.name_span(node),
                    format!("`{qualifier}` does not export type `{name}`"),
                    |this| {
                        let (_, Some(interface)) = this.env.qualified_module(qualifier)? else {
                            return None;
                        };
                        let types = interface
                            .exports
                            .iter()
                            .filter(|e| e.entity.namespace == Namespace::Type);
                        closest(name, types.map(|e| e.entity.name.as_str()))
                            .map(|g| format!("{qualifier}.{g}"))
                    },
                );
                return None;
            };
            export.entity.module.clone()
        };
        let def = self.env.type_def(&module, name)?.clone();
        Some((module, def))
    }

    /// The module and the name of the service `S` in `S.Impl` or `M.S.Impl`:
    /// a service in scope wins over a module of its name, as it does for
    /// `S.operation`.
    fn service_module(&self, qualifier: &str, name: &str) -> Option<(String, String)> {
        if name != "Impl" {
            return None;
        }
        let Found::Entity(entity) = self.env.find_type(qualifier) else {
            return None;
        };
        matches!(
            self.env
                .type_def(&entity.module, &entity.name)
                .map(|d| &d.form),
            Some(Form::Service { .. })
        )
        .then_some((entity.module, entity.name))
    }
}
