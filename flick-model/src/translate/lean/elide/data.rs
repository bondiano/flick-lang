//! `T ā` in the core: the owner of a region, the capture row a value of data
//! with hidden storage hides, and a row given to a row parameter.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::{Env, caps, con, row, ty};
use crate::syntax::Path;
use crate::types::{Caps, Kind, Type};

/// Types with hidden storage: the qualifier of a value is their first
/// argument, as `appQ` of the scheme translation.
const STORED: [&str; 1] = ["Seq"];

/// What a certificate's writer knows of the data types of its signature.
#[derive(Default)]
pub(crate) struct Types {
    /// The kinds of the parameters of each type constructor.
    pub(crate) params: HashMap<Path, Vec<Kind>>,
    /// Data with a function field: as for `Seq`, the capture row a value
    /// hides is its first argument, `.caps Caps.none` for a bare `T`.
    pub(crate) stored: HashSet<Path>,
}

/// A field whose captures its type does not show: a function or a
/// computation (`docs/formal/regions.md` §1.2, hidden storage).
pub(crate) fn hides(t: &Type) -> bool {
    match t {
        Type::Captured(t, _) | Type::Bounded(t, _) => hides(t),
        t => matches!(t, Type::Fun(..) | Type::Eff(..)),
    }
}

/// The type `Κ : τ̄ → T ā` of a constructor of data with hidden storage at
/// the capture row `q` its value hides: each field that hides captures, and
/// the value, qualified by `q` (the model's (Con), `regions/mod.rs` `construct`).
pub(crate) fn hidden_at(t: &Type, q: &Caps) -> Type {
    match t {
        Type::Fun(p, s, r) => {
            let p = if hides(p) {
                Type::Captured(p.clone(), q.clone())
            } else {
                (**p).clone()
            };
            Type::Fun(Rc::new(p), s.clone(), Rc::new(hidden_at(r, q)))
        }
        t => Type::Captured(Rc::new(t.clone()), q.clone()),
    }
}

/// The capture row a value of `t` hides, as [`data`] writes it: the
/// qualifier nearest to the data.
pub(crate) fn hidden_by(t: &Type) -> Caps {
    match t {
        Type::Captured(x, _) | Type::Bounded(x, _)
            if matches!(**x, Type::Captured(..) | Type::Bounded(..)) =>
        {
            hidden_by(x)
        }
        Type::Captured(_, c) => c.clone(),
        _ => Caps::default(),
    }
}

/// `T ā` with the qualifier `c` of its value: the first argument of a type
/// with hidden storage, dropped by data without it.
pub(super) fn data(t: &Type, c: Option<&Caps>, env: &Env) -> Result<String, String> {
    let mut args = Vec::new();
    let mut head = t;
    while let Type::App(h, a) = head {
        args.push(&**a);
        head = h;
    }
    args.reverse();
    // the owner of a region is the core's own type
    if let (Type::Con(p), [s]) = (head, &args[..])
        && p.to_string() == "Fiber.Owner"
    {
        return Ok(format!("(.owner {})", ty(s, env)?));
    }
    let types = env.types.as_deref();
    let mut out = match head {
        Type::Con(p) => con(p),
        h => ty(h, env)?,
    };
    if let Type::Con(p) = head
        && (types.is_some_and(|ts| ts.stored.contains(p))
            || c.is_some() && STORED.contains(&&*p.name))
    {
        out = format!(
            "(.tapp {out} (.caps {}))",
            caps(c.unwrap_or(&Caps::default()), env)?
        );
    }
    let kinds = match head {
        Type::Con(p) => types.and_then(|ts| ts.params.get(p)),
        _ => None,
    };
    for (i, a) in args.into_iter().enumerate() {
        // a row parameter (`Failure e`) takes a row, as the core instantiates it
        let a = match (a, kinds.and_then(|k| k.get(i))) {
            (Type::Errors(r), Some(Kind::Errors | Kind::Services)) => {
                format!("(.row {})", row(r, env)?)
            }
            (a, _) => ty(a, env)?,
        };
        out = format!("(.tapp {out} {a})");
    }
    Ok(out)
}
