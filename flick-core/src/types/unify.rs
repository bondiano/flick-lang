//! Unification variables, skolems and their levels; unification of types.
//!
//! Levels serve two checks. A `let` right-hand side is inferred one level
//! deeper, and only variables still deeper than the `let` are generalized. A
//! skolem gets the level at which its `forall` is checked, and a variable of a
//! shallower level may not be bound to it: that would let the rigid type escape
//! its scope.

use super::Checker;
use super::env::Form;
use super::ty::{Norm, Row, Type};
use crate::source::Span;

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
    pub level: u32,
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

    /// A variable of the level of `m`, to extend what `m` stands for.
    pub fn fresh_beside(&mut self, m: u32) -> Type {
        let level = self.meta_level(m).unwrap_or(self.level);
        self.metas.push(MetaState::Open { level });
        Type::Meta(id(self.metas.len() - 1))
    }

    pub fn skolem(&mut self, name: &str) -> Type {
        self.skolems.push(Skolem {
            name: name.to_owned(),
            level: self.level,
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
        let mut t = t;
        while let Type::Meta(m) = t {
            match &self.metas[*m as usize] {
                MetaState::Bound(next) => t = next,
                MetaState::Open { .. } => break,
            }
        }
        t.clone()
    }

    /// `t` with every bound variable replaced by its solution.
    pub fn zonk(&self, t: &Type) -> Type {
        match t {
            Type::Meta(m) => match &self.metas[*m as usize] {
                MetaState::Bound(next) => self.zonk(next),
                MetaState::Open { .. } => t.clone(),
            },
            Type::Con { .. } | Type::Var(_) | Type::Symbol(_) | Type::Skolem(_) | Type::Error => {
                t.clone()
            }
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
            // a qualifier that came out empty is no qualifier
            Type::Captured { ty, captures } => match self.zonk(captures) {
                Type::Row(row) if row.entries.is_empty() && row.tail.is_none() => self.zonk(ty),
                captures => Type::Captured {
                    ty: Box::new(self.zonk(ty)),
                    captures: Box::new(captures),
                },
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
        entries.dedup_by(|a, b| a.0 == b.0);
        Row {
            entries,
            tail: tail.map(Box::new),
        }
    }

    /// A row as its labelled entries and the row variables it extends:
    /// `{A | r} + {B | s}` is `A, B` and `r, s`. Repeated labels and tails
    /// count once.
    pub fn normalize(&self, t: &Type) -> Norm {
        let mut norm = Norm::default();
        self.collect_row(t, &mut norm);
        norm.entries.sort_by(|a, b| a.0.cmp(&b.0));
        norm.entries.dedup_by(|a, b| a.0 == b.0);
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

/// The current label of a row entry: a capture row entry follows its element.
fn relabel(label: &str, t: &Type) -> String {
    match label.starts_with('^').then(|| super::ty::capture_label(t)) {
        Some(Some(label)) => label,
        _ => label.to_owned(),
    }
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

    /// [`Self::whnf`], and the capture row of the qualifiers taken off.
    pub fn whnf_captured(&self, t: &Type) -> (Type, Option<Type>) {
        let mut t = self.store.resolve(t);
        let mut captures: Option<Type> = None;
        for _ in 0..64 {
            if let Type::Captured { ty, captures: c } = t {
                captures = Some(match captures {
                    Some(outer) => Type::Union(Box::new(outer), c),
                    None => *c,
                });
                t = self.store.resolve(&ty);
                continue;
            }
            if let Some(reduced) = self.project(&t) {
                t = self.store.resolve(&reduced);
                continue;
            }
            let (head, args) = t.spine();
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
            let expanded = Type::apps(
                body.substitute(&subst),
                args[params.len()..].iter().map(|a| (*a).clone()),
            );
            t = self.store.resolve(&expanded);
        }
        (t, captures)
    }

    /// The fixed projections of a row into a type, once the row is known
    /// well enough: `Errors none` is `Never`, `Errors {E}` is `E`, and
    /// `ServiceImpl {S}` is `S.Impl`; `Record.Field "x" T` is `{ x : T }`.
    fn project(&self, t: &Type) -> Option<Type> {
        let (head, args) = t.spine();
        if let ([label, field], Type::Con { module, name }) = (&args[..], head)
            && module == super::classes::RECORD
            && name == "Field"
            && let Type::Symbol(label) = self.store.resolve(label)
        {
            return Some(Type::Record(Row::closed(vec![(label, (*field).clone())])));
        }
        let [row] = args[..] else { return None };
        if !head.is_prim("Errors") && !head.is_prim("ServiceImpl") {
            return None;
        }
        let norm = self.store.normalize(row);
        if !norm.tails.is_empty() || norm.entries.len() > 1 {
            return None;
        }
        let entry = norm.entries.into_iter().next().map(|(_, ty)| ty);
        if head.is_prim("Errors") {
            return Some(entry.unwrap_or_else(|| Type::prim("Never")));
        }
        if head.is_prim("ServiceImpl") {
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
        if !head.is_prim("Errors") {
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
        if other.is_prim("Never") {
            return Some(self.bind(m, &Type::empty_row()));
        }
        Some(match self.error_label(other) {
            Some(label) => self.bind(m, &Type::Row(Row::closed(vec![(label, other.clone())]))),
            None => Err(Mismatch::Types(projection.clone(), other.clone())),
        })
    }

    pub fn unify(&mut self, a: &Type, b: &Type) -> Unified {
        if self.same_application(a, b) {
            return Ok(());
        }
        let ((a, ca), (b, cb)) = (self.whnf_captured(a), self.whnf_captured(b));
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
                        if x.spine().0.is_prim("Errors") || y.spine().0.is_prim("Errors") =>
                    {
                        Mismatch::Types(x, y)
                    }
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
                    (Some(e1), Some(e2)) => {
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

    pub fn bind(&mut self, m: u32, t: &Type) -> Unified {
        let t = self.zonk(t);
        if t.contains(&mut |x| *x == Type::Meta(m)) {
            return Err(Mismatch::Occurs(t));
        }
        if t.contains(&mut |x| matches!(x, Type::Forall { .. })) {
            return Err(Mismatch::Polymorphic(t));
        }
        let level = self.store.meta_level(m).expect("an open variable");
        self.store.adjust(&t, level).map_err(Mismatch::Escape)?;
        self.store.set(m, t);
        Ok(())
    }

    /// The body of a polymorphic type with fresh variables for its own; its
    /// class constraints become constraints to solve, required at `span`.
    pub fn instantiate(&mut self, t: &Type, span: Span) -> Type {
        match self.store.resolve(t) {
            Type::Captured { ty, captures } => Type::Captured {
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
                    .map(|v| (v.name.clone(), self.fresh()))
                    .collect();
                for c in &constraints {
                    self.want(c.substitute(&subst), span);
                }
                self.instantiate(&body.substitute(&subst), span)
            }
            other => other,
        }
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
                    .map(|v| (v.name.clone(), self.store.skolem(&v.name)))
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
