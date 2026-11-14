//! What matching a constructor of a GADT tells about the matched value's
//! type: ordinary bindings plus equations constraining every residual row.
//! Idempotent row equations need not have a single most general unifier. The store keeps no undo
//! log, so this never binds a variable of it; the caller applies the result.

use std::collections::HashMap;

use super::row_facts::{self, Equation, RowKind};
use crate::types::Checker;
use crate::types::kinds::Kind;
use crate::types::regions::{AliasKey, Unfold, leave_alias};
use crate::types::ty::{Binder, Type};

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
        rows: Vec<Equation>,
    },
    /// A type that is still being inferred decides it.
    Unknown,
    /// The branch would learn an unsupported equality, such as regions.
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
    /// The constructor's own variables and their checked kinds.
    locals: &'c [Binder],
    bound: HashMap<String, Type>,
    equals: Vec<(u32, Type)>,
    rows: Vec<Equation>,
}

impl Checker<'_> {
    /// Unifies `built`, the type a constructor builds over its variables
    /// `locals`, with `scrutinee`, the type of the matched value.
    pub(in crate::types) fn refine(
        &self,
        locals: &[Binder],
        built: &Type,
        scrutinee: &Type,
    ) -> Refined {
        let mut r = Refiner {
            checker: self,
            locals,
            bound: HashMap::new(),
            equals: Vec::new(),
            rows: Vec::new(),
        };
        match r.unify(built, scrutinee) {
            Ok(()) => {
                let bound = locals
                    .iter()
                    .filter_map(|v| Some((v.name.clone(), r.full(r.bound.get(&v.name)?))))
                    .collect();
                let equals = r.equals.iter().map(|(s, t)| (*s, r.full(t))).collect();
                let rows: Vec<_> = r
                    .rows
                    .iter()
                    .map(|e| Equation {
                        kind: e.kind,
                        left: r.full(&e.left),
                        right: r.full(&e.right),
                        level: None,
                    })
                    .collect();
                if row_facts::consistent(self, &rows) {
                    Refined::Unifier {
                        bound,
                        equals,
                        rows,
                    }
                } else {
                    Refined::Impossible
                }
            }
            Err(Stop::Impossible) => Refined::Impossible,
            Err(Stop::Unknown) => Refined::Unknown,
            Err(Stop::Unsupported(u)) => Refined::Unsupported(u),
        }
    }
}

impl Refiner<'_, '_> {
    fn is_local(&self, v: &str) -> bool {
        self.locals.iter().any(|l| l.name == v)
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
        self.full_in(t, &mut Vec::new())
    }

    /// `path` holds the aliases it is unfolding: one met again inside its
    /// own unfolding stays as written, its arguments substituted.
    fn full_in(&self, t: &Type, path: &mut Vec<AliasKey>) -> Type {
        let unfold = self.checker.enter_alias(t, path);
        let full = match unfold {
            Unfold::Again => t.map_children(&mut |c| self.full_in(c, path)),
            _ => self.walk(t).map_children(&mut |c| self.full_in(c, path)),
        };
        leave_alias(path, unfold);
        full
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
            Some(Kind::Row(_)) => return Err(Stop::Unsupported(Unsupported::Row)),
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
        let kind = self.kind(&a).or_else(|| self.kind(&b));
        self.unify_at(&a, &b, kind.as_ref().and_then(RowKind::of))
    }

    fn kind(&self, ty: &Type) -> Option<Kind> {
        match ty {
            Type::Var(v) => self
                .locals
                .iter()
                .find(|l| l.name == *v)
                .map(|l| l.kind.clone()),
            Type::Skolem(s) => self.rigid_kind(*s).cloned(),
            Type::Con { module, name } => self
                .checker
                .env
                .type_def(module, name)
                .map(|d| d.kind.clone()),
            Type::App { head, .. } => match self.kind(head)? {
                Kind::Arrow(_, result) => Some(*result),
                _ => None,
            },
            _ => None,
        }
    }

    fn row(&mut self, a: &Type, b: &Type, kind: RowKind) -> Result<(), Stop> {
        if a == b || matches!(a, Type::Error) || matches!(b, Type::Error) {
            return Ok(());
        }
        if a.contains(&mut |t| matches!(t, Type::Meta(_)))
            || b.contains(&mut |t| matches!(t, Type::Meta(_)))
        {
            return Err(Stop::Unknown);
        }
        match (a, b) {
            (Type::Var(v), t) | (t, Type::Var(v))
                if self.is_local(v) && !self.occurs(t, &Type::Var(v.clone())) =>
            {
                self.bound.insert(v.clone(), t.clone());
                return Ok(());
            }
            (Type::Skolem(s), t) | (t, Type::Skolem(s)) if !self.occurs(t, &Type::Skolem(*s)) => {
                return self.equate(*s, t);
            }
            _ => {}
        }
        let left = self.checker.store.normalize(&self.full(a));
        let right = self.checker.store.normalize(&self.full(b));
        for (label, ty) in &left.entries {
            if let Some(other) = right.get(label) {
                self.unify(ty, other)?;
            }
        }
        self.rows.push(Equation {
            kind,
            left: a.clone(),
            right: b.clone(),
            level: None,
        });
        Ok(())
    }

    fn unify_at(&mut self, a: &Type, b: &Type, row: Option<RowKind>) -> Result<(), Stop> {
        let (a, b) = (self.walk(a), self.walk(b));
        if let Some(kind) = row {
            return self.row(&a, &b, kind);
        }
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
                let row = match self.kind(h1) {
                    Some(Kind::Arrow(param, _)) => RowKind::of(&param),
                    _ => None,
                };
                self.unify_at(a1, a2, row)
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
                match (e1, e2) {
                    (None, None) => Ok(()),
                    (Some(e1), Some(e2)) if e1.sync == e2.sync => {
                        self.unify_at(&e1.services, &e2.services, Some(RowKind::Services))?;
                        self.unify_at(&e1.errors, &e2.errors, Some(RowKind::Errors))
                    }
                    _ => Err(Stop::Unsupported(Unsupported::Row)),
                }
            }
            (Type::Record(_) | Type::Row(_) | Type::Union(..) | Type::Without { .. }, _)
            | (_, Type::Record(_) | Type::Row(_) | Type::Union(..) | Type::Without { .. }) => {
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
