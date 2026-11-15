/// Rebase only identities in this lexical witness scope. Record field keys,
/// type variables, symbols and frozen label atoms are not witness identities.
struct WitnessScope<'a> {
    from: &'a str,
    mappings: &'a [(&'a str, &'a str)],
    to: &'a str,
    shadow: Option<&'a str>,
    changed: bool,
}

impl WitnessScope<'_> {
    fn reference(&mut self, value: &str, child_only: bool) -> String {
        let mappings = [(self.from, self.to)];
        let mappings = if self.mappings.is_empty() {
            &mappings[..]
        } else {
            self.mappings
        };
        let found = mappings
            .iter()
            .filter_map(|(from, to)| {
                let suffix = if !child_only && value == *from {
                    Some("")
                } else {
                    value.strip_prefix(from).filter(|s| s.starts_with('.'))
                };
                suffix.map(|suffix| (from.len(), *to, suffix))
            })
            .max_by_key(|(length, _, _)| *length);
        if let Some((_, to, suffix)) = found {
            self.changed = true;
            format!("{to}{suffix}")
        } else {
            value.into()
        }
    }

    fn module(&mut self, module: &str) -> String {
        self.reference(module, false)
    }

    fn kind(&mut self, kind: &Kind) -> Kind {
        match kind {
            Kind::Named { module, name } => Kind::Named {
                module: self.module(module),
                name: name.clone(),
            },
            Kind::Arrow(a, b) => Kind::arrow(self.kind(a), self.kind(b)),
            Kind::Row(inner) => Kind::Row(Box::new(self.kind(inner))),
            _ => kind.clone(),
        }
    }

    fn elem(&mut self, elem: &Elem) -> Elem {
        match elem {
            Elem::Named(label) => Elem::named(self.reference(label, true)),
            Elem::Provided(inner) => Elem::Provided(Box::new(self.elem(inner))),
            _ => elem.clone(),
        }
    }

    fn ty(&mut self, ty: &Type) -> Type {
        self.ty_in(ty, false)
    }

    fn ty_in(&mut self, ty: &Type, field_row: bool) -> Type {
        if let Type::Exists {
            scope,
            binder,
            contract,
            body,
        } = ty
        {
            // The new binder does not scope its contract.
            let contract = self.ty(contract);
            if self.shadow == Some(binder.as_str()) {
                return Type::Exists {
                    scope: *scope,
                    binder: binder.clone(),
                    contract: Box::new(contract),
                    body: body.clone(),
                };
            }
            let mut binder = binder.clone();
            let mut body = (**body).clone();
            if self.shadow.is_none()
                && self.from != self.to
                && self.to.strip_prefix("$exists:") == Some(binder.as_str())
                && witness_scope_occurs(&body, self.from)
            {
                // Capture-avoiding alpha conversion is deterministic, not a
                // checker-fresh identity persisted into the published type.
                let mut index = 0;
                let renamed = loop {
                    let candidate = format!("{binder}$Closed{index}");
                    let declared = body.contains(
                        &mut |ty| matches!(ty, Type::Exists { binder, .. } if binder == &candidate),
                    );
                    if !declared && !witness_scope_occurs(&body, &format!("$exists:{candidate}")) {
                        break candidate;
                    }
                    index += 1;
                };
                body = open_existential_type(&body, &binder, &format!("$exists:{renamed}"));
                binder = renamed;
            }
            let body = if self.mappings.is_empty() {
                self.ty(&body)
            } else {
                let closed = format!("$exists:{binder}");
                let mappings = self
                    .mappings
                    .iter()
                    .copied()
                    .filter(|(from, _)| *from != closed)
                    .collect::<Vec<_>>();
                let mut nested = WitnessScope {
                    from: self.from,
                    to: self.to,
                    mappings: &mappings,
                    shadow: self.shadow,
                    changed: false,
                };
                let body = nested.ty(&body);
                self.changed |= nested.changed;
                body
            };
            return Type::Exists {
                scope: *scope,
                binder,
                contract: Box::new(contract),
                body: Box::new(body),
            };
        }
        let mut mapped = match ty {
            Type::Record(row) | Type::Row(row) => {
                let record = matches!(ty, Type::Record(_));
                let row = super::Row {
                    entries: row
                        .entries
                        .iter()
                        .map(|(label, value)| (label.clone(), self.ty(value)))
                        .collect(),
                    tail: row
                        .tail
                        .as_ref()
                        .map(|tail| Box::new(self.ty_in(tail, record || field_row))),
                    written: row.written,
                };
                if record {
                    Type::Record(row)
                } else {
                    Type::Row(row)
                }
            }
            Type::Union(a, b) => Type::Union(
                Box::new(self.ty_in(a, field_row)),
                Box::new(self.ty_in(b, field_row)),
            ),
            Type::Without { row, gone } => Type::Without {
                row: Box::new(self.ty_in(row, field_row)),
                gone: gone.clone(),
            },
            _ => ty.map_children(&mut |child| self.ty(child)),
        };
        // A record tail stays a field row through its unions and omissions;
        // only its value types carry witness identities, not the field keys.
        if !field_row || !matches!(mapped, Type::Row(_) | Type::Without { .. }) {
            self.local(&mut mapped);
        }
        mapped
    }
    fn local(&mut self, mapped: &mut Type) {
        match mapped {
            Type::Con { module, .. } => *module = self.module(module),
            Type::Forall { vars, .. } => {
                for binder in vars {
                    binder.kind = self.kind(&binder.kind);
                }
            }
            Type::Row(row) => {
                for (label, _) in &mut row.entries {
                    *label = self.elem(label);
                }
                row.sort();
            }
            Type::Without { gone, .. } => {
                for label in gone.iter_mut() {
                    *label = self.elem(label);
                }
                gone.sort();
            }
            _ => {}
        }
    }
}

fn witness_local_scope_occurs(ty: &Type, witness: &str) -> bool {
    let mut scope = WitnessScope {
        from: witness,
        to: witness,
        mappings: &[],
        shadow: None,
        changed: false,
    };
    scope.local(&mut ty.clone());
    scope.changed
}

pub(in crate::types) fn witness_scope_occurs(ty: &Type, witness: &str) -> bool {
    let mut scope = WitnessScope {
        from: witness,
        to: witness,
        mappings: &[],
        shadow: None,
        changed: false,
    };
    scope.ty(ty);
    scope.changed
}

pub(super) fn open_existential_type(ty: &Type, binder: &str, witness: &str) -> Type {
    WitnessScope {
        from: &format!("$exists:{binder}"),
        to: witness,
        mappings: &[],
        shadow: Some(binder),
        changed: false,
    }
    .ty(ty)
}

#[cfg(test)]
pub(super) fn close_existential_type(ty: &Type, witness: &str, binder: &str) -> Type {
    WitnessScope {
        from: witness,
        to: &format!("$exists:{binder}"),
        mappings: &[],
        shadow: None,
        changed: false,
    }
    .ty(ty)
}
