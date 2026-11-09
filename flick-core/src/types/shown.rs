//! The short form of a type, for a reader: without the outer `forall` and the
//! qualifiers of later stages, which a signature written that way gets back.
//!
//! It works without the environment, so a qualifier is left out only when the
//! kinds of the binders are enough to tell: what cannot be told stays shown.

use std::collections::BTreeSet;

use super::kinds::Kind;
use super::lang::Lang;
use super::regions::HOLD_NOTHING;
use super::ty::{Binder, Type};
use super::unify::Store;

/// The names a value may hold; `None` when the type does not tell.
type Atoms = Option<BTreeSet<String>>;

/// `t` as a reader sees a declared or inferred type. A type on its own that a
/// `forall` belongs to, as a parameter of rank 2, must not come here.
pub(super) fn short_form(t: &Type, store: &Store) -> Type {
    let none = Some(BTreeSet::new());
    match t {
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            let kinds: Vec<(String, Kind)> = vars
                .iter()
                .map(|v| (v.name.clone(), v.kind.clone()))
                .collect();
            let short = stages(body, &mut kinds.clone(), &none);
            let vars = if implicit(vars, constraints, body, store) {
                Vec::new()
            } else {
                vars.clone()
            };
            if vars.is_empty() && constraints.is_empty() {
                return short;
            }
            Type::Forall {
                vars,
                constraints: constraints.clone(),
                body: Box::new(short),
            }
        }
        other => stages(other, &mut Vec::new(), &none),
    }
}

/// Whether the signature without `forall` binds the same variables: each is
/// used outside a nested `forall`, and no other variable shows the same name.
fn implicit(vars: &[Binder], constraints: &[Type], body: &Type, store: &Store) -> bool {
    let mut used = BTreeSet::new();
    let mut rigid = BTreeSet::new();
    constraints
        .iter()
        .chain([body])
        .for_each(|t| names(t, store, &mut used, &mut rigid));
    let distinct: BTreeSet<&str> = vars.iter().map(|v| v.name.as_str()).collect();
    distinct.len() == vars.len()
        && vars
            .iter()
            .all(|v| used.contains(&v.name) && !rigid.contains(&v.name))
}

/// The variables of `t` outside a nested `forall`, and the rigid ones by the
/// name they are shown with.
fn names(t: &Type, store: &Store, used: &mut BTreeSet<String>, rigid: &mut BTreeSet<String>) {
    match t {
        Type::Var(name) => {
            used.insert(name.clone());
        }
        Type::Skolem(s) => {
            rigid.insert(store.skolem_name(*s).to_owned());
        }
        Type::Forall { .. } => {}
        other => {
            // only to walk the children; `visit` cannot stop at a nested `forall`
            let _ = other.map_children(&mut |child| {
                names(child, store, used, rigid);
                Type::Error
            });
        }
    }
}

/// `t` without the qualifiers `Checker::stages` would add back. Follows it,
/// with `held` what the parameters before this stage hold.
fn stages(t: &Type, kinds: &mut Vec<(String, Kind)>, held: &Atoms) -> Type {
    let none = Some(BTreeSet::new());
    match t {
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            let mark = kinds.len();
            kinds.extend(vars.iter().map(|v| (v.name.clone(), v.kind.clone())));
            let body = stages(body, kinds, held);
            kinds.truncate(mark);
            Type::Forall {
                vars: vars.clone(),
                constraints: constraints.clone(),
                body: Box::new(body),
            }
        }
        Type::Captured {
            ty,
            captures,
            value,
        } => Type::Captured {
            ty: Box::new(stages(ty, kinds, &row_atoms(captures))),
            captures: captures.clone(),
            value: *value,
        },
        Type::Bounded { ty, captures } => Type::Bounded {
            ty: Box::new(stages(ty, kinds, &row_atoms(captures))),
            captures: captures.clone(),
        },
        Type::Fun {
            param,
            effects,
            result,
        } => {
            let later = union(held, &retained(param, kinds));
            let result = match &**result {
                Type::Captured {
                    ty,
                    captures,
                    value: false,
                } if matches!(**ty, Type::Fun { .. }) && row_atoms(captures) == later => {
                    stages(ty, kinds, &later)
                }
                other => stages(other, kinds, &later),
            };
            Type::Fun {
                param: Box::new(stages(param, kinds, &none)),
                effects: effects.clone(),
                result: Box::new(result),
            }
        }
        other => other.map_children(&mut |child| stages(child, kinds, &none)),
    }
}

fn union(a: &Atoms, b: &Atoms) -> Atoms {
    let (a, b) = (a.as_ref()?, b.as_ref()?);
    Some(a.union(b).cloned().collect())
}

/// What a later stage holds of parameter `t`: its atoms of a kind that holds.
fn retained(t: &Type, kinds: &[(String, Kind)]) -> Atoms {
    let held = reach(t, kinds, false)?;
    Some(
        held.into_iter()
            .filter(|name| {
                matches!(
                    kind_of(name, kinds),
                    None | Some(
                        Kind::Type
                            | Kind::Arrow(..)
                            | Kind::Region
                            | Kind::CaptureRow
                            | Kind::ErrorRow
                    )
                )
            })
            .collect(),
    )
}

fn kind_of<'k>(name: &str, kinds: &'k [(String, Kind)]) -> Option<&'k Kind> {
    (kinds.iter().rev())
        .find(|(n, _)| n == name)
        .map(|(_, kind)| kind)
}

/// The atoms a capture row names.
pub(super) fn row_atoms(row: &Type) -> Atoms {
    match row {
        Type::Var(name) => Some(BTreeSet::from([name.clone()])),
        Type::Row(r) => {
            let mut out = BTreeSet::new();
            for (_, entry) in &r.entries {
                out.append(&mut row_atoms(entry)?);
            }
            if let Some(tail) = &r.tail {
                out.append(&mut row_atoms(tail)?);
            }
            Some(out)
        }
        Type::Union(a, b) => union(&row_atoms(a), &row_atoms(b)),
        _ => None,
    }
}

/// What `Checker::collect_bounded_captures` finds in `t`, when kinds tell. A
/// row given as an argument may be services, which hold nothing, so it does not.
// shortcut: an alias is taken for what it is applied to, so one that adds a
// qualifier of its own could hide a narrower one; the check over std covers it.
fn reach(t: &Type, kinds: &[(String, Kind)], bounded: bool) -> Atoms {
    match t {
        Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
            let mut row = row_atoms(captures)?;
            if bounded {
                row.retain(|name| matches!(kind_of(name, kinds), Some(Kind::Region)));
            }
            union(&Some(row), &reach(ty, kinds, true))
        }
        Type::Var(name) => {
            let holds = !bounded || matches!(kind_of(name, kinds), Some(Kind::Region));
            Some(if holds {
                BTreeSet::from([name.clone()])
            } else {
                BTreeSet::new()
            })
        }
        Type::Con { .. } | Type::Symbol(_) => Some(BTreeSet::new()),
        Type::App { .. } => {
            let (head, args) = t.spine();
            if HOLD_NOTHING.into_iter().any(|n| head.is_lang(n)) {
                return Some(BTreeSet::new());
            }
            // the services and the errors of an `Eff` are those of its stage
            let skip = if head.is_lang(Lang::Eff) { 2 } else { 0 };
            let mut out = BTreeSet::new();
            if let (Type::Var(name), false) = (head, bounded) {
                out.insert(name.clone());
            }
            for arg in args.into_iter().skip(skip) {
                out.append(&mut argument(arg, kinds, bounded)?);
            }
            Some(out)
        }
        Type::Tuple(items) => {
            let mut out = BTreeSet::new();
            for item in items {
                out.append(&mut reach(item, kinds, bounded)?);
            }
            Some(out)
        }
        Type::Record(row) => {
            let mut out = BTreeSet::new();
            for (_, field) in &row.entries {
                out.append(&mut reach(field, kinds, bounded)?);
            }
            Some(out)
        }
        Type::Fun { param, result, .. } => union(
            &reach(param, kinds, bounded),
            &reach(result, kinds, bounded),
        ),
        // what the `forall` binds is no capture of the value
        Type::Forall { vars, body, .. } => {
            let mut inner = kinds.to_vec();
            inner.extend(vars.iter().map(|v| (v.name.clone(), v.kind.clone())));
            let mut atoms = reach(body, &inner, bounded)?;
            atoms.retain(|name| !vars.iter().any(|v| v.name == *name));
            Some(atoms)
        }
        _ => None,
    }
}

/// What an argument of a type holds: a variable of a service row or a symbol
/// holds nothing, and an empty row holds nothing whatever its kind.
fn argument(arg: &Type, kinds: &[(String, Kind)], bounded: bool) -> Atoms {
    match arg {
        Type::Var(name)
            if matches!(kind_of(name, kinds), Some(Kind::ServiceRow | Kind::Symbol)) =>
        {
            Some(BTreeSet::new())
        }
        Type::Row(row)
            if row
                .entries
                .iter()
                .all(|(_, member)| matches!(member, Type::Con { .. } | Type::Symbol(_))) =>
        {
            match &row.tail {
                Some(tail) => argument(tail, kinds, bounded),
                None => Some(BTreeSet::new()),
            }
        }
        Type::Union(a, b) => union(&argument(a, kinds, bounded), &argument(b, kinds, bounded)),
        Type::Row(_) => None,
        other => reach(other, kinds, bounded),
    }
}

#[cfg(test)]
#[path = "shown_tests.rs"]
mod tests;
