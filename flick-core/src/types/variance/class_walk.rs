//! Polarity walking over superclass constraints and method types for class
//! parameter variance inference (`class_solve.rs`).

use std::collections::HashMap;

use super::class_solve::Uses;
use super::solve::occurs_in;
use super::{Key, Variance};
use crate::types::Checker;
use crate::types::classes::split;
use crate::types::elem::{Atom, Elem};
use crate::types::lang::Lang;
use crate::types::ty::Type;

fn label_names(label: &Elem, a: &str) -> bool {
    match label {
        Elem::Capture(Atom::Var(n)) | Elem::Error(Atom::Var(n)) | Elem::Service(Atom::Var(n)) => {
            n == a
        }
        Elem::Provided(inner) => label_names(inner, a),
        _ => false,
    }
}

/// Whether `a` occurs free in `ty`, respecting inner `Forall` binders.
pub(super) fn occurs_free(a: &str, ty: &Type) -> bool {
    match ty {
        Type::Var(n) => n == a,
        Type::Con { .. } | Type::Symbol(_) | Type::Meta(_) | Type::Skolem(_) | Type::Error => false,
        Type::App { head, arg } => occurs_free(a, head) || occurs_free(a, arg),
        Type::Fun {
            param,
            effects,
            result,
        } => {
            occurs_free(a, param)
                || effects
                    .as_ref()
                    .is_some_and(|e| occurs_free(a, &e.services) || occurs_free(a, &e.errors))
                || occurs_free(a, result)
        }
        Type::Tuple(items) => items.iter().any(|t| occurs_free(a, t)),
        Type::Record(row) | Type::Row(row) => {
            row.entries
                .iter()
                .any(|(l, t)| label_names(l, a) || occurs_free(a, t))
                || row.tail.as_deref().is_some_and(|t| occurs_free(a, t))
        }
        Type::Union(left, right) => occurs_free(a, left) || occurs_free(a, right),
        Type::Without { row, gone } => {
            occurs_free(a, row) || gone.iter().any(|l| label_names(l, a))
        }
        Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
            occurs_free(a, ty) || occurs_free(a, captures)
        }
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            !vars.iter().any(|b| b.name == a)
                && (constraints.iter().any(|c| occurs_free(a, c)) || occurs_free(a, body))
        }
        Type::Exists { contract, body, .. } => occurs_free(a, contract) || occurs_free(a, body),
    }
}

impl Checker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn walk_constraint(
        &self,
        current: &HashMap<Key, Vec<Variance>>,
        bi_is_fixed: bool,
        param: &str,
        constraint: &Type,
        positive: bool,
        flipped: bool,
        out: &mut Uses,
    ) {
        let expanded = self.witness_head(constraint);
        let Some(((cm, cn), args)) = split(&expanded) else {
            out.fixed(occurs_free(param, &expanded));
            return;
        };
        let theirs = self.lookup_class_variance(current, cm, cn);
        for (j, arg) in args.into_iter().enumerate() {
            if !occurs_free(param, arg) {
                continue;
            }
            match theirs
                .as_ref()
                .and_then(|s| s.get(j))
                .copied()
                .unwrap_or(Variance::Invariant)
            {
                Variance::Covariant => {
                    self.walk_class_ty(current, bi_is_fixed, param, arg, positive, flipped, out, 0);
                }
                Variance::Contravariant => {
                    self.walk_class_ty(current, bi_is_fixed, param, arg, !positive, true, out, 0);
                }
                Variance::Invariant => out.fixed(true),
                Variance::Bivariant if bi_is_fixed => out.fixed(true),
                Variance::Bivariant => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn walk_class_ty(
        &self,
        current: &HashMap<Key, Vec<Variance>>,
        bi_is_fixed: bool,
        param: &str,
        ty: &Type,
        positive: bool,
        flipped: bool,
        out: &mut Uses,
        depth: usize,
    ) {
        if depth > 64 {
            out.fixed(occurs_free(param, ty));
            return;
        }
        let head_ty = self.witness_head(ty);
        match &head_ty {
            Type::Var(name) if name == param => out.at(positive, flipped),
            Type::Fun {
                param: arg,
                effects,
                result,
            } => {
                if let Some(effects) = effects {
                    out.fixed(
                        occurs_free(param, &effects.services)
                            || occurs_free(param, &effects.errors),
                    );
                }
                self.walk_class_ty(
                    current,
                    bi_is_fixed,
                    param,
                    arg,
                    !positive,
                    true,
                    out,
                    depth + 1,
                );
                self.walk_class_ty(
                    current,
                    bi_is_fixed,
                    param,
                    result,
                    positive,
                    flipped,
                    out,
                    depth + 1,
                );
            }
            Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
                if occurs_free(param, captures) {
                    out.at(positive, flipped);
                }
                self.walk_class_ty(
                    current,
                    bi_is_fixed,
                    param,
                    ty,
                    positive,
                    flipped,
                    out,
                    depth + 1,
                );
            }
            Type::Record(row) if !row.tail.as_deref().is_some_and(|t| occurs_free(param, t)) => {
                for (label, field) in &row.entries {
                    out.fixed(label_names(label, param));
                    self.walk_class_ty(
                        current,
                        bi_is_fixed,
                        param,
                        field,
                        positive,
                        flipped,
                        out,
                        depth + 1,
                    );
                }
            }
            Type::Tuple(items) => {
                for item in items {
                    self.walk_class_ty(
                        current,
                        bi_is_fixed,
                        param,
                        item,
                        positive,
                        flipped,
                        out,
                        depth + 1,
                    );
                }
            }
            Type::App { .. } => {
                self.walk_class_app(
                    current,
                    bi_is_fixed,
                    param,
                    &head_ty,
                    positive,
                    flipped,
                    out,
                    depth,
                );
            }
            _ => out.fixed(occurs_free(param, &head_ty)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_class_app(
        &self,
        current: &HashMap<Key, Vec<Variance>>,
        bi_is_fixed: bool,
        param: &str,
        head_ty: &Type,
        positive: bool,
        flipped: bool,
        out: &mut Uses,
        depth: usize,
    ) {
        let (callee, args) = head_ty.spine();
        match (callee, &args[..]) {
            (eff, [services, errors, item]) if eff.is_lang(Lang::Eff) => {
                out.fixed(occurs_free(param, services) || occurs_free(param, errors));
                self.walk_class_ty(
                    current,
                    bi_is_fixed,
                    param,
                    item,
                    positive,
                    flipped,
                    out,
                    depth + 1,
                );
            }
            (Type::Con { module, name }, _) => {
                if let Some(class_name) = name.strip_suffix("$Dict")
                    && self.class_info(module, class_name).is_some()
                {
                    let theirs = self.lookup_class_variance(current, module, class_name);
                    for (j, arg) in args.into_iter().enumerate() {
                        if !occurs_free(param, arg) {
                            continue;
                        }
                        match theirs
                            .as_ref()
                            .and_then(|s| s.get(j))
                            .copied()
                            .unwrap_or(Variance::Invariant)
                        {
                            Variance::Covariant => self.walk_class_ty(
                                current,
                                bi_is_fixed,
                                param,
                                arg,
                                positive,
                                flipped,
                                out,
                                depth + 1,
                            ),
                            Variance::Contravariant => self.walk_class_ty(
                                current,
                                bi_is_fixed,
                                param,
                                arg,
                                !positive,
                                true,
                                out,
                                depth + 1,
                            ),
                            Variance::Invariant => out.fixed(true),
                            Variance::Bivariant if bi_is_fixed => out.fixed(true),
                            Variance::Bivariant => {}
                        }
                    }
                    return;
                }
                let theirs = self.compute_signs(module, name);
                for (j, arg) in args.into_iter().enumerate() {
                    if !occurs_free(param, arg) {
                        continue;
                    }
                    match theirs
                        .as_ref()
                        .and_then(|s| s.get(j))
                        .copied()
                        .unwrap_or(Variance::Invariant)
                    {
                        Variance::Covariant => self.walk_class_ty(
                            current,
                            bi_is_fixed,
                            param,
                            arg,
                            positive,
                            flipped,
                            out,
                            depth + 1,
                        ),
                        Variance::Contravariant => self.walk_class_ty(
                            current,
                            bi_is_fixed,
                            param,
                            arg,
                            !positive,
                            true,
                            out,
                            depth + 1,
                        ),
                        Variance::Invariant => out.fixed(true),
                        // Lean `inferTyVarArgs` / `Ty.PolD.data`: under a `bi`
                        // data parameter, occurrence requires `s.at q` (no flips).
                        Variance::Bivariant => out.at(positive, flipped),
                    }
                }
            }
            _ => out.fixed(occurs_in(param, head_ty)),
        }
    }
}
