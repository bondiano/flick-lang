//! Unification variables, skolems and their levels; unification of types.
//!
//! Levels serve two checks. A `let` right-hand side is inferred one level
//! deeper, and only variables still deeper than the `let` are generalized. A
//! skolem gets the level at which its `forall` is checked, and a variable of a
//! shallower level may not be bound to it: that would let the rigid type escape
//! its scope.

use crate::numeric::NumLang;
use std::borrow::Cow;
use std::collections::HashSet;
use std::ptr;

use super::Checker;
use super::env::Form;
use super::kinds::Kind;
use super::ty::{Norm, Row, Type};
use super::typing::passes_dictionary;
use crate::numeric::Num;
use crate::source::Span;
use crate::types::lang::{self, Lang};

#[derive(Debug, Clone)]
enum MetaState {
    Open { level: u32 },
    Bound(Type),
}

#[derive(Debug, Clone)]
struct Skolem {
    name: String,
    level: u32,
}

#[derive(Debug, Default)]
pub struct Store {
    metas: Vec<MetaState>,
    skolems: Vec<Skolem>,
    /// The variables that stand for a capture row: an instantiated `^c`.
    capture_rows: HashSet<u32>,
    /// Those that stand for a row of errors: an instantiated `e` of kind
    /// `ErrorRow`, or the errors of a stage or a handler.
    error_rows: HashSet<u32>,
    /// The variables of number literals, which only a numeric type or a
    /// rigid variable binds.
    numeric: HashSet<u32>,
    /// Those of literals with a fraction, which an integer type cannot bind.
    fractional: HashSet<u32>,
    pub level: u32,
    /// What rigid variables equal in the branches of GADT matches being
    /// checked, innermost last.
    pub equalities: Vec<Equality>,
}

/// A rigid variable that a matched constructor made equal to `ty`.
#[derive(Debug, Clone)]
pub struct Equality {
    pub skolem: u32,
    pub ty: Type,
    /// The level of the branch that learned it; `None` while a deferred
    /// constraint is solved under the assumptions it was made with.
    pub level: Option<u32>,
}

/// Why two types do not unify, about the innermost conflicting part.
#[derive(Debug, Clone)]
pub enum Mismatch {
    Types(Type, Type),
    /// A record or row lacks a label the other one has; `left` is the side
    /// that has it.
    Label {
        label: String,
        left: bool,
    },
    /// Binding the variable would make the type infinite.
    Occurs(Type),
    /// A rigid type variable, by skolem, would leave the scope of its
    /// `forall` or the region of its `scope`.
    Escape(u32),
    /// A type variable cannot stand for a polymorphic type (instantiation is predicative).
    Polymorphic(Type),
    /// A variable from outside a branch that learned an equality would get
    /// its type from that equality.
    Untouchable(Type),
}

pub type Unified = Result<(), Mismatch>;

fn id(n: usize) -> u32 {
    u32::try_from(n).expect("fewer than 2^32 type variables")
}

impl Store {
    pub fn fresh(&mut self) -> Type {
        self.metas.push(MetaState::Open { level: self.level });
        Type::Meta(id(self.metas.len() - 1))
    }

    /// A variable for a capture row of a quantified `^c`.
    pub fn fresh_capture_row(&mut self) -> Type {
        let row = self.fresh();
        if let Type::Meta(m) = row {
            self.capture_rows.insert(m);
        }
        row
    }

    /// A variable for a row of errors.
    pub fn fresh_error_row(&mut self) -> Type {
        let row = self.fresh();
        if let Type::Meta(m) = row {
            self.error_rows.insert(m);
        }
        row
    }

    /// A variable for a quantified variable of `kind`.
    pub fn fresh_of(&mut self, kind: &Kind) -> Type {
        match kind {
            Kind::CaptureRow => self.fresh_capture_row(),
            Kind::ErrorRow => self.fresh_error_row(),
            _ => self.fresh(),
        }
    }

    /// A variable that only a numeric type binds, a `Float` one when
    /// `fractional`.
    pub fn fresh_numeric(&mut self, fractional: bool) -> Type {
        let ty = self.fresh();
        if let Type::Meta(m) = ty {
            self.numeric.insert(m);
            if fractional {
                self.fractional.insert(m);
            }
        }
        ty
    }

    pub fn is_numeric(&self, m: u32) -> bool {
        self.numeric.contains(&m)
    }

    pub fn is_fractional(&self, m: u32) -> bool {
        self.fractional.contains(&m)
    }

    /// Whether the numeric variable `m` may stand for `t`; a variable it
    /// binds takes its restriction. A rigid variable fits: its class
    /// constraint is checked when the literal is settled.
    fn numeric_fits(&mut self, m: u32, t: &Type) -> bool {
        let fractional = self.is_fractional(m);
        match t {
            Type::Captured { ty, .. } | Type::Bounded { ty, .. } => self.numeric_fits(m, ty),
            Type::Meta(n) => {
                self.numeric.insert(*n);
                if fractional {
                    self.fractional.insert(*n);
                }
                true
            }
            Type::Error | Type::Skolem(_) => true,
            t => Num::ALL
                .into_iter()
                .any(|n| t.is_lang(n.lang()) && (n.fractional() || !fractional)),
        }
    }

    /// Whether `m` stands for a capture row, not a type whose captures count.
    pub fn is_capture_row(&self, m: u32) -> bool {
        self.capture_rows.contains(&m)
    }

    /// Whether `m` stands for a row of errors.
    pub fn is_error_row(&self, m: u32) -> bool {
        self.error_rows.contains(&m)
    }

    /// A variable of the level and the kind of `m`, to extend what `m`
    /// stands for.
    pub fn fresh_beside(&mut self, m: u32) -> Type {
        let level = self.meta_level(m).unwrap_or(self.level);
        self.metas.push(MetaState::Open { level });
        let rest = id(self.metas.len() - 1);
        self.mark_like(rest, m);
        Type::Meta(rest)
    }

    /// Gives `to` the kind marks of `from`: a capture row or a row of errors.
    pub fn mark_like(&mut self, to: u32, from: u32) {
        if self.is_capture_row(from) {
            self.capture_rows.insert(to);
        }
        if self.is_error_row(from) {
            self.error_rows.insert(to);
        }
    }

    pub fn skolem(&mut self, name: &str) -> Type {
        self.skolem_at(name, self.level)
    }

    /// A rigid variable of the code at `level`, around the current one.
    pub fn skolem_at(&mut self, name: &str, level: u32) -> Type {
        self.skolems.push(Skolem {
            name: name.to_owned(),
            level,
        });
        Type::Skolem(id(self.skolems.len() - 1))
    }

    pub fn skolem_name(&self, s: u32) -> &str {
        &self.skolems[s as usize].name
    }

    pub fn meta_level(&self, m: u32) -> Option<u32> {
        match self.metas[m as usize] {
            MetaState::Open { level } => Some(level),
            MetaState::Bound(_) => None,
        }
    }

    pub fn set(&mut self, m: u32, t: Type) {
        self.metas[m as usize] = MetaState::Bound(t);
    }

    /// Follows bound variables at the top of `t`.
    pub fn resolve(&self, t: &Type) -> Type {
        self.head(t).clone()
    }

    /// [`Store::resolve`] without the copy.
    pub fn head<'a>(&'a self, mut t: &'a Type) -> &'a Type {
        loop {
            match t {
                // a variable this store never made (a stored type shown by
                // `show_type` with an empty store) is open
                Type::Meta(m) => match self.metas.get(*m as usize) {
                    Some(MetaState::Bound(next)) => t = next,
                    Some(MetaState::Open { .. }) | None => return t,
                },
                Type::Skolem(s) => match self.equal(*s) {
                    Some(next) => t = next,
                    None => return t,
                },
                _ => return t,
            }
        }
    }

    /// What the rigid variable `s` equals in the branch being checked.
    pub fn equal(&self, s: u32) -> Option<&Type> {
        self.equalities
            .iter()
            .rev()
            .find(|e| e.skolem == s)
            .map(|e| &e.ty)
    }

    /// Whether binding the variable `m` would take a branch's equality
    /// outside it: `m` is older than a branch that learned one.
    fn untouchable(&self, m: u32) -> bool {
        let level = self.meta_level(m).unwrap_or(u32::MAX);
        self.equalities
            .iter()
            .any(|e| e.level.is_some_and(|l| l > level))
    }

    /// `t` with every bound variable replaced by its solution.
    pub fn zonk(&self, t: &Type) -> Type {
        match t {
            Type::Meta(m) => match &self.metas[*m as usize] {
                MetaState::Bound(next) => self.zonk(next),
                MetaState::Open { .. } => t.clone(),
            },
            Type::Skolem(s) => match self.equal(*s) {
                Some(next) => self.zonk(next),
                None => t.clone(),
            },
            Type::Con { .. } | Type::Var(_) | Type::Symbol(_) | Type::Error => t.clone(),
            Type::Union(a, b) => self
                .normalize(&Type::Union(Box::new(self.zonk(a)), Box::new(self.zonk(b))))
                .into_type(),
            Type::Record(row) | Type::Row(row) => {
                let row = self.zonk_row(row);
                if matches!(t, Type::Record(_)) {
                    Type::Record(row)
                } else if row.entries.is_empty() && row.tail.is_some() {
                    *row.tail.expect("checked")
                } else {
                    Type::Row(row)
                }
            }
            Type::Captured {
                ty,
                captures,
                value,
            } => Type::Captured {
                ty: Box::new(self.zonk(ty)),
                captures: Box::new(self.zonk(captures)),
                value: *value,
            },
            _ => t.map_children(&mut |c| self.zonk(c)),
        }
    }

    /// A row with nested row tails merged into one entry list.
    pub fn zonk_row(&self, row: &Row) -> Row {
        let mut entries: Vec<(String, Type)> = row
            .entries
            .iter()
            .map(|(l, t)| {
                let t = self.zonk(t);
                (relabel(l, &t), t)
            })
            .collect();
        let mut tail = row.tail.as_ref().map(|t| self.zonk(t));
        while let Some(Type::Row(inner)) = tail {
            entries.extend(inner.entries);
            tail = inner.tail.map(|t| *t);
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.dedup();
        Row {
            entries,
            tail: tail.map(Box::new),
            written: row.written,
        }
    }

    /// A row as its labelled entries and the row variables it extends:
    /// `{A | r} + {B | s}` is `A, B` and `r, s`. Repeated labels and tails
    /// count once.
    pub fn normalize(&self, t: &Type) -> Norm {
        let mut norm = Norm::default();
        self.collect_row(t, &mut norm);
        norm.entries.sort_by(|a, b| a.0.cmp(&b.0));
        // the same name with other arguments stays, for unification to compare
        norm.entries.dedup();
        norm
    }

    fn collect_row(&self, t: &Type, out: &mut Norm) {
        match self.resolve(t) {
            Type::Row(row) => {
                out.entries.extend(row.entries.into_iter().map(|(l, t)| {
                    let t = self.resolve(&t);
                    (relabel(&l, &t), t)
                }));
                if let Some(tail) = row.tail {
                    self.collect_row(&tail, out);
                }
            }
            Type::Union(a, b) => {
                self.collect_row(&a, out);
                self.collect_row(&b, out);
            }
            other => {
                if !out.tails.contains(&other) {
                    out.tails.push(other);
                }
            }
        }
    }

    /// Lowers the levels of the variables in `t` to `level`; `Err` is a
    /// skolem of a deeper level.
    fn adjust(&mut self, t: &Type, level: u32) -> Result<(), u32> {
        let mut escaped = None;
        let mut lower = Vec::new();
        t.visit(&mut |t| match t {
            Type::Meta(m) => lower.push(*m),
            Type::Skolem(s) if self.skolems[*s as usize].level > level => {
                escaped = Some(*s);
            }
            _ => {}
        });
        if let Some(s) = escaped {
            return Err(s);
        }
        for m in lower {
            if let MetaState::Open { level: l } = &mut self.metas[m as usize] {
                *l = (*l).min(level);
            }
        }
        Ok(())
    }

    /// Variables of `t` deeper than the current level, in order of appearance.
    pub fn deep_metas(&self, t: &Type) -> Vec<u32> {
        let mut out = Vec::new();
        t.visit(&mut |t| {
            if let Type::Meta(m) = t
                && self.meta_level(*m).is_some_and(|l| l > self.level)
                && !out.contains(m)
            {
                out.push(*m);
            }
        });
        out
    }

    /// Moves `t` to the current level, as a value that leaves a scope: `Err`
    /// is a rigid variable of the scope that it would take along.
    pub fn leave(&mut self, t: &Type) -> Result<(), u32> {
        let level = self.level;
        self.adjust(&self.zonk(t), level)
    }

    /// Keeps the variables of a binding that is not generalized from being
    /// generalized by an enclosing one.
    pub fn lower(&mut self, t: &Type) {
        let level = self.level;
        let _ = self.adjust(&self.zonk(t), level);
    }
}

/// The current label of a row entry: a capture row entry, and a type
/// variable of an error row, follow their element.
fn relabel(label: &str, t: &Type) -> String {
    let current = if label.starts_with('^') {
        super::ty::capture_label(t)
    } else if label.starts_with('~') {
        super::ty::error_label(t)
    } else if label.starts_with('@') {
        super::ty::service_label(t)
    } else {
        None
    };
    current.unwrap_or_else(|| label.to_owned())
}

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
            return Some(Type::Record(Row::closed(vec![(label, (*field).clone())])));
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
            (Type::Row(_) | Type::Union(..), _) | (_, Type::Row(_) | Type::Union(..)) => {
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

    /// The body of a polymorphic type with fresh variables for its own; its
    /// class constraints become constraints to solve, required at `span`.
    /// Records the type variable elements of error rows in `t`, as the
    /// signature wrote them (`~t`), to check that each is an error type.
    fn error_elements(&mut self, t: &Type, span: Span) {
        let mut found = Vec::new();
        t.visit(&mut |t| {
            if let Type::Row(row) = t {
                let elements = row
                    .entries
                    .iter()
                    .filter(|(label, _)| label.starts_with('~'));
                found.extend(elements.map(|(_, element)| (element.clone(), span)));
            }
        });
        self.regions.error_elements.extend(found);
    }

    pub fn instantiate(&mut self, t: &Type, span: Span) -> Type {
        match self.store.resolve(t) {
            Type::Captured {
                ty,
                captures,
                value,
            } => Type::Captured {
                ty: Box::new(self.instantiate(&ty, span)),
                captures,
                value,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.instantiate(&ty, span)),
                captures,
            },
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .map(|v| (v.name.clone(), self.store.fresh_of(&v.kind)))
                    .collect();
                for c in &constraints {
                    self.want(c.substitute(&subst), span);
                }
                let body = body.substitute(&subst);
                self.error_elements(&body, span);
                self.instantiate(&body, span)
            }
            other => other,
        }
    }

    /// The name of the rigid variable for `name` bound with `constraints`:
    /// numbered when an assumption in scope that passes a dictionary uses it
    /// too, since code generation finds a dictionary by its constraint as shown.
    fn unshadowed(&self, name: &str, constraints: &[Type]) -> String {
        let is_var = |t: &Type| matches!(t, Type::Var(n) if n == name);
        if !constraints
            .iter()
            .any(|c| passes_dictionary(c) && c.contains(&mut |t| is_var(t)))
        {
            return name.to_owned();
        }
        let mut used = HashSet::new();
        for g in self.classes.givens.iter().filter(|g| passes_dictionary(g)) {
            g.visit(&mut |t| match t {
                Type::Var(n) => drop(used.insert(n.clone())),
                Type::Skolem(s) => drop(used.insert(self.store.skolem_name(*s).to_owned())),
                _ => {}
            });
        }
        if !used.contains(name) {
            return name.to_owned();
        }
        // `used` is finite: one of its length + 1 numbers is free
        (1..=used.len() + 1)
            .map(|i| format!("{name}{i}"))
            .find(|n| !used.contains(n))
            .expect("a free number")
    }

    /// The body of a polymorphic type with fresh skolems at the current
    /// level, the skolems by name, and its class constraints over them.
    pub fn skolemize(&mut self, t: &Type) -> (Type, Vec<(String, Type)>, Vec<Type>) {
        match self.store.resolve(t) {
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .map(|v| {
                        let ty = self.store.skolem(&self.unshadowed(&v.name, &constraints));
                        if let Type::Skolem(s) = &ty {
                            self.regions.rigid_kinds.push((*s, v.kind.clone()));
                        }
                        (v.name.clone(), ty)
                    })
                    .collect();
                let (body, mut more, mut givens) = self.skolemize(&body.substitute(&subst));
                more.splice(0..0, subst.iter().cloned());
                givens.splice(0..0, constraints.iter().map(|c| c.substitute(&subst)));
                (body, more, givens)
            }
            other => (other, Vec::new(), Vec::new()),
        }
    }
}
