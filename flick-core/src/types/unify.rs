//! Unification variables, skolems and their levels; unification of types.
//!
//! Levels serve two checks. A `let` right-hand side is inferred one level
//! deeper, and only variables still deeper than the `let` are generalized. A
//! skolem gets the level at which its `forall` is checked, and a variable of a
//! shallower level may not be bound to it: that would let the rigid type escape
//! its scope.

use super::Checker;
use super::env::Form;
use super::ty::{Row, Type};

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
    /// A rigid type variable would leave the scope of its `forall`.
    Escape(String),
    /// A type variable cannot stand for a polymorphic type (§5: predicative).
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
            Type::Con { .. } | Type::Var(_) | Type::Skolem(_) | Type::Error => t.clone(),
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
            _ => t.map_children(&mut |c| self.zonk(c)),
        }
    }

    /// A row with nested row tails merged into one entry list.
    pub fn zonk_row(&self, row: &Row) -> Row {
        let mut entries: Vec<(String, Type)> = row
            .entries
            .iter()
            .map(|(l, t)| (l.clone(), self.zonk(t)))
            .collect();
        let mut tail = row.tail.as_ref().map(|t| self.zonk(t));
        while let Some(Type::Row(inner)) = tail {
            entries.extend(inner.entries);
            tail = inner.tail.map(|t| *t);
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Row {
            entries,
            tail: tail.map(Box::new),
        }
    }

    /// Lowers the levels of the variables in `t` to `level`; `Err` names a
    /// skolem of a deeper level.
    fn adjust(&mut self, t: &Type, level: u32) -> Result<(), String> {
        let mut escaped = None;
        let mut lower = Vec::new();
        t.visit(&mut |t| match t {
            Type::Meta(m) => lower.push(*m),
            Type::Skolem(s) if self.skolems[*s as usize].level > level => {
                escaped = Some(self.skolems[*s as usize].name.clone());
            }
            _ => {}
        });
        if let Some(name) = escaped {
            return Err(name);
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

    /// Keeps the variables of a binding that is not generalized from being
    /// generalized by an enclosing one.
    pub fn lower(&mut self, t: &Type) {
        let level = self.level;
        let _ = self.adjust(&self.zonk(t), level);
    }
}

impl Checker<'_> {
    pub fn fresh(&mut self) -> Type {
        self.store.fresh()
    }

    pub fn zonk(&self, t: &Type) -> Type {
        self.store.zonk(t)
    }

    /// `t` with bound variables followed and aliases at its head expanded.
    pub fn whnf(&self, t: &Type) -> Type {
        let mut t = self.store.resolve(t);
        for _ in 0..64 {
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
        t
    }

    pub fn unify(&mut self, a: &Type, b: &Type) -> Unified {
        if self.same_application(a, b) {
            return Ok(());
        }
        let (a, b) = (self.whnf(a), self.whnf(b));
        match (&a, &b) {
            (Type::Error, _) | (_, Type::Error) => Ok(()),
            (Type::Meta(x), Type::Meta(y)) if x == y => Ok(()),
            (Type::Meta(m), t) | (t, Type::Meta(m)) => self.bind(*m, t),
            (Type::Skolem(x), Type::Skolem(y)) if x == y => Ok(()),
            (Type::Var(x), Type::Var(y)) if x == y => Ok(()),
            (Type::Con { .. }, Type::Con { .. }) if a == b => Ok(()),
            (Type::App { head: h1, arg: a1 }, Type::App { head: h2, arg: a2 }) => {
                self.unify(h1, h2)
                    .map_err(|_| Mismatch::Types(a.clone(), b.clone()))?;
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
            (Type::Row(_), _) | (_, Type::Row(_)) => {
                let as_row = |t: &Type| match t {
                    Type::Row(r) => r.clone(),
                    other => Row {
                        entries: Vec::new(),
                        tail: Some(Box::new(other.clone())),
                    },
                };
                self.unify_rows(&as_row(&a), &as_row(&b))
            }
            // shortcut: unions compare part by part; the effect row solver normalizes them
            (Type::Union(a1, b1), Type::Union(a2, b2)) => {
                self.unify(a1, a2)?;
                self.unify(b1, b2)
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

    /// The body of a polymorphic type with fresh variables for its own.
    pub fn instantiate(&mut self, t: &Type) -> Type {
        match self.store.resolve(t) {
            Type::Forall { vars, body, .. } => {
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .map(|v| (v.name.clone(), self.fresh()))
                    .collect();
                self.instantiate(&body.substitute(&subst))
            }
            other => other,
        }
    }

    /// The body of a polymorphic type with fresh skolems at the current
    /// level, and the skolems by name.
    pub fn skolemize(&mut self, t: &Type) -> (Type, Vec<(String, Type)>) {
        match self.store.resolve(t) {
            Type::Forall { vars, body, .. } => {
                let subst: Vec<(String, Type)> = vars
                    .iter()
                    .map(|v| (v.name.clone(), self.store.skolem(&v.name)))
                    .collect();
                let (body, mut more) = self.skolemize(&body.substitute(&subst));
                more.splice(0..0, subst);
                (body, more)
            }
            other => (other, Vec::new()),
        }
    }
}
