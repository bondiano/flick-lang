impl Type {
    /// Applies `f` to every direct child type.
    #[must_use]
    pub fn map_children(&self, f: &mut dyn FnMut(&Type) -> Type) -> Type {
        let row = |row: &Row, f: &mut dyn FnMut(&Type) -> Type| Row {
            entries: row.entries.iter().map(|(l, t)| (l.clone(), f(t))).collect(),
            tail: row.tail.as_ref().map(|t| Box::new(f(t))),
            written: row.written,
        };
        match self {
            Type::App { head, arg } => Type::app(f(head), f(arg)),
            Type::Fun {
                param,
                effects,
                result,
            } => Type::Fun {
                param: Box::new(f(param)),
                effects: effects.as_ref().map(|e| {
                    Box::new(Effects {
                        services: f(&e.services),
                        errors: f(&e.errors),
                        sync: e.sync,
                        direct: e.direct,
                        written: e.written,
                    })
                }),
                result: Box::new(f(result)),
            },
            Type::Tuple(items) => Type::Tuple(items.iter().map(&mut *f).collect()),
            Type::Record(r) => Type::Record(row(r, f)),
            Type::Row(r) => Type::Row(row(r, f)),
            Type::Union(a, b) => Type::Union(Box::new(f(a)), Box::new(f(b))),
            Type::Without { row, gone } => Type::Without {
                row: Box::new(f(row)),
                gone: gone.clone(),
            },
            Type::Captured {
                ty,
                captures,
                value,
                elided,
            } => Type::Captured {
                ty: Box::new(f(ty)),
                captures: Box::new(f(captures)),
                value: *value,
                elided: *elided,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(f(ty)),
                captures: Box::new(f(captures)),
            },
            Type::Forall {
                vars,
                constraints,
                body,
            } => Type::Forall {
                vars: vars.clone(),
                constraints: constraints.iter().map(&mut *f).collect(),
                body: Box::new(f(body)),
            },
            Type::Exists {
                scope,
                binder,
                contract,
                body,
            } => Type::Exists {
                scope: *scope,
                binder: binder.clone(),
                contract: Box::new(f(contract)),
                body: Box::new(f(body)),
            },
            Type::Con { .. }
            | Type::Var(_)
            | Type::Symbol(_)
            | Type::Meta(_)
            | Type::MetaView(_)
            | Type::Skolem(_)
            | Type::Error => self.clone(),
        }
    }

    /// Calls `f` on every direct child type.
    pub fn for_each_child(&self, f: &mut dyn FnMut(&Type)) {
        let row = |row: &Row, f: &mut dyn FnMut(&Type)| {
            row.entries.iter().for_each(|(_, t)| f(t));
            row.tail.iter().for_each(|t| f(t));
        };
        match self {
            Type::App { head, arg } => {
                f(head);
                f(arg);
            }
            Type::Fun {
                param,
                effects,
                result,
            } => {
                f(param);
                if let Some(e) = effects {
                    f(&e.services);
                    f(&e.errors);
                }
                f(result);
            }
            Type::Tuple(items) => items.iter().for_each(f),
            Type::Record(r) | Type::Row(r) => row(r, f),
            Type::Without { row, .. } => f(row),
            Type::Union(a, b)
            | Type::Captured {
                ty: a, captures: b, ..
            }
            | Type::Bounded { ty: a, captures: b } => {
                f(a);
                f(b);
            }
            Type::Forall {
                constraints, body, ..
            } => {
                constraints.iter().for_each(&mut *f);
                f(body);
            }
            Type::Exists { contract, body, .. } => {
                f(contract);
                f(body);
            }
            Type::Con { .. }
            | Type::Var(_)
            | Type::Symbol(_)
            | Type::Meta(_)
            | Type::MetaView(_)
            | Type::Skolem(_)
            | Type::Error => {}
        }
    }

    /// Calls `f` on every type inside, this one first.
    pub fn visit(&self, f: &mut dyn FnMut(&Type)) {
        f(self);
        self.for_each_child(&mut |t| t.visit(f));
    }

    /// Replaces the free variables named in `subst`; stops at a `forall`
    /// that rebinds a name.
    #[must_use]
    pub fn substitute(&self, subst: &[(String, Type)]) -> Type {
        self.substitute_inner(subst)
    }

    fn substitute_inner(&self, subst: &[(String, Type)]) -> Type {
        match self {
            Type::Exists {
                scope,
                binder,
                contract,
                body,
            } => {
                let copied = Scope::default();
                // Rename the original body before inserting substitutions:
                // free caller handles in replacements are not bound here.
                let body = body.copy_bound_handle(scope.identity(), copied.identity());
                Type::Exists {
                    scope: copied,
                    binder: binder.clone(),
                    contract: Box::new(contract.substitute_inner(subst)),
                    body: Box::new(body.substitute_inner(subst)),
                }
            }
            Type::Var(name) => {
                if let Some((_, ty)) = subst.iter().find(|(n, _)| n == name) {
                    return ty.copy_template();
                }
                if let Some(class) = name.strip_suffix("$Dict")
                    && let Some((_, identity)) = subst.iter().find(|(n, _)| n == class)
                {
                    return match identity {
                        Type::Con { module, name } => Type::con(module, &format!("{name}$Dict")),
                        Type::Var(name) => Type::Var(format!("{name}$Dict")),
                        _ => self.clone(),
                    };
                }
                self.clone()
            }
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let inner: Vec<(String, Type)> = subst
                    .iter()
                    .filter(|(n, _)| !vars.iter().any(|v| &v.name == n))
                    .cloned()
                    .collect();
                Type::Forall {
                    vars: vars.clone(),
                    constraints: constraints
                        .iter()
                        .map(|c| c.substitute_inner(&inner))
                        .collect(),
                    body: Box::new(body.substitute_inner(&inner)),
                }
            }
            _ => self.map_children(&mut |t| t.substitute_inner(subst)),
        }
    }
}
