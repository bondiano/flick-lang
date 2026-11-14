//! Type heads, projections and equality.

use super::{Mismatch, Unified};
use crate::types::lang::{self, Lang};
use crate::types::{
    Checker,
    elem::Elem,
    env::Form,
    ty::{Row, Type},
};
use std::{borrow::Cow, ptr};

impl Checker<'_> {
    pub fn fresh(&mut self) -> Type {
        self.store.fresh()
    }

    pub fn zonk(&self, t: &Type) -> Type {
        self.store.zonk(t)
    }

    /// `t` with bound variables followed and aliases at its head expanded,
    /// without its capture qualifier.
    pub fn whnf(&self, t: &Type) -> Type {
        self.whnf_captured(t).0
    }

    /// [`Self::whnf`] without the copy of a type that does not reduce.
    pub fn borrowed_whnf<'a>(&'a self, t: &'a Type) -> Cow<'a, Type> {
        let mut t = self.borrowed_head(t);
        for _ in 0..64 {
            t = match t {
                Cow::Borrowed(Type::Captured { ty, .. } | Type::Bounded { ty, .. }) => {
                    self.borrowed_head(ty)
                }
                Cow::Owned(Type::Captured { ty, .. } | Type::Bounded { ty, .. }) => {
                    Cow::Owned(self.qualified_head(&ty))
                }
                other => return other,
            };
        }
        t
    }

    /// [`Self::whnf`], and the capture row of the qualifiers taken off.
    pub fn whnf_captured(&self, t: &Type) -> (Type, Option<Type>) {
        let mut t = self.qualified_head(t);
        let mut captures: Option<Type> = None;
        for _ in 0..64 {
            match t {
                Type::Captured {
                    ty, captures: c, ..
                }
                | Type::Bounded { ty, captures: c } => {
                    captures = Some(match captures {
                        Some(outer) => Type::Union(Box::new(outer), c),
                        None => *c,
                    });
                    t = self.qualified_head(&ty);
                }
                _ => break,
            }
        }
        (t, captures)
    }

    /// Head reduction without erasing the distinction between hidden
    /// captures and a refinement of a complete generic type.
    pub fn qualified_head(&self, t: &Type) -> Type {
        self.borrowed_head(t).into_owned()
    }

    /// [`Self::qualified_head`] without the copy of a type that does not
    /// reduce, which is most of what the capture summaries ask about.
    pub fn borrowed_head<'a>(&'a self, t: &'a Type) -> Cow<'a, Type> {
        let mut t = Cow::Borrowed(self.store.head(t));
        for _ in 0..64 {
            // only an application or a constructor has a head to reduce
            if !matches!(*t, Type::App { .. } | Type::Con { .. }) {
                break;
            }
            let (head, args) = t.spine();
            if let Some(reduced) = self.project(head, &args) {
                t = Cow::Owned(self.store.resolve(&reduced));
                continue;
            }
            let resolved = self.store.head(head);
            if !ptr::eq(resolved, head) && resolved != head {
                t = Cow::Owned(Type::apps(resolved.clone(), args.into_iter().cloned()));
                continue;
            }
            let Type::Con { module, name } = head else {
                break;
            };
            let Some(Form::Alias { params, body, .. }) =
                self.env.type_def(module, name).map(|d| &d.form)
            else {
                break;
            };
            if args.len() < params.len() {
                break;
            }
            let subst: Vec<(String, Type)> = params
                .iter()
                .cloned()
                .zip(args.iter().map(|a| (*a).clone()))
                .collect();
            t = Cow::Owned(self.store.resolve(&Type::apps(
                body.substitute(&subst),
                args[params.len()..].iter().map(|a| (*a).clone()),
            )));
        }
        t
    }

    /// The fixed projections of a row into a type, once the row is known
    /// well enough: `Errors none` is `Never`, `Errors {E}` is `E`, and
    /// `ServiceImpl {S}` is `S.Impl`; `Record.Field "x" T` is `{ x : T }`.
    fn project(&self, head: &Type, args: &[&Type]) -> Option<Type> {
        if let ([label, field], Type::Con { module, name }) = (args, head)
            && lang::is(Lang::Field, module, name)
            && let Type::Symbol(label) = self.store.resolve(label)
        {
            return Some(Type::Record(Row::closed(vec![(
                Elem::Named(label),
                (*field).clone(),
            )])));
        }
        let [row] = args[..] else { return None };
        if !head.is_lang(Lang::Errors) && !head.is_lang(Lang::ServiceImpl) {
            return None;
        }
        let norm = self.store.normalize(row);
        if !norm.tails.is_empty() || norm.entries.len() > 1 {
            return None;
        }
        let entry = norm.entries.into_iter().next().map(|(_, ty)| ty);
        if head.is_lang(Lang::Errors) {
            return Some(entry.unwrap_or_else(|| Type::lang(Lang::Never)));
        }
        if head.is_lang(Lang::ServiceImpl) {
            let service = entry?;
            let (head, args) = service.spine();
            let Type::Con { module, name } = head else {
                return None;
            };
            let implementation = Type::con(module, &format!("{name}.Impl"));
            return Some(Type::apps(implementation, args.into_iter().cloned()));
        }
        None
    }

    /// `Errors e` against a single error type or `Never`, with `e` unknown:
    /// `e` becomes the singleton row of that error, or the empty row.
    fn unify_errors(&mut self, projection: &Type, other: &Type) -> Option<Unified> {
        let (head, args) = projection.spine();
        let [row] = args[..] else { return None };
        if !head.is_lang(Lang::Errors) {
            return None;
        }
        let norm = self.store.normalize(row);
        let ([Type::Meta(m)], true) = (&norm.tails[..], norm.entries.is_empty()) else {
            return None;
        };
        let m = *m;
        if matches!(other, Type::Meta(_) | Type::Error) {
            return None;
        }
        if other.is_lang(Lang::Never) {
            return Some(self.bind(m, &Type::empty_row()));
        }
        Some(match self.error_label(other) {
            Some(label) => self.bind(m, &Type::Row(Row::closed(vec![(label, other.clone())]))),
            None => Err(Mismatch::Types(projection.clone(), other.clone())),
        })
    }

    pub fn unify(&mut self, a: &Type, b: &Type) -> Unified {
        self.compare_unfolding(a, b, |c| c.unify_unfolded(a, b))
    }

    #[allow(clippy::too_many_lines)]
    fn unify_unfolded(&mut self, a: &Type, b: &Type) -> Unified {
        let (mut left, mut right) = (self.bound_head(a), self.bound_head(b));
        if matches!(&left, Type::Captured { value: true, .. })
            || matches!(&right, Type::Captured { value: true, .. })
        {
            let (x, y) = (self.whnf(&left), self.whnf(&right));
            if let (Type::App { head: x, .. }, Type::App { head: y, .. }) = (&x, &y)
                && (matches!(self.store.resolve(x), Type::Meta(_))
                    || matches!(self.store.resolve(y), Type::Meta(_)))
            {
                self.unify(x, y)?;
                (left, right) = (self.bound_head(a), self.bound_head(b));
            }
        }
        match (&left, &right) {
            (
                Type::Bounded {
                    ty: x,
                    captures: cx,
                },
                Type::Bounded {
                    ty: y,
                    captures: cy,
                },
            ) => {
                self.unify(cx, cy)?;
                let (x, y) = (self.capture_identity(x), self.capture_identity(y));
                return self.unify(&x, &y);
            }
            // `α^c ≡ α` is no infinite type: the bound is compared below
            (Type::Bounded { .. } | Type::Captured { .. }, Type::Meta(m))
                if !self.qualifies_meta(&left, *m) =>
            {
                return self.bind(*m, &left);
            }
            (Type::Meta(m), Type::Bounded { .. } | Type::Captured { .. })
                if !self.qualifies_meta(&right, *m) =>
            {
                return self.bind(*m, &right);
            }
            (Type::Bounded { ty, captures }, other) | (other, Type::Bounded { ty, captures }) => {
                // A function holds what its qualifier says, not its parameter
                // or result types; only data holds the captures its parts show;
                // a scalar holds nothing, whatever its bound says.
                if !self.scalar(other) {
                    let held = if matches!(self.whnf(other), Type::Fun { .. }) {
                        Type::empty_row()
                    } else {
                        self.captures_of(other).into_type()
                    };
                    self.unify_qualifiers(captures, &held)?;
                }
                let identity = self.capture_identity(ty);
                return self.unify(&identity, other);
            }
            _ => {}
        }
        if self.same_application(&left, &right) {
            return Ok(());
        }
        let ((a, ca), (b, cb)) = (self.whnf_captured(&left), self.whnf_captured(&right));
        if ca.is_some() || cb.is_some() {
            return self.unify_captured((a, ca), (b, cb));
        }
        if let Some(result) = self
            .unify_errors(&a, &b)
            .or_else(|| self.unify_errors(&b, &a))
        {
            return result;
        }
        match (&a, &b) {
            (Type::Error, _) | (_, Type::Error) => Ok(()),
            (Type::Meta(x), Type::Meta(y)) if x == y => Ok(()),
            (Type::Meta(m), t) | (t, Type::Meta(m)) => self.bind(*m, t),
            (Type::Skolem(x), Type::Skolem(y)) if x == y => Ok(()),
            (Type::Var(x), Type::Var(y)) | (Type::Symbol(x), Type::Symbol(y)) if x == y => Ok(()),
            (Type::Con { .. }, Type::Con { .. }) if a == b => Ok(()),
            (Type::App { head: h1, arg: a1 }, Type::App { head: h2, arg: a2 }) => {
                // an argument that is no error explains itself better than the application
                self.unify(h1, h2).map_err(|m| match m {
                    Mismatch::Types(x, y)
                        if x.spine().0.is_lang(Lang::Errors)
                            || y.spine().0.is_lang(Lang::Errors) =>
                    {
                        Mismatch::Types(x, y)
                    }
                    // a region leaves its `scope` in the `Live` of a row,
                    // which no type shows
                    Mismatch::Escape(s) => Mismatch::Escape(s),
                    _ => Mismatch::Types(a.clone(), b.clone()),
                })?;
                self.unify(a1, a2)
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
                match (e1, e2) {
                    (None, None) => {}
                    (Some(e1), Some(e2)) if e1.sync == e2.sync => {
                        self.unify(&e1.services, &e2.services)?;
                        self.unify(&e1.errors, &e2.errors)?;
                    }
                    _ => return Err(Mismatch::Types(a.clone(), b.clone())),
                }
                self.unify(p1, p2)?;
                self.unify(r1, r2)
            }
            (Type::Tuple(x), Type::Tuple(y)) if x.len() == y.len() => {
                x.iter().zip(y).try_for_each(|(x, y)| self.unify(x, y))
            }
            (Type::Record(r1), Type::Record(r2)) => self.unify_rows(r1, r2),
            (
                Type::Exists {
                    binder: x,
                    contract: c1,
                    body: b1,
                },
                Type::Exists {
                    binder: y,
                    contract: c2,
                    body: b2,
                },
            ) => {
                fn alpha(ty: &Type, binder: &str) -> Type {
                    if let Type::Con { module, name } = ty
                        && module == &format!("$exists:{binder}")
                    {
                        return Type::con("$exists:#", name);
                    }
                    if matches!(ty, Type::Exists { binder: inner, .. } if inner == binder) {
                        return ty.clone();
                    }
                    ty.map_children(&mut |child| alpha(child, binder))
                }
                self.unify(c1, c2)?;
                self.unify(&alpha(b1, x), &alpha(b2, y))
            }
            (Type::Row(_) | Type::Union(..) | Type::Without { .. }, _)
            | (_, Type::Row(_) | Type::Union(..) | Type::Without { .. }) => {
                self.unify_row_types(&a, &b)
            }
            (
                Type::Forall {
                    vars: v1, body: b1, ..
                },
                Type::Forall {
                    vars: v2, body: b2, ..
                },
            ) if v1.len() == v2.len() && v1.iter().zip(v2).all(|(x, y)| x.kind == y.kind) => {
                self.store.level += 1;
                let skolems: Vec<Type> = v1.iter().map(|v| self.store.skolem(&v.name)).collect();
                let s1: Vec<_> = v1
                    .iter()
                    .map(|v| v.name.clone())
                    .zip(skolems.clone())
                    .collect();
                let s2: Vec<_> = v2.iter().map(|v| v.name.clone()).zip(skolems).collect();
                let result = self.unify(&b1.substitute(&s1), &b2.substitute(&s2));
                self.store.level -= 1;
                result
            }
            _ => Err(Mismatch::Types(a.clone(), b.clone())),
        }
    }

    /// One type constructor applied to arguments that unify: equal without
    /// expanding an alias, which a self-referencing opaque type needs.
    fn same_application(&mut self, a: &Type, b: &Type) -> bool {
        let (a, b) = (self.store.resolve(a), self.store.resolve(b));
        let ((h1, args1), (h2, args2)) = (a.spine(), b.spine());
        if !matches!(h1, Type::Con { .. }) || h1 != h2 || args1.len() != args2.len() {
            return false;
        }
        args1.is_empty()
            || args1
                .iter()
                .zip(&args2)
                .all(|(x, y)| self.unify(x, y).is_ok())
    }

    /// Whether `t` is a qualifier on the meta `m` itself.
    fn qualifies_meta(&self, t: &Type, m: u32) -> bool {
        self.store.resolve(&self.capture_identity(t)) == Type::Meta(m)
    }

    pub fn bind(&mut self, m: u32, t: &Type) -> Unified {
        let t = self.zonk(t);
        if t.contains(&mut |x| *x == Type::Meta(m)) {
            return Err(Mismatch::Occurs(t));
        }
        if t.contains(&mut |x| matches!(x, Type::Forall { .. })) {
            return Err(Mismatch::Polymorphic(t));
        }
        if self.store.is_numeric(m) && !self.store.numeric_fits(m, &t) {
            return Err(Mismatch::Types(Type::Meta(m), t));
        }
        if self.store.untouchable(m) {
            return Err(Mismatch::Untouchable(t));
        }
        let level = self.store.meta_level(m).expect("an open variable");
        self.store.adjust(&t, level).map_err(Mismatch::Escape)?;
        self.store.set(m, t);
        Ok(())
    }
}
