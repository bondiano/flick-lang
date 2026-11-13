//! Bounds on complete data and callable types. A bound on a projection
//! refines the declared field type; it does not create a hidden environment.

use std::rc::Rc;

use super::super::Checker;
use super::SCALARS;
use crate::syntax::Path;
use crate::types::{Type, Var};

impl Checker<'_> {
    /// Whether a value of `t` stores what its type does not show: a
    /// function, a computation, a cell, a type without constructors, or
    /// data with such a field (§1.2).
    pub(super) fn hidden(&self, t: &Type, seen: &mut Vec<Path>) -> bool {
        match self.resolve(t) {
            Type::Var(_) => false,
            Type::Tuple(ts) => ts.iter().any(|t| self.hidden(t, seen)),
            Type::Record(f) => f.fields.values().any(|t| self.hidden(t, seen)),
            Type::Forall(s) => self.hidden(&s.body, seen),
            t @ (Type::Con(_) | Type::App(..)) => {
                let mut args = Vec::new();
                let mut head = t;
                while let Type::App(h, a) = head {
                    args.push((*a).clone());
                    head = self.resolve(&h);
                }
                args.reverse();
                let Type::Con(p) = head else { return true };
                if p.module.as_ref() == "Prelude" && SCALARS.contains(&p.name.as_ref()) {
                    return false;
                }
                if p == Path::new("Prelude", "List") {
                    return args.iter().any(|t| self.hidden(t, seen));
                }
                if seen.contains(&p) {
                    return false;
                }
                let Some(fields) = self.fields_of(&p, &args) else {
                    return true;
                };
                seen.push(p);
                fields.iter().any(|t| self.hidden(t, seen))
            }
            _ => true,
        }
    }

    /// Data with hidden storage a signature writes without a qualifier
    /// (`regions.md` §1.4): a declared ADT whose values hide captures,
    /// through the data it holds, or a type without constructors its
    /// interface says hides them (an opaque ADT, a foreign `@stores` type).
    /// Another type without constructors gets no row: what a foreign
    /// binding can hide is G4's business, not elision's.
    pub(in crate::check) fn bare_hidden(&self, t: &Type) -> bool {
        let mut head = t;
        while let Type::App(h, _) = head {
            head = h;
        }
        let Type::Con(p) = head else { return false };
        self.decls.hidden.contains(p)
            || (self.decls.variants.contains_key(p)
                && !self.decls.errors.contains(p)
                && self.hidden(t, &mut Vec::new()))
    }

    /// The fields of every constructor of the declared type `p` at `args`;
    /// `None` for a type without constructors.
    pub(super) fn fields_of(&self, p: &Path, args: &[Type]) -> Option<Vec<Type>> {
        let variants = self.decls.variants.get(p)?;
        if self.decls.errors.contains(p) {
            return None;
        }
        let mut out = Vec::new();
        for (name, _) in variants {
            let s = self.decls.constructors.get(&Path {
                module: p.module.clone(),
                name: name.clone(),
            })?;
            let mut params = Vec::new();
            let mut t = &s.body;
            while let Type::Fun(x, _, r) = t {
                params.push((**x).clone());
                t = r;
            }
            let mut result = Vec::new();
            while let Type::App(h, a) = t {
                result.push((**a).clone());
                t = h;
            }
            result.reverse();
            let map = result
                .iter()
                .zip(args)
                .filter_map(|(v, a)| match v {
                    Type::Var(Var::Named(n)) => {
                        Some((n.clone(), super::super::Inst::Type(a.clone())))
                    }
                    _ => None,
                })
                .collect();
            out.extend(params.iter().map(|x| super::super::substitute(x, &map)));
        }
        Some(out)
    }

    /// (Proj): a part of a value of `(T ῡ)^C` taken apart has `C` too.
    pub(in crate::check) fn project_part(&self, whole: &Type, part: Type) -> Type {
        match self.follow(whole) {
            Type::Captured(t, c) | Type::Bounded(t, c)
                if !c.is_empty() && self.hidden(&t, &mut Vec::new()) =>
            {
                if matches!(part, Type::Captured(..) | Type::Bounded(..)) {
                    part
                } else {
                    Type::Bounded(Rc::new(part), c)
                }
            }
            _ => part,
        }
    }
}
