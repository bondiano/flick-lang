//! Unification variables, skolems and their levels; unification of types.
//!
//! Levels serve two checks. A `let` right-hand side is inferred one level
//! deeper, and only variables still deeper than the `let` are generalized. A
//! skolem gets the level at which its `forall` is checked, and a variable of a
//! shallower level may not be bound to it: that would let the rigid type escape
//! its scope.

use crate::numeric::NumLang;
use std::collections::HashSet;

use super::elem::Elem;
use super::kinds::Kind;
use super::ty::{Norm, Row, Type};
use crate::numeric::Num;

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
    /// Row equations assumed by GADT branches; no residual row is chosen.
    pub row_equalities: Vec<super::gadt::row_facts::Equation>,
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
        label: Elem,
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

    pub fn skolem_level(&self, s: u32) -> u32 {
        self.skolems[s as usize].level
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
            || self
                .row_equalities
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
            Type::Without { row, gone } => {
                let row = Box::new(self.zonk(row));
                let gone = gone.clone();
                self.normalize(&Type::Without { row, gone }).into_type()
            }
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
                elided,
            } => Type::Captured {
                ty: Box::new(self.zonk(ty)),
                captures: Box::new(self.zonk(captures)),
                value: *value,
                elided: *elided,
            },
            _ => t.map_children(&mut |c| self.zonk(c)),
        }
    }

    /// A row with nested row tails merged into one entry list.
    pub fn zonk_row(&self, row: &Row) -> Row {
        let mut entries: Vec<(Elem, Type)> = row
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
        Row::sort_entries(&mut entries);
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
        Row::sort_entries(&mut norm.entries);
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
            // a known row loses the elements; what is still a variable keeps
            // the names to lose
            Type::Without { row, gone } => {
                let mut inner = Norm::default();
                self.collect_row(&row, &mut inner);
                let kept = (inner.entries.into_iter()).filter(|(label, _)| !gone.contains(label));
                out.entries.extend(kept);
                for tail in inner.tails {
                    let tail = super::ty::without(tail, &gone);
                    if !out.tails.contains(&tail) {
                        out.tails.push(tail);
                    }
                }
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
fn relabel(label: &Elem, t: &Type) -> Elem {
    let current = match label {
        Elem::Capture(_) => super::ty::capture_label(t),
        Elem::Error(_) => super::ty::error_label(t),
        Elem::Service(_) => super::ty::service_label(t),
        Elem::Named(_) | Elem::Live | Elem::Provided(_) => None,
    };
    current.unwrap_or_else(|| label.clone())
}

mod compare;
mod instances;
