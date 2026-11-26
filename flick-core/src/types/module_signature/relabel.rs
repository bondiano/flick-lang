pub(in crate::types) fn relabel_contract_elem(elem: &Elem, value: &Type) -> Elem {
    match elem {
        Elem::Provided(inner) => Elem::Provided(Box::new(relabel_contract_elem(inner, value))),
        Elem::Service(_) | Elem::Error(_) | Elem::Capture(_) => match value.spine().0 {
            Type::Con { module, name } => Elem::Named(format!("{module}.{name}")),
            _ => match elem {
                Elem::Service(_) => super::ty::service_label(value).unwrap_or_else(|| elem.clone()),
                Elem::Error(_) => super::ty::error_label(value).unwrap_or_else(|| elem.clone()),
                Elem::Capture(_) => super::ty::capture_label(value).unwrap_or_else(|| elem.clone()),
                _ => unreachable!(),
            },
        },
        Elem::Named(_) | Elem::Live => elem.clone(),
    }
}

fn substitute_contract_elem(elem: &Elem, subst: &[(String, Type)]) -> Elem {
    match elem {
        Elem::Provided(inner) => Elem::Provided(Box::new(substitute_contract_elem(inner, subst))),
        Elem::Service(super::Atom::Var(name))
        | Elem::Error(super::Atom::Var(name))
        | Elem::Capture(super::Atom::Var(name)) => {
            if let Some((_, replacement)) = subst.iter().find(|(n, _)| n == name) {
                relabel_contract_elem(elem, replacement)
            } else {
                elem.clone()
            }
        }
        _ => elem.clone(),
    }
}

/// Substitutes contract/signature bindings and canonically relabels row keys
/// (`Elem::Service`/`Error`/`Capture`, `Provided`, and `Without` omissions)
/// once their bound atoms resolve to concrete nominal constructors.
pub(in crate::types) fn substitute_contract_type(ty: &Type, subst: &[(String, Type)]) -> Type {
    fn relabel(ty: &Type, subst: &[(String, Type)]) -> Type {
        match ty {
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
                    constraints: constraints.iter().map(|c| relabel(c, &inner)).collect(),
                    body: Box::new(relabel(body, &inner)),
                }
            }
            Type::Row(row) => {
                let mut entries: Vec<(Elem, Type)> = row
                    .entries
                    .iter()
                    .map(|(key, value)| {
                        let value = relabel(value, subst);
                        (relabel_contract_elem(key, &value), value)
                    })
                    .collect();
                super::Row::sort_entries(&mut entries);
                entries.dedup();
                Type::Row(super::Row {
                    entries,
                    tail: row.tail.as_deref().map(|t| Box::new(relabel(t, subst))),
                    written: row.written,
                })
            }
            Type::Without { row, gone } => {
                let mut gone: Vec<Elem> = gone
                    .iter()
                    .map(|elem| substitute_contract_elem(elem, subst))
                    .collect();
                gone.sort();
                gone.dedup();
                Type::Without {
                    row: Box::new(relabel(row, subst)),
                    gone,
                }
            }
            _ => ty.map_children(&mut |child| relabel(child, subst)),
        }
    }
    relabel(&ty.substitute(subst), subst)
}
