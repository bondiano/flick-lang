//! Branch-local row equations. They constrain every possible residual row;
//! they do not solve an idempotent equation by choosing its least witness.

use std::mem;

use super::row_logic::{Theory, Union};
use crate::types::kinds::Kind;
use crate::types::ty::{Norm, Type};
use crate::types::{Checker, Elem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Services,
    Errors,
    Captures,
}

impl RowKind {
    pub fn of(kind: &Kind) -> Option<Self> {
        match kind {
            Kind::ServiceRow => Some(Self::Services),
            Kind::ErrorRow => Some(Self::Errors),
            Kind::CaptureRow => Some(Self::Captures),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Services => "Services",
            Self::Errors => "Errors",
            Self::Captures => "Captures",
        }
    }
}

/// A checked equation; its level prevents inference escaping its branch.
#[derive(Clone, Debug, PartialEq)]
pub struct Equation {
    pub kind: RowKind,
    pub left: Type,
    pub right: Type,
    pub level: Option<u32>,
}

const MODULE: &str = "$gadt-row-equality";

impl Equation {
    pub fn given(&self) -> Type {
        Type::apps(
            Type::con(MODULE, self.kind.name()),
            [self.left.clone(), self.right.clone()],
        )
    }

    pub fn from_given(given: &Type) -> Option<Self> {
        let (Type::Con { module, name }, args) = given.spine() else {
            return None;
        };
        if module != MODULE {
            return None;
        }
        let kind = match name.as_str() {
            "Services" => RowKind::Services,
            "Errors" => RowKind::Errors,
            "Captures" => RowKind::Captures,
            _ => return None,
        };
        let [left, right] = args[..] else { return None };
        Some(Self {
            kind,
            left: left.clone(),
            right: right.clone(),
            level: None,
        })
    }

    pub fn substitute(&self, subst: &[(String, Type)]) -> Self {
        Self {
            kind: self.kind,
            left: self.left.substitute(subst),
            right: self.right.substitute(subst),
            level: self.level,
        }
    }
}

/// A finite set of explicit labels, plus a representative for all other labels.
fn labels(rows: &[Norm]) -> Vec<Option<Elem>> {
    let mut found = vec![None];
    for row in rows {
        for (label, _) in &row.entries {
            if matches!(label, Elem::Named(_) | Elem::Live | Elem::Provided(_)) {
                let label = Some(label.clone());
                if !found.contains(&label) {
                    found.push(label);
                }
            }
        }
        for tail in &row.tails {
            tail.visit(&mut |ty| {
                if let Type::Without { gone, .. } = ty {
                    for label in gone {
                        let label = Some(label.clone());
                        if !found.contains(&label) {
                            found.push(label);
                        }
                    }
                }
            });
        }
    }
    found
}

fn variable(variables: &mut Vec<Type>, ty: &Type, result: &mut Union) {
    let index = if let Some(index) = variables.iter().position(|v| v == ty) {
        index
    } else {
        variables.push(ty.clone());
        variables.len() - 1
    };
    if !result.variables.contains(&index) {
        result.variables.push(index);
    }
}

fn tail_at(ty: &Type, label: Option<&Elem>, variables: &mut Vec<Type>, result: &mut Union) {
    match ty {
        Type::Without { row, gone } => {
            if !label.is_some_and(|l| gone.contains(l)) {
                tail_at(row, label, variables, result);
            }
        }
        Type::Union(a, b) => {
            tail_at(a, label, variables, result);
            tail_at(b, label, variables, result);
        }
        _ => variable(variables, ty, result),
    }
}

/// Membership at one nominal instance, or at a label not explicitly named.
/// Distinct open payload types may coincide after a future substitution;
/// their membership stays a variable, rather than assuming disjointness.
#[derive(Clone, Copy)]
enum Payload<'a> {
    Any,
    Instance(&'a Type),
    Other,
}

fn at(row: &Norm, label: Option<&Elem>, payload: Payload<'_>, variables: &mut Vec<Type>) -> Union {
    let mut result = Union::default();
    for (entry, ty) in &row.entries {
        if matches!(entry, Elem::Capture(_) | Elem::Error(_) | Elem::Service(_)) {
            variable(variables, ty, &mut result);
        } else if label == Some(entry) {
            match payload {
                Payload::Any => result.known = true,
                Payload::Instance(expected) if ty == expected => result.known = true,
                Payload::Instance(_) => variable(variables, ty, &mut result),
                Payload::Other => {}
            }
        }
    }
    for tail in &row.tails {
        tail_at(tail, label, variables, &mut result);
    }
    result
}

fn normalize(checker: &Checker<'_>, kind: RowKind, ty: &Type) -> Norm {
    if kind == RowKind::Captures {
        let mut result = Norm::default();
        checker.capture_row(ty, &mut result);
        result
    } else {
        checker.store.normalize(ty)
    }
}

pub fn consistent(checker: &Checker<'_>, equations: &[Equation]) -> bool {
    if equations.is_empty() {
        return true;
    }
    [RowKind::Services, RowKind::Errors, RowKind::Captures]
        .into_iter()
        .all(|kind| {
            let rows: Vec<_> = equations
                .iter()
                .filter(|e| e.kind == kind)
                .flat_map(|e| {
                    [
                        normalize(checker, kind, &e.left),
                        normalize(checker, kind, &e.right),
                    ]
                })
                .collect();
            labels(&rows).into_iter().all(|label| {
                let mut variables = Vec::new();
                let mut theory = Theory::default();
                for pair in rows.chunks_exact(2) {
                    theory.equal(
                        &at(&pair[0], label.as_ref(), Payload::Any, &mut variables),
                        &at(&pair[1], label.as_ref(), Payload::Any, &mut variables),
                    );
                }
                theory.consistent()
            })
        })
}

impl Checker<'_> {
    pub(in crate::types) fn inferred_row_kind(&self, ty: &Type) -> Option<RowKind> {
        match self.store.resolve(ty) {
            Type::Row(row) => {
                if row
                    .entries
                    .iter()
                    .any(|(l, _)| matches!(l, Elem::Capture(_)))
                {
                    Some(RowKind::Captures)
                } else {
                    row.tail.as_deref().and_then(|t| self.inferred_row_kind(t))
                }
            }
            Type::Union(a, b) => self
                .inferred_row_kind(&a)
                .or_else(|| self.inferred_row_kind(&b)),
            Type::Without { row, .. } => self.inferred_row_kind(&row),
            t => self.atom_kind(&t, &[]).as_ref().and_then(RowKind::of),
        }
    }

    pub(in crate::types) fn row_implied(&self, left: &Type, right: &Type, kind: RowKind) -> bool {
        let equations: Vec<_> = self
            .store
            .row_equalities
            .iter()
            .filter(|e| e.kind == kind)
            .collect();
        let mut rows: Vec<_> = equations
            .iter()
            .flat_map(|e| {
                [
                    normalize(self, kind, &e.left),
                    normalize(self, kind, &e.right),
                ]
            })
            .collect();
        let boundary = rows.len();
        rows.extend([normalize(self, kind, left), normalize(self, kind, right)]);
        // Each observed nominal payload and one distinct, unseen payload
        // form the finite universe. An equality never identifies S Int and
        // S String merely because their unparameterized names agree.
        for row in &mut rows {
            for (_, ty) in &mut row.entries {
                *ty = self.store.zonk(ty);
            }
        }
        labels(&rows).into_iter().all(|label| {
            let mut payloads = Vec::new();
            if let Some(label) = &label {
                for row in &rows {
                    if let Some(ty) = row.get(label)
                        && !payloads.contains(&ty)
                    {
                        payloads.push(ty);
                    }
                }
            }
            payloads
                .into_iter()
                .map(Payload::Instance)
                .chain([Payload::Other])
                .all(|payload| {
                    let mut variables = Vec::new();
                    let mut theory = Theory::default();
                    for pair in rows[..boundary].chunks_exact(2) {
                        theory.equal(
                            &at(&pair[0], label.as_ref(), payload, &mut variables),
                            &at(&pair[1], label.as_ref(), payload, &mut variables),
                        );
                    }
                    theory.entails(
                        &at(&rows[boundary], label.as_ref(), payload, &mut variables),
                        &at(&rows[boundary + 1], label.as_ref(), payload, &mut variables),
                    )
                })
        })
    }

    /// Publish an inferred effect row without leaking a constructor's
    /// existential row through a handler. The replacement is a proven upper
    /// bound at the receiving meta's scope, valid for every residual.
    pub(in crate::types) fn grow_scoped_row(
        &mut self,
        actual: &Norm,
        kind: RowKind,
        target: u32,
    ) -> bool {
        if self.store.row_equalities.is_empty() && self.store.equalities.is_empty() {
            return false;
        }
        let Some(level) = self.store.meta_level(target) else {
            return false;
        };
        let visible = |ty: &Type| {
            !ty.contains(&mut |t| match t {
                Type::Meta(_) => true,
                Type::Skolem(s) => self.store.skolem_level(*s) > level,
                _ => false,
            })
        };
        let actual_ty = actual.clone().into_type();
        if visible(&actual_ty) {
            return false;
        }
        let mut candidates: Vec<Type> = self
            .store
            .row_equalities
            .iter()
            .filter(|e| e.kind == kind)
            .flat_map(|e| [e.left.clone(), e.right.clone()])
            .collect();
        for e in &self.store.equalities {
            if self.inferred_row_kind(&Type::Skolem(e.skolem)) == Some(kind) {
                candidates.extend([Type::Skolem(e.skolem), e.ty.clone()]);
            }
        }
        let mut exposed = actual.clone();
        let mut changed = false;
        for tail in &mut exposed.tails {
            if visible(tail) {
                continue;
            }
            let mut upper = None;
            for candidate in &candidates {
                if visible(candidate) && self.row_implied(tail, candidate, kind) {
                    let smaller = upper.as_ref().is_none_or(|old| {
                        self.row_implied(candidate, old, kind)
                            && !self.row_implied(old, candidate, kind)
                    });
                    if smaller {
                        upper = Some(candidate.clone());
                    }
                }
            }
            let Some(upper) = upper else { return false };
            *tail = upper;
            changed = true;
        }
        let upper = exposed.into_type();
        if !changed || !visible(&upper) {
            return false;
        }
        // Do not rewrite this lexical upper bound by a branch assumption
        // while storing it. The proof above already discharged that branch;
        // the regular binder still checks occurs, scope and row restrictions.
        let equalities = mem::take(&mut self.store.equalities);
        let rows = mem::take(&mut self.store.row_equalities);
        let rest = self.store.fresh_beside(target);
        let result = self
            .bind(target, &Type::Union(Box::new(upper), Box::new(rest)))
            .is_ok();
        self.store.equalities = equalities;
        self.store.row_equalities = rows;
        result
    }
}
