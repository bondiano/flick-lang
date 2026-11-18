//! Capture rows local to a callback. In `(a^v -> b)^c -> (f a)^d -> (f b)^c`
//! only the callback mentions `v`, and it bounds what the callback is given:
//! the callback is polymorphic in it, as if written `(forall v. a^v -> b)^c`.
//! Quantified over the whole signature instead, `v` would be the caller's
//! choice, and the body could give the callback nothing but a `v` it never has.

use std::mem;

use crate::types::kinds::Kind;
use crate::types::ty::{Binder, Type};

/// `ty`, a signature's type, with each capture row of `new` that a callback
/// owns (see [`owner`]) bound on that callback's arrow, inside its own
/// qualifier, in the order of `new`; and the rest of `new`.
pub(super) fn localize_callback_rows(mut ty: Type, mut new: Vec<Binder>) -> (Type, Vec<Binder>) {
    let (constraints, body) = match &mut ty {
        Type::Forall {
            constraints, body, ..
        } => (&constraints[..], &mut **body),
        other => (&[][..], other),
    };
    // from the last, each before those bound already: in the order of `new`
    for k in (0..new.len()).rev() {
        if let Some(param) = owner(constraints, body, &new[k]).and_then(|i| param_mut(body, i)) {
            bind(param, new.remove(k));
        }
    }
    (ty, new)
}

/// The parameter of `body`, a signature's type under `constraints`, whose
/// callback owns `binder`: a capture row that only that parameter's arrow
/// mentions, not on the parameter's own qualifier, and that bounds a
/// parameter of the arrow, as `a^v` or `a^(c + v)`.
pub(in crate::types) fn owner(constraints: &[Type], body: &Type, binder: &Binder) -> Option<usize> {
    let v = binder.name.as_str();
    if binder.kind != Kind::CaptureRow || constraints.iter().any(|c| occurs(c, v)) {
        return None;
    }
    let (mut t, mut i, mut found) = (body, 0, None);
    loop {
        match t {
            Type::Fun {
                param,
                effects,
                result,
            } => {
                if let Some(e) = effects
                    && (occurs(&e.services, v) || occurs(&e.errors, v))
                {
                    return None;
                }
                if occurs(param, v) {
                    if found.is_some() || !bounds_callback(param, v) {
                        return None;
                    }
                    found = Some(i);
                }
                (t, i) = (result, i + 1);
            }
            Type::Captured { ty, captures, .. } if matches!(**ty, Type::Fun { .. }) => {
                if occurs(captures, v) {
                    return None;
                }
                t = ty;
            }
            _ => return if occurs(t, v) { None } else { found },
        }
    }
}

/// Parameter `i` of the arrows of `t`, through the qualifiers of their results.
fn param_mut(t: &mut Type, i: usize) -> Option<&mut Type> {
    match t {
        Type::Fun { param, .. } if i == 0 => Some(param),
        Type::Fun { result, .. } => param_mut(result, i - 1),
        Type::Captured { ty, .. } => param_mut(ty, i),
        _ => None,
    }
}

/// The arrow of a parameter, inside its qualifier.
fn arrow_mut(param: &mut Type) -> &mut Type {
    match param {
        Type::Captured { ty, .. } => ty,
        other => other,
    }
}

/// Whether `param` is an arrow whose own qualifier does not mention `v`
/// and that takes a parameter `v` bounds.
fn bounds_callback(param: &Type, v: &str) -> bool {
    let mut t = match param {
        Type::Captured { captures, .. } if occurs(captures, v) => return false,
        Type::Captured { ty, .. } => ty,
        other => other,
    };
    if let Type::Forall { body, .. } = t {
        t = body;
    }
    loop {
        match t {
            Type::Fun { param, result, .. } => {
                if let Type::Captured { captures, .. } | Type::Bounded { captures, .. } = &**param
                    && occurs(captures, v)
                {
                    return true;
                }
                t = result;
            }
            Type::Captured { ty, .. } => t = ty,
            _ => return false,
        }
    }
}

/// Binds `binder` on the arrow of `param`, before the binders of its
/// `forall`, as a signature binds its implicit variables before its written
/// `forall`.
fn bind(param: &mut Type, binder: Binder) {
    let arrow = arrow_mut(param);
    if let Type::Forall { vars, .. } = arrow {
        return vars.insert(0, binder);
    }
    let body = mem::replace(arrow, Type::Error);
    *arrow = Type::Forall {
        vars: vec![binder],
        constraints: Vec::new(),
        body: Box::new(body),
    };
}

/// Whether `t` is a `forall` that binds `v`.
fn binds(t: &Type, v: &str) -> bool {
    matches!(t, Type::Forall { vars, .. } if vars.iter().any(|b| b.name == v))
}

/// Whether the free variable `v` occurs in `t`.
fn occurs(t: &Type, v: &str) -> bool {
    match t {
        Type::Var(name) => name == v,
        _ if binds(t, v) => false,
        _ => {
            let mut found = false;
            t.for_each_child(&mut |c| found = found || occurs(c, v));
            found
        }
    }
}

#[cfg(test)]
#[path = "callbacks_tests.rs"]
mod tests;
