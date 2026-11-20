//! Placement of surface qualifiers on syntactic curried/rank-2 callables and
//! propagation of live region atoms across hidden qualifiers and value bounds.

use super::live::add_live;
use super::{AliasKey, Unfold, leave_alias};
use crate::types::Checker;
use crate::types::ty::{Binder, Effects, Norm, Type};

impl Checker<'_> {
    /// Whether `t` is a function, polymorphic callable, task or lazy whose
    /// qualifier tracks what using it needs live.
    pub(in crate::types) fn is_callable(&self, t: &Type) -> bool {
        let t = self.whnf(t);
        matches!(t, Type::Fun { .. } | Type::Forall { .. })
            || self.eff_parts(&t).is_some()
            || self.lazy_parts(&t).is_some()
    }

    /// The hidden qualifiers (`Type::Captured`, skipping value bounds) at the
    /// head of `t`.
    pub(in crate::types) fn hidden_qualifier(&self, t: &Type) -> Option<Type> {
        let mut t = self.qualified_head(t);
        let mut hidden: Option<Type> = None;
        for _ in 0..64 {
            match t {
                Type::Bounded { ty, .. } => t = self.qualified_head(&ty),
                Type::Captured { ty, captures, .. } => {
                    hidden = Some(match hidden {
                        Some(outer) => Type::Union(Box::new(outer), captures),
                        None => *captures,
                    });
                    t = self.qualified_head(&ty);
                }
                _ => break,
            }
        }
        hidden
    }

    /// `ty` with `captures` added to its hidden qualifier.
    pub(in crate::types) fn qualify_hidden(ty: Type, captures: Type) -> Type {
        super::qualify(ty, captures)
    }

    /// Carries the non-empty outer `Type::Bounded` value bounds of `outer`
    /// onto `inner`, skipping `Type::Captured` hidden qualifiers.
    pub(in crate::types) fn carry_bound(&self, outer: &Type, inner: Type) -> Type {
        let mut layers = Vec::new();
        let mut cur = self.qualified_head(outer);
        for _ in 0..64 {
            match cur {
                Type::Captured { ty, .. } => cur = self.qualified_head(&ty),
                Type::Bounded { ty, captures } => {
                    let norm = self.store.normalize(&captures);
                    if !norm.entries.is_empty() || !norm.tails.is_empty() {
                        layers.push(*captures);
                    }
                    cur = self.qualified_head(&ty);
                }
                _ => break,
            }
        }
        layers.into_iter().rev().fold(inner, |t, c| Type::Bounded {
            ty: Box::new(t),
            captures: Box::new(c),
        })
    }

    /// The union of the outer `Type::Bounded` capture rows on `t`, if any.
    pub(in crate::types) fn bound_captures(&self, t: &Type) -> Option<Type> {
        let mut cur = self.qualified_head(t);
        let mut out: Option<Type> = None;
        for _ in 0..64 {
            match cur {
                Type::Bounded { ty, captures } => {
                    out = Some(match out {
                        Some(outer) => super::union_captures(outer, *captures),
                        None => *captures,
                    });
                    cur = self.qualified_head(&ty);
                }
                Type::Captured { ty, .. } => cur = self.qualified_head(&ty),
                _ => break,
            }
        }
        out
    }

    /// Places any outer surface hidden qualifier (`Type::Captured`) on a
    /// syntactically curried or rank-2 callable starting with a pure arrow onto
    /// its inner callable stage after the pure arrows, before type-variable
    /// instantiation.
    pub(in crate::types) fn place_qualifiers(&self, t: &Type) -> Type {
        match t {
            Type::Captured {
                ty,
                captures,
                value,
                elided,
            } => {
                let ty = self.place_qualifiers(ty);
                let ty = if self.starts_with_pure_to_runtime(&ty, &mut Vec::new()) {
                    self.push_past_pure_arrows(&ty, captures, &mut Vec::new())
                } else {
                    ty
                };
                Type::Captured {
                    ty: Box::new(ty),
                    captures: captures.clone(),
                    value: *value,
                    elided: *elided,
                }
            }
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.place_qualifiers(ty)),
                captures: captures.clone(),
            },
            Type::Forall {
                vars,
                constraints,
                body,
            } => Type::Forall {
                vars: vars.clone(),
                constraints: constraints
                    .iter()
                    .map(|c| self.place_qualifiers(c))
                    .collect(),
                body: Box::new(self.place_qualifiers(body)),
            },
            Type::Fun {
                param,
                effects,
                result,
            } => Type::Fun {
                param: Box::new(self.place_qualifiers(param)),
                effects: effects.clone(),
                result: Box::new(self.place_qualifiers(result)),
            },
            other => other.map_children(&mut |c| self.place_qualifiers(c)),
        }
    }

    fn has_runtime_stage(&self, t: &Type, path: &mut Vec<AliasKey>) -> bool {
        let unfold = self.enter_alias(t, path);
        if matches!(unfold, Unfold::Again) {
            return false;
        }
        let found = match self.qualified_head(t) {
            Type::Fun {
                effects: Some(_), ..
            } => true,
            Type::Fun {
                effects: None,
                result,
                ..
            } => self.has_runtime_stage(&result, path),
            Type::Forall { body, .. } => self.has_runtime_stage(&body, path),
            Type::Captured { ty, .. } => self.has_runtime_stage(&ty, path),
            other => self.eff_parts(&other).is_some() || self.lazy_parts(&other).is_some(),
        };
        leave_alias(path, unfold);
        found
    }

    fn starts_with_pure_to_runtime(&self, t: &Type, path: &mut Vec<AliasKey>) -> bool {
        let unfold = self.enter_alias(t, path);
        if matches!(unfold, Unfold::Again) {
            return false;
        }
        let found = match self.qualified_head(t) {
            Type::Fun {
                effects: None,
                result,
                ..
            } => self.has_runtime_stage(&result, path),
            Type::Forall { body, .. } => self.starts_with_pure_to_runtime(&body, path),
            Type::Captured { ty, .. } => self.starts_with_pure_to_runtime(&ty, path),
            _ => false,
        };
        leave_alias(path, unfold);
        found
    }

    fn push_past_pure_arrows(&self, t: &Type, captures: &Type, path: &mut Vec<AliasKey>) -> Type {
        let unfold = self.enter_alias(t, path);
        if matches!(unfold, Unfold::Again) {
            return t.clone();
        }
        let pushed = match self.qualified_head(t) {
            Type::Fun {
                param,
                effects: None,
                result,
            } => {
                let result = if self.starts_with_pure_to_runtime(&result, path) {
                    self.push_past_pure_arrows(&result, captures, path)
                } else {
                    Self::qualify_hidden(*result, captures.clone())
                };
                Type::Fun {
                    param,
                    effects: None,
                    result: Box::new(result),
                }
            }
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let mut subst = Vec::new();
                let mut used: Vec<String> = vars.iter().map(|v| v.name.clone()).collect();
                let vars: Vec<_> = vars
                    .iter()
                    .map(|v| {
                        let shadowed =
                            captures.contains(&mut |x| matches!(x, Type::Var(n) if n == &v.name));
                        if !shadowed {
                            return v.clone();
                        }
                        let mut k = 0usize;
                        let fresh = loop {
                            let candidate = format!("{}${k}", v.name);
                            let collides = used.contains(&candidate)
                                || captures.contains(
                                    &mut |x| matches!(x, Type::Var(n) if n == &candidate),
                                );
                            if !collides {
                                break candidate;
                            }
                            k += 1;
                        };
                        used.push(fresh.clone());
                        subst.push((v.name.clone(), Type::Var(fresh.clone())));
                        Binder {
                            name: fresh,
                            kind: v.kind.clone(),
                        }
                    })
                    .collect();
                let (constraints, body) = if subst.is_empty() {
                    (constraints, *body)
                } else {
                    (
                        constraints.iter().map(|c| c.substitute(&subst)).collect(),
                        body.substitute(&subst),
                    )
                };
                Type::Forall {
                    vars,
                    constraints,
                    body: Box::new(self.push_past_pure_arrows(&body, captures, path)),
                }
            }
            Type::Captured {
                ty,
                captures: inner,
                value,
                elided,
            } => {
                let combined = super::union_captures((*inner).clone(), captures.clone());
                let ty = self.push_past_pure_arrows(&ty, &combined, path);
                Type::Captured {
                    ty: Box::new(ty),
                    captures: inner,
                    value,
                    elided,
                }
            }
            other => other,
        };
        leave_alias(path, unfold);
        pushed
    }

    /// `bare` (from `whnf_captured(actual).0`) with `Live` added for `actual`'s
    /// outer qualifiers: `Type::Bounded` value bounds recurse across pure outer
    /// arrows onto inner runtime stages, while `Type::Captured` hidden qualifiers
    /// follow `Stage.use` directly.
    pub(in crate::types) fn with_actual_live(&self, actual: &Type, bare: Type) -> Type {
        let mut cur = self.qualified_head(actual);
        let mut hidden: Option<Type> = None;
        let mut bounded: Option<Type> = None;
        for _ in 0..64 {
            match cur {
                Type::Captured { ty, captures, .. } => {
                    hidden = Some(match hidden {
                        Some(outer) => super::union_captures(outer, *captures),
                        None => *captures,
                    });
                    cur = self.qualified_head(&ty);
                }
                Type::Bounded { ty, captures } => {
                    bounded = Some(match bounded {
                        Some(outer) => super::union_captures(outer, *captures),
                        None => *captures,
                    });
                    cur = self.qualified_head(&ty);
                }
                _ => break,
            }
        }
        let bare = self.with_live(bare, hidden.as_ref());
        match bounded {
            Some(captures) => self.with_live_bound(bare, &captures),
            None => bare,
        }
    }

    /// `t`, a function, task or lazy, whose runtime stage also needs live what
    /// `captures` holds (`Stage.use`); pure arrows stay pure.
    pub(in crate::types) fn with_live(&self, t: Type, captures: Option<&Type>) -> Type {
        let Some(atoms) = captures.map(|c| self.live_atoms(c, &[])) else {
            return t;
        };
        if atoms.entries.is_empty() && atoms.tails.is_empty() {
            return t;
        }
        self.with_live_atoms(&t, &atoms, false)
    }

    /// `t` under a whole-value `Type::Bounded` bound: adds `Live` of `captures`
    /// onto any runtime stage reachable through pure arrows and `forall`s,
    /// leaving all-pure arrows unchanged.
    pub(in crate::types) fn with_live_bound(&self, t: Type, captures: &Type) -> Type {
        let atoms = self.live_atoms(captures, &[]);
        if atoms.entries.is_empty() && atoms.tails.is_empty() {
            return t;
        }
        self.with_live_atoms(&t, &atoms, true)
    }

    fn with_live_atoms(&self, t: &Type, atoms: &Norm, through_pure: bool) -> Type {
        match self.store.resolve(t) {
            Type::Fun {
                param,
                effects: Some(effects),
                result,
            } => Type::Fun {
                param,
                effects: Some(Box::new(Effects {
                    services: add_live(&effects.services, atoms.clone()),
                    ..*effects
                })),
                result,
            },
            Type::Fun {
                param,
                effects: None,
                result,
            } if through_pure => Type::Fun {
                param,
                effects: None,
                result: Box::new(self.with_live_atoms(&result, atoms, true)),
            },
            Type::Forall {
                vars,
                constraints,
                body,
            } => Type::Forall {
                vars,
                constraints,
                body: Box::new(self.with_live_atoms(&body, atoms, through_pure)),
            },
            Type::Captured {
                ty,
                captures: qualifier,
                value,
                elided,
            } => Type::Captured {
                ty: Box::new(self.with_live_atoms(&ty, atoms, through_pure)),
                captures: qualifier,
                value,
                elided,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(self.with_live_atoms(&ty, atoms, through_pure)),
                captures,
            },
            other => {
                if let Some((s, e, r)) = self.eff_parts(&other) {
                    return super::super::infer::eff(add_live(&s, atoms.clone()), e, r);
                }
                if let Some((s, e, r)) = self.lazy_parts(&other) {
                    return super::super::lazy::lazy(add_live(&s, atoms.clone()), e, r);
                }
                if matches!(other, Type::App { .. } | Type::Con { .. }) {
                    let head = self.witness_head(&other);
                    if head != other
                        && matches!(
                            head,
                            Type::Fun { .. }
                                | Type::Forall { .. }
                                | Type::Captured { .. }
                                | Type::Bounded { .. }
                        )
                    {
                        return self.with_live_atoms(&head, atoms, through_pure);
                    }
                }
                other
            }
        }
    }
}
