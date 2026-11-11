//! What matching a constructor of a GADT tells about the matched value's
//! type: the most general unifier of the type the constructor builds and the
//! value's type, as an outcome instead of bindings. The store keeps no undo
//! log, so this never binds a variable of it; the caller applies the result.

use std::collections::HashMap;

use crate::types::Checker;
use crate::types::kinds::Kind;
use crate::types::ty::{Effects, Type};

/// The outcome of matching a constructor against a value's type.
#[derive(Debug, Clone, PartialEq)]
pub enum Refined {
    /// The values of this type are never built by the constructor.
    Impossible,
    /// The constructor's variables `bound` to parts of the value's type, and
    /// what each rigid variable of the value's type `equals` in the branch.
    Unifier {
        bound: Vec<(String, Type)>,
        equals: Vec<(u32, Type)>,
    },
    /// A type that is still being inferred decides it.
    Unknown,
    /// The branch would learn an equality of regions or rows.
    Unsupported(Unsupported),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    Region,
    Row,
}

/// Why a unification step fails.
enum Stop {
    Impossible,
    Unknown,
    Unsupported(Unsupported),
}

struct Refiner<'c, 'a> {
    checker: &'c Checker<'a>,
    /// The constructor's own variables, by name.
    locals: &'c [String],
    bound: HashMap<String, Type>,
    equals: Vec<(u32, Type)>,
}

impl Checker<'_> {
    /// Unifies `built`, the type a constructor builds over its variables
    /// `locals`, with `scrutinee`, the type of the matched value.
    pub(in crate::types) fn refine(
        &self,
        locals: &[String],
        built: &Type,
        scrutinee: &Type,
    ) -> Refined {
        let mut r = Refiner {
            checker: self,
            locals,
            bound: HashMap::new(),
            equals: Vec::new(),
        };
        match r.unify(built, scrutinee) {
            Ok(()) => {
                let bound = locals
                    .iter()
                    .filter_map(|v| Some((v.clone(), r.full(r.bound.get(v)?))))
                    .collect();
                let equals = r.equals.iter().map(|(s, t)| (*s, r.full(t))).collect();
                Refined::Unifier { bound, equals }
            }
            Err(Stop::Impossible) => Refined::Impossible,
            Err(Stop::Unknown) => Refined::Unknown,
            Err(Stop::Unsupported(u)) => Refined::Unsupported(u),
        }
    }
}

impl Refiner<'_, '_> {
    fn is_local(&self, v: &str) -> bool {
        self.locals.iter().any(|l| l == v)
    }

    /// `t` without qualifiers, aliases and solved variables at its head.
    fn walk(&self, t: &Type) -> Type {
        let mut t = self.checker.whnf(t);
        loop {
            let next = match &t {
                Type::Var(v) => self.bound.get(v),
                Type::Skolem(s) => self.equals.iter().find(|(x, _)| x == s).map(|(_, t)| t),
                _ => None,
            };
            match next {
                Some(next) => t = self.checker.whnf(next),
                None => return t,
            }
        }
    }

    /// `t` with everything solved substituted.
    fn full(&self, t: &Type) -> Type {
        let t = self.walk(t);
        t.map_children(&mut |c| self.full(c))
    }

    fn occurs(&self, t: &Type, x: &Type) -> bool {
        self.full(t).contains(&mut |y| y == x)
    }

    fn rigid_kind(&self, s: u32) -> Option<&Kind> {
        self.checker
            .cx
            .regions
            .rigid_kinds
            .iter()
            .find(|(x, _)| *x == s)
            .map(|(_, k)| k)
    }

    /// Records that the rigid `s` is `t` in the branch.
    fn equate(&mut self, s: u32, t: &Type) -> Result<(), Stop> {
        match self.rigid_kind(s) {
            Some(Kind::Region) => return Err(Stop::Unsupported(Unsupported::Region)),
            Some(k) if k.is_row() => return Err(Stop::Unsupported(Unsupported::Row)),
            _ => {}
        }
        if self.occurs(t, &Type::Skolem(s)) {
            return Err(Stop::Impossible);
        }
        // a variable still being inferred outside would decide it
        let open = self.full(t).contains(&mut |y| matches!(y, Type::Meta(_)));
        if open {
            return Err(Stop::Unknown);
        }
        self.equals.push((s, t.clone()));
        Ok(())
    }

    fn unify(&mut self, a: &Type, b: &Type) -> Result<(), Stop> {
        let (a, b) = (self.walk(a), self.walk(b));
        match (&a, &b) {
            (Type::Error, _) | (_, Type::Error) => Ok(()),
            (Type::Var(x), Type::Var(y)) if x == y => Ok(()),
            (Type::Var(v), t) | (t, Type::Var(v)) if self.is_local(v) => {
                if self.occurs(t, &Type::Var(v.clone())) {
                    return Err(Stop::Impossible);
                }
                self.bound.insert(v.clone(), t.clone());
                Ok(())
            }
            (Type::Meta(x), Type::Meta(y)) if x == y => Ok(()),
            (Type::Meta(_), _) | (_, Type::Meta(_)) => Err(Stop::Unknown),
            (Type::Skolem(x), Type::Skolem(y)) if x == y => Ok(()),
            (Type::Skolem(x), Type::Skolem(y)) => {
                // the later variable is the one the branch learns about
                let (later, other) = if x > y { (*x, &b) } else { (*y, &a) };
                self.equate(later, other)
            }
            (Type::Skolem(s), t) | (t, Type::Skolem(s)) => self.equate(*s, t),
            (Type::Con { .. }, Type::Con { .. }) | (Type::Symbol(_), Type::Symbol(_)) => {
                if a == b {
                    Ok(())
                } else {
                    Err(Stop::Impossible)
                }
            }
            (Type::App { head: h1, arg: a1 }, Type::App { head: h2, arg: a2 }) => {
                self.unify(h1, h2)?;
                self.unify(a1, a2)
            }
            (Type::Tuple(xs), Type::Tuple(ys)) => {
                if xs.len() != ys.len() {
                    return Err(Stop::Impossible);
                }
                xs.iter().zip(ys).try_for_each(|(x, y)| self.unify(x, y))
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
                self.unify(p1, p2)?;
                self.unify(r1, r2)?;
                let store = &self.checker.store;
                let zonked = |e: &Option<Box<Effects>>| {
                    e.as_ref()
                        .map(|e| (store.zonk(&e.services), store.zonk(&e.errors), e.sync))
                };
                if zonked(e1) == zonked(e2) {
                    Ok(())
                } else {
                    Err(Stop::Unsupported(Unsupported::Row))
                }
            }
            (Type::Record(_) | Type::Row(_) | Type::Union(..), _)
            | (_, Type::Record(_) | Type::Row(_) | Type::Union(..)) => {
                if self.full(&a) == self.full(&b) {
                    Ok(())
                } else {
                    Err(Stop::Unsupported(Unsupported::Row))
                }
            }
            _ => Err(Stop::Impossible),
        }
    }
}
