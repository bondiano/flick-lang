//! Order-independent greatest-fixed-point inference for data parameter variance.

use std::collections::HashMap;

use super::{Data, Key, Variance};
use crate::types::Checker;
use crate::types::elem::{Atom, Elem};
use crate::types::kinds::Kind;
use crate::types::lang::Lang;
use crate::types::ty::Type;

/// Candidate variance set for one parameter during greatest-fixed-point
/// inference. `bi` is exact and static (disjoint from `pos`/`neg`); `pos` and
/// `neg` shrink monotonically until a unique valid assignment (`Sig.var_ok`)
/// remains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cand {
    bi: bool,
    pos: bool,
    neg: bool,
}

impl Cand {
    const BI: Self = Self {
        bi: true,
        pos: false,
        neg: false,
    };
    const POS: Self = Self {
        bi: false,
        pos: true,
        neg: false,
    };
    const NEG: Self = Self {
        bi: false,
        pos: false,
        neg: true,
    };
    const INV: Self = Self {
        bi: false,
        pos: false,
        neg: false,
    };
    const POS_OR_NEG: Self = Self {
        bi: false,
        pos: true,
        neg: true,
    };

    fn exact(variance: Variance) -> Self {
        match variance {
            Variance::Bivariant => Self::BI,
            Variance::Covariant => Self::POS,
            Variance::Contravariant => Self::NEG,
            Variance::Invariant => Self::INV,
        }
    }

    fn finalize(self) -> Variance {
        if self.bi {
            Variance::Bivariant
        } else if self.pos {
            Variance::Covariant
        } else if self.neg {
            Variance::Contravariant
        } else {
            Variance::Invariant
        }
    }
}

/// Whether `variance` (`Covariant` or `Contravariant`) may stand directly at
/// polarity `positive` with `flipped` recording whether any flip sits above.
/// A covariant parameter never stands below a flip (Lean `Ty.Pol.arrow`).
fn at_ok(variance: Variance, positive: bool, flipped: bool) -> bool {
    matches!(
        (variance, positive, flipped),
        (Variance::Covariant, true, false) | (Variance::Contravariant, false, _)
    )
}

fn names(l: &Elem, a: &str) -> bool {
    match l {
        Elem::Capture(Atom::Var(n)) | Elem::Error(Atom::Var(n)) | Elem::Service(Atom::Var(n)) => {
            n == a
        }
        Elem::Provided(e) => names(e, a),
        _ => false,
    }
}

/// Whether `a` stands anywhere in `t`, rows and qualifiers included.
pub(super) fn occurs_in(a: &str, t: &Type) -> bool {
    occurs(a, t)
}

/// Whether `a` stands anywhere in `t`, rows and qualifiers included.
fn occurs(a: &str, t: &Type) -> bool {
    t.contains(&mut |t| match t {
        Type::Var(n) => n == a,
        Type::Record(r) | Type::Row(r) => r.entries.iter().any(|(l, _)| names(l, a)),
        _ => false,
    })
}

/// `t` without the qualifiers of a value of it.
pub(super) fn bare(mut t: &Type) -> &Type {
    while let Type::Captured { ty, .. } | Type::Bounded { ty, .. } = t {
        t = ty;
    }
    t
}

/// The fields of a constructor's scheme, and the type it builds.
pub(super) fn fields(scheme: &Type) -> (Vec<&Type>, &Type) {
    let mut t = match scheme {
        Type::Forall { body, .. } => &**body,
        t => t,
    };
    let mut out = Vec::new();
    while let Type::Fun { param, result, .. } = t {
        out.push(&**param);
        t = result;
    }
    (out, bare(t))
}

impl Checker<'_> {
    pub(super) fn solve_group(&self, key: &Key) -> HashMap<Key, Vec<Variance>> {
        let Some(root) = self.data(key) else {
            return HashMap::new();
        };
        if let Some(exported) = root.exported {
            return HashMap::from([(key.clone(), exported.to_vec())]);
        }
        if root.ctors.is_none() {
            return HashMap::from([(key.clone(), vec![Variance::Invariant; root.kinds.len()])]);
        }
        // Only types without validated exported variance and with visible
        // constructors belong to the mutable group; imported declarations with
        // serialized variance and opaque/hidden-constructor types stay fixed.
        let mut group: Vec<Key> = Vec::new();
        let mut todo = vec![key.clone()];
        while let Some(k) = todo.pop() {
            if group.contains(&k) || self.signs.contains_key(&k) {
                continue;
            }
            let Some(data) = self.data(&k) else { continue };
            if data.exported.is_some() {
                continue;
            }
            let Some(ctors) = data.ctors else { continue };
            for scheme in ctors {
                scheme.visit(&mut |t| {
                    if let Type::Con { module, name } = t {
                        todo.push((module.clone(), name.clone()));
                    }
                });
            }
            group.push(k);
        }
        let mut current: HashMap<Key, Vec<Cand>> = group
            .iter()
            .map(|k| (k.clone(), self.initial_candidates(k)))
            .collect();
        loop {
            loop {
                let mut changed = false;
                for k in &group {
                    let next = self.refine_candidates(k, &current);
                    if next != current[k] {
                        current.insert(k.clone(), next);
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            // Any parameter still allowing both `+` and `-` (pure positive
            // cycles or closed-index-only GADTs) tie-breaks simultaneously to
            // `+`, then re-converges so dependents see the chosen sign.
            let mut broke_tie = false;
            for cands in current.values_mut() {
                for cand in cands {
                    if cand.pos && cand.neg {
                        cand.neg = false;
                        broke_tie = true;
                    }
                }
            }
            if !broke_tie {
                break;
            }
        }
        current
            .into_iter()
            .map(|(k, cands)| (k, cands.into_iter().map(Cand::finalize).collect()))
            .collect()
    }

    fn initial_candidates(&self, k: &Key) -> Vec<Cand> {
        let Some(data) = self.data(k) else {
            return Vec::new();
        };
        if let Some(exported) = data.exported {
            return exported.iter().copied().map(Cand::exact).collect();
        }
        let Some(ctors) = data.ctors else {
            return vec![Cand::INV; data.kinds.len()];
        };
        data.kinds
            .iter()
            .enumerate()
            .map(|(j, kind)| {
                if *kind != Kind::Type {
                    return Cand::INV;
                }
                if Self::is_unused_param(&ctors, j) {
                    Cand::BI
                } else {
                    Cand::POS_OR_NEG
                }
            })
            .collect()
    }

    /// Whether parameter `j` is truly bivariant (Lean `Sig.var_ok`): every
    /// constructor builds an ordinary variable alone at `j` and mentions it in
    /// no field. This is syntactic and independent of other types' variances.
    fn is_unused_param(ctors: &[&Type], j: usize) -> bool {
        ctors.iter().all(|scheme| {
            let (fields, res) = fields(scheme);
            let (_, res) = res.spine();
            let Some(r) = res.get(j) else {
                return false;
            };
            let alone = |v: &str| !(res.iter().enumerate()).any(|(m, o)| m != j && occurs(v, o));
            matches!(bare(r), Type::Var(v) if alone(v) && fields.iter().all(|f| !occurs(v, f)))
        })
    }

    fn refine_candidates(&self, k: &Key, current: &HashMap<Key, Vec<Cand>>) -> Vec<Cand> {
        let old = &current[k];
        let Some(Data {
            ctors: Some(ctors), ..
        }) = self.data(k)
        else {
            return old.clone();
        };
        old.iter()
            .enumerate()
            .map(|(j, &cand)| {
                if cand.bi || (!cand.pos && !cand.neg) {
                    return cand;
                }
                Cand {
                    bi: false,
                    pos: cand.pos && self.ctors_allow(current, &ctors, j, Variance::Covariant),
                    neg: cand.neg && self.ctors_allow(current, &ctors, j, Variance::Contravariant),
                }
            })
            .collect()
    }

    fn ctors_allow(
        &self,
        current: &HashMap<Key, Vec<Cand>>,
        ctors: &[&Type],
        j: usize,
        sign: Variance,
    ) -> bool {
        ctors.iter().all(|scheme| {
            let (fields, res) = fields(scheme);
            let (_, res) = res.spine();
            let Some(r) = res.get(j) else {
                return false;
            };
            let alone = |v: &str| !(res.iter().enumerate()).any(|(m, o)| m != j && occurs(v, o));
            match bare(r) {
                Type::Var(v) if alone(v) => fields
                    .iter()
                    .all(|f| self.pol_ok(current, v, sign, true, false, f)),
                c if self.closed(c, &mut Vec::new()) => true,
                _ => false,
            }
        })
    }

    fn cand_of(
        &self,
        current: &HashMap<Key, Vec<Cand>>,
        head: &Type,
        key: &Key,
    ) -> Option<Vec<Cand>> {
        if let Some(cands) = current.get(key) {
            return Some(cands.clone());
        }
        if let Some(found) = self.signs.get(key) {
            return found
                .as_ref()
                .map(|vs| vs.iter().copied().map(Cand::exact).collect());
        }
        if let Some(exported) = self.data(key).and_then(|d| d.exported) {
            return Some(exported.iter().copied().map(Cand::exact).collect());
        }
        head.is_lang(Lang::List).then(|| vec![Cand::POS])
    }

    /// Whether `parameter` with candidate variance `sign` (`Covariant` or
    /// `Contravariant`) may stand in `ty` at polarity `positive` (with
    /// `flipped` once any contravariant step lies above).
    fn pol_ok(
        &self,
        current: &HashMap<Key, Vec<Cand>>,
        parameter: &str,
        sign: Variance,
        positive: bool,
        flipped: bool,
        ty: &Type,
    ) -> bool {
        match ty {
            Type::Var(name) if name == parameter => at_ok(sign, positive, flipped),
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let effect_free = !effects.as_ref().is_some_and(|errors| {
                    occurs(parameter, &errors.services) || occurs(parameter, &errors.errors)
                });
                effect_free
                    && self.pol_ok(current, parameter, sign, !positive, true, param)
                    && self.pol_ok(current, parameter, sign, positive, flipped, result)
            }
            Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
                (!occurs(parameter, captures) || at_ok(sign, positive, flipped))
                    && self.pol_ok(current, parameter, sign, positive, flipped, ty)
            }
            Type::Record(row) if !row.tail.as_ref().is_some_and(|ty| occurs(parameter, ty)) => {
                row.entries.iter().all(|(label, field)| {
                    !names(label, parameter)
                        && self.pol_ok(current, parameter, sign, positive, flipped, field)
                })
            }
            Type::Tuple(items) => items
                .iter()
                .all(|item| self.pol_ok(current, parameter, sign, positive, flipped, item)),
            Type::App { .. } => {
                let (head, args) = ty.spine();
                match (head, &args[..]) {
                    (callee, [row, errors, item]) if callee.is_lang(Lang::Eff) => {
                        !occurs(parameter, row)
                            && !occurs(parameter, errors)
                            && self.pol_ok(current, parameter, sign, positive, flipped, item)
                    }
                    (Type::Con { module, name }, _) => {
                        let key = (module.clone(), name.clone());
                        let theirs = self.cand_of(current, head, &key);
                        args.into_iter().enumerate().all(|(index, item)| {
                            if !occurs(parameter, item) {
                                return true;
                            }
                            let Some(cand) = theirs.as_ref().and_then(|s| s.get(index)).copied()
                            else {
                                return false;
                            };
                            // An unused (`bi`) parameter still reaches regions, so `item` must
                            // satisfy the covariant reachability check (Lean `Ty.Pol.data`).
                            let ok_pos = (cand.bi || cand.pos)
                                && self.pol_ok(current, parameter, sign, positive, flipped, item);
                            let ok_neg = cand.neg
                                && self.pol_ok(current, parameter, sign, !positive, true, item);
                            ok_pos || ok_neg
                        })
                    }
                    _ => !occurs(parameter, ty),
                }
            }
            _ => !occurs(parameter, ty),
        }
    }
}
