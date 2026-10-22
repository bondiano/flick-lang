//! Matching an instance head against a constraint, which binds only the
//! variables of the head, and unifying two heads, which coherence needs.
//! Both work on canonical types.

use super::has_metas;
use crate::types::ty::{Row, Type};

/// Whether a head matches; `Unknown` when only unknown types in the
/// constraint stand in the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    Yes,
    Unknown,
    No,
}

impl Match {
    pub(super) fn and(self, next: impl FnOnce() -> Match) -> Match {
        match self {
            Match::No => Match::No,
            Match::Yes => next(),
            Match::Unknown if next() == Match::No => Match::No,
            Match::Unknown => Match::Unknown,
        }
    }
}

pub type Subst = Vec<(String, Type)>;

/// Binds the variables of `pattern` so that it equals `target`.
pub fn match_type(pattern: &Type, target: &Type, subst: &mut Subst) -> Match {
    match (pattern, target) {
        (_, Type::Error) => Match::Yes,
        (Type::Var(v), _) => match subst.iter().find(|(n, _)| n == v) {
            Some((_, bound)) if bound == target => Match::Yes,
            Some((_, bound)) if has_metas(bound) || has_metas(target) => Match::Unknown,
            Some(_) => Match::No,
            None => {
                subst.push((v.clone(), target.clone()));
                Match::Yes
            }
        },
        (_, Type::Meta(_)) => Match::Unknown,
        (Type::App { head: h1, arg: a1 }, Type::App { head: h2, arg: a2 }) => {
            match_type(h1, h2, subst).and(|| match_type(a1, a2, subst))
        }
        (
            Type::Fun {
                param: p1,
                effects: e1,
                result: r1,
            },
            Type::Fun {
                param: p2,
                effects: e2,
                result: r2,
            },
        ) => {
            let effects = match (e1, e2) {
                (None, None) => Match::Yes,
                (Some(x), Some(y)) => match_type(&x.services, &y.services, subst)
                    .and(|| match_type(&x.errors, &y.errors, subst)),
                _ => Match::No,
            };
            effects
                .and(|| match_type(p1, p2, subst))
                .and(|| match_type(r1, r2, subst))
        }
        (Type::Tuple(x), Type::Tuple(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .fold(Match::Yes, |m, (a, b)| m.and(|| match_type(a, b, subst))),
        (Type::Record(p), Type::Record(t)) | (Type::Row(p), Type::Row(t)) => match_row(p, t, subst),
        _ if pattern == target => Match::Yes,
        _ => Match::No,
    }
}

/// Every labelled entry of the pattern is in the target; the pattern's tail
/// takes the rest.
fn match_row(pattern: &Row, target: &Row, subst: &mut Subst) -> Match {
    let open = matches!(target.tail.as_deref(), Some(Type::Meta(_)));
    let mut result = Match::Yes;
    for (label, p) in &pattern.entries {
        result = result.and(|| match target.get(label) {
            Some(t) => match_type(p, t, subst),
            None if open => Match::Unknown,
            None => Match::No,
        });
    }
    let rest: Vec<(String, Type)> = target
        .entries
        .iter()
        .filter(|(l, _)| pattern.get(l).is_none())
        .cloned()
        .collect();
    result.and(|| match pattern.tail.as_deref() {
        None if !rest.is_empty() => Match::No,
        None if open => Match::Unknown,
        None if target.tail.is_some() => Match::No,
        None => Match::Yes,
        Some(tail) => match (&rest[..], target.tail.as_deref()) {
            ([], Some(t)) => match_type(tail, t, subst),
            _ => match_type(
                tail,
                &Type::Row(Row {
                    entries: rest,
                    tail: target.tail.clone(),
                }),
                subst,
            ),
        },
    })
}

/// The variables of a head renamed apart from those of another.
pub fn rename(t: &Type, vars: &[String]) -> Type {
    let subst: Subst = vars
        .iter()
        .map(|v| (v.clone(), Type::Var(format!("{v}'"))))
        .collect();
    t.substitute(&subst)
}

/// Whether two heads with disjoint variables describe a common constraint.
pub fn unify_heads(left: &Type, right: &Type, subst: &mut Subst) -> bool {
    let (a, b) = (walk(left, subst), walk(right, subst));
    match (&a, &b) {
        (Type::Var(x), Type::Var(y)) if x == y => true,
        (Type::Var(x), t) | (t, Type::Var(x)) => {
            if resolve(t, subst).contains(&mut |n| *n == Type::Var(x.clone())) {
                return false;
            }
            subst.push((x.clone(), t.clone()));
            true
        }
        (Type::App { head: h1, arg: a1 }, Type::App { head: h2, arg: a2 }) => {
            unify_heads(h1, h2, subst) && unify_heads(a1, a2, subst)
        }
        (
            Type::Fun {
                param: p1,
                effects: e1,
                result: r1,
            },
            Type::Fun {
                param: p2,
                effects: e2,
                result: r2,
            },
        ) => {
            let effects = match (e1, e2) {
                (None, None) => true,
                (Some(x), Some(y)) => {
                    unify_heads(&x.services, &y.services, subst)
                        && unify_heads(&x.errors, &y.errors, subst)
                }
                _ => false,
            };
            effects && unify_heads(p1, p2, subst) && unify_heads(r1, r2, subst)
        }
        (Type::Tuple(xs), Type::Tuple(ys)) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys)
                    .all(|(one, other)| unify_heads(one, other, subst))
        }
        (Type::Record(xs), Type::Record(ys)) | (Type::Row(xs), Type::Row(ys)) => {
            unify_rows(xs, ys, subst)
        }
        _ => a == b,
    }
}

fn unify_rows(left: &Row, right: &Row, subst: &mut Subst) -> bool {
    let same = left.entries.len() == right.entries.len()
        && left
            .entries
            .iter()
            .zip(&right.entries)
            .all(|(one, other)| one.0 == other.0);
    if !same {
        // shortcut: rows of different labels overlap when one is open
        return left.tail.is_some() || right.tail.is_some();
    }
    left.entries
        .iter()
        .zip(&right.entries)
        .all(|(one, other)| unify_heads(&one.1, &other.1, subst))
        && match (&left.tail, &right.tail) {
            (None, None) => true,
            (Some(one), Some(other)) => unify_heads(one, other, subst),
            _ => false,
        }
}

fn walk(t: &Type, subst: &Subst) -> Type {
    match t {
        Type::Var(v) => match subst.iter().find(|(n, _)| n == v) {
            Some((_, bound)) => walk(bound, subst),
            None => t.clone(),
        },
        _ => t.clone(),
    }
}

/// `t` with every bound variable replaced, through chains of bindings.
pub fn resolve(t: &Type, subst: &Subst) -> Type {
    match walk(t, subst) {
        Type::Var(v) => Type::Var(v),
        other => other.map_children(&mut |c| resolve(c, subst)),
    }
}
