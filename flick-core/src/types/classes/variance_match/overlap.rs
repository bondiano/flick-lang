//! Symmetric K1' overlap solver (`Formal/Elab/Hyp.lean`: `Coherent.k1'`).

use std::collections::{HashMap, HashSet, VecDeque};

use super::caps::{has_type_var_in, reach_of, row_without_live_norm, split_caps, stage_norm};
use crate::types::Checker;
use crate::types::classes::matching::Subst;
use crate::types::lang::Lang;
use crate::types::ty::{Effects, Norm, Type};
use crate::types::variance::Variance;

// ---------------------------------------------------------------------------
// Symmetric K1' overlap solver (`Formal/Elab/Hyp.lean`: `Coherent.k1'`)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum OverlapMode {
    /// `∃ σ. t₁[σ] = t₂[σ]` (`inv` or `bi` class parameter).
    Equal,
    /// `∃ σ, Q. Q ≤ t₁[σ] ∧ Q ≤ t₂[σ] ∧ ⌈t₁[σ]⌉ = ⌈Q⌉ = ⌈t₂[σ]⌉` (`-` class parameter).
    CommonLowerReach,
    /// `∃ σ, Q. Q ≤ t₁[σ] ∧ Q ≤ t₂[σ]` (contravariant sub-position).
    CommonLower,
    /// `∃ σ, Q. t₁[σ] ≤ Q ∧ t₂[σ] ≤ Q` (`+` class parameter).
    CommonUpper,
    /// Two constraints on the same head variable must share a compatible type shape.
    ShapeCompat,
}

impl OverlapMode {
    fn flip(self) -> Self {
        match self {
            OverlapMode::Equal => OverlapMode::Equal,
            OverlapMode::CommonLowerReach | OverlapMode::CommonLower => OverlapMode::CommonUpper,
            OverlapMode::CommonUpper => OverlapMode::CommonLower,
            OverlapMode::ShapeCompat => OverlapMode::ShapeCompat,
        }
    }

    /// Drops the outer reach-equality requirement (`CommonLowerReach` ->
    /// `CommonLower`) when descending into covariant sub-positions: reach
    /// equality holds for the outer `-` class argument as a whole, not
    /// individually for each covariant child when a sibling subterm supplies
    /// the remaining reach.
    fn relax_reach(self) -> Self {
        match self {
            OverlapMode::CommonLowerReach => OverlapMode::CommonLower,
            other => other,
        }
    }
}

pub(super) struct OverlapSolver<'c, 'a> {
    checker: &'c mut Checker<'a>,
    vars: HashSet<String>,
    bindings: HashMap<String, Type>,
    lower_reach: HashMap<String, Vec<Type>>,
    lower: HashMap<String, Vec<Type>>,
    upper: HashMap<String, Vec<Type>>,
    compat: HashMap<String, Vec<Type>>,
    pub(super) queue: VecDeque<(OverlapMode, Type, Type)>,
    seen_compat: HashSet<(String, String)>,
}

impl<'c, 'a> OverlapSolver<'c, 'a> {
    pub(super) fn new(checker: &'c mut Checker<'a>, vars: HashSet<String>) -> Self {
        Self {
            checker,
            vars,
            bindings: HashMap::new(),
            lower_reach: HashMap::new(),
            lower: HashMap::new(),
            upper: HashMap::new(),
            compat: HashMap::new(),
            queue: VecDeque::new(),
            seen_compat: HashSet::new(),
        }
    }

    fn is_var(&self, t: &Type) -> Option<String> {
        match t {
            Type::Var(v) if self.vars.contains(v) => Some(v.clone()),
            _ => None,
        }
    }

    fn resolve(&self, t: &Type) -> Type {
        let mut cur = t.clone();
        for _ in 0..64 {
            let Type::Var(v) = &cur else { break };
            let Some(next) = self.bindings.get(v) else {
                break;
            };
            cur = next.clone();
        }
        cur
    }

    pub(super) fn apply_bindings(&self, t: &Type) -> Type {
        let subst: Subst = self
            .bindings
            .iter()
            .map(|(k, v)| (k.clone(), self.resolve(v)))
            .collect();
        t.substitute(&subst)
    }

    fn bind_var(&mut self, v: &str, t: &Type) -> bool {
        let t = self.apply_bindings(t);
        if self.is_var(&t).as_deref() == Some(v) {
            return true;
        }
        if t.contains(&mut |n| matches!(n, Type::Var(w) if w == v)) {
            return false;
        }
        self.bindings.insert(v.to_owned(), t.clone());
        for (map, mode) in [
            (self.lower_reach.remove(v), OverlapMode::CommonLowerReach),
            (self.lower.remove(v), OverlapMode::CommonLower),
            (self.upper.remove(v), OverlapMode::CommonUpper),
            (self.compat.remove(v), OverlapMode::ShapeCompat),
        ] {
            if let Some(xs) = map {
                for x in xs {
                    self.queue.push_back((mode, t.clone(), x));
                }
            }
        }
        true
    }

    fn record_var_constraint(&mut self, mode: OverlapMode, v: String, other: Type) {
        if self.is_var(&other).as_deref() == Some(v.as_str()) {
            return;
        }
        if let Some(w) = self.is_var(&other) {
            let pair = if v <= w {
                (v.clone(), w)
            } else {
                (w, v.clone())
            };
            if !self.seen_compat.insert(pair) {
                return;
            }
        }
        let existing: Vec<Type> = self
            .lower_reach
            .get(&v)
            .into_iter()
            .flatten()
            .chain(self.lower.get(&v).into_iter().flatten())
            .chain(self.upper.get(&v).into_iter().flatten())
            .chain(self.compat.get(&v).into_iter().flatten())
            .cloned()
            .collect();
        for prev in existing {
            self.queue
                .push_back((OverlapMode::ShapeCompat, prev, other.clone()));
        }
        if mode == OverlapMode::CommonLowerReach {
            let prev_lr = self.lower_reach.get(&v).cloned().unwrap_or_default();
            for prev in prev_lr {
                if !has_type_var_in(&prev, &self.vars) && !has_type_var_in(&other, &self.vars) {
                    self.queue
                        .push_back((OverlapMode::CommonLowerReach, prev, other.clone()));
                }
            }
            self.lower_reach.entry(v).or_default().push(other);
        } else {
            match mode {
                OverlapMode::CommonLower => self.lower.entry(v).or_default().push(other),
                OverlapMode::CommonUpper => self.upper.entry(v).or_default().push(other),
                OverlapMode::ShapeCompat => self.compat.entry(v).or_default().push(other),
                OverlapMode::Equal | OverlapMode::CommonLowerReach => unreachable!(),
            }
        }
    }

    pub(super) fn solve(&mut self) -> bool {
        while let Some((mode, raw_a, raw_b)) = self.queue.pop_front() {
            let a = self.resolve(&raw_a);
            let b = self.resolve(&raw_b);
            if a == b {
                continue;
            }
            match (self.is_var(&a), self.is_var(&b)) {
                (Some(v), _) if mode == OverlapMode::Equal => {
                    if !self.bind_var(&v, &b) {
                        return false;
                    }
                    continue;
                }
                (_, Some(v)) if mode == OverlapMode::Equal => {
                    if !self.bind_var(&v, &a) {
                        return false;
                    }
                    continue;
                }
                (Some(v), _) => {
                    self.record_var_constraint(mode, v, b);
                    continue;
                }
                (_, Some(v)) => {
                    self.record_var_constraint(mode, v, a);
                    continue;
                }
                (None, None) => {}
            }

            if mode == OverlapMode::CommonLowerReach
                && !has_type_var_in(&a, &self.vars)
                && !has_type_var_in(&b, &self.vars)
                && reach_of(self.checker, &a) != reach_of(self.checker, &b)
            {
                return false;
            }

            let (ba, ca) = split_caps(self.checker, &a);
            let (bb, cb) = split_caps(self.checker, &b);
            if mode == OverlapMode::Equal && !self.caps_unifiable(&ca, &cb) {
                return false;
            }
            if !self.decompose(mode, &ba, &bb) {
                return false;
            }
        }
        true
    }

    fn caps_unifiable(&self, a: &Norm, b: &Norm) -> bool {
        let open = |n: &Norm| {
            n.tails
                .iter()
                .any(|t| matches!(t, Type::Var(v) if self.vars.contains(v)))
        };
        open(a) || open(b) || a == b
    }

    fn rows_compatible(
        &mut self,
        mode: OverlapMode,
        r1: &Type,
        r2: &Type,
        is_services: bool,
    ) -> bool {
        let n1 = if is_services {
            row_without_live_norm(self.checker, r1)
        } else {
            stage_norm(self.checker, r1)
        };
        let n2 = if is_services {
            row_without_live_norm(self.checker, r2)
        } else {
            stage_norm(self.checker, r2)
        };
        let open1 = n1
            .tails
            .iter()
            .any(|t| matches!(t, Type::Var(v) if self.vars.contains(v)));
        let open2 = n2
            .tails
            .iter()
            .any(|t| matches!(t, Type::Var(v) if self.vars.contains(v)));
        match mode {
            OverlapMode::Equal => {
                if !open1
                    && !open2
                    && (n1.entries.len() != n2.entries.len() || n1.tails != n2.tails)
                {
                    return false;
                }
                for (l, t1) in &n1.entries {
                    if let Some((_, t2)) = n2.entries.iter().find(|(el, _)| el == l) {
                        self.queue
                            .push_back((OverlapMode::Equal, t1.clone(), t2.clone()));
                    } else if !open2 {
                        return false;
                    }
                }
                for (l, _) in &n2.entries {
                    if !n1.entries.iter().any(|(el, _)| el == l) && !open1 {
                        return false;
                    }
                }
                true
            }
            OverlapMode::CommonLowerReach | OverlapMode::CommonLower | OverlapMode::ShapeCompat => {
                true
            }
            OverlapMode::CommonUpper => {
                for (l, t1) in &n1.entries {
                    if let Some((_, t2)) = n2.entries.iter().find(|(el, _)| el == l) {
                        self.queue
                            .push_back((OverlapMode::Equal, t1.clone(), t2.clone()));
                    }
                }
                true
            }
        }
    }

    fn stages_compatible(
        &mut self,
        mode: OverlapMode,
        e1: Option<&Effects>,
        e2: Option<&Effects>,
    ) -> bool {
        match (mode, e1, e2) {
            (OverlapMode::Equal, None, None)
            | (
                OverlapMode::CommonLowerReach | OverlapMode::CommonLower | OverlapMode::ShapeCompat,
                _,
                _,
            )
            | (OverlapMode::CommonUpper, None, _)
            | (OverlapMode::CommonUpper, _, None) => true,
            (OverlapMode::Equal, Some(x), Some(y)) => {
                x.sync == y.sync
                    && self.rows_compatible(OverlapMode::Equal, &x.services, &y.services, true)
                    && self.rows_compatible(OverlapMode::Equal, &x.errors, &y.errors, false)
            }
            (OverlapMode::Equal, _, _) => false,
            (OverlapMode::CommonUpper, Some(x), Some(y)) => {
                self.rows_compatible(OverlapMode::CommonUpper, &x.services, &y.services, true)
                    && self.rows_compatible(OverlapMode::CommonUpper, &x.errors, &y.errors, false)
            }
        }
    }

    fn decompose(&mut self, mode: OverlapMode, a: &Type, b: &Type) -> bool {
        match (a, b) {
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
                if !self.stages_compatible(mode, e1.as_deref(), e2.as_deref()) {
                    return false;
                }
                self.queue
                    .push_back((mode.flip(), (**p1).clone(), (**p2).clone()));
                self.queue
                    .push_back((mode.relax_reach(), (**r1).clone(), (**r2).clone()));
                true
            }
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
                let elem_mode = mode.relax_reach();
                for (x, y) in xs.iter().zip(ys) {
                    self.queue.push_back((elem_mode, x.clone(), y.clone()));
                }
                true
            }
            (Type::Record(r1), Type::Record(r2)) => {
                if mode != OverlapMode::Equal
                    && r1.tail.is_none()
                    && r2.tail.is_none()
                    && r1.entries.len() == r2.entries.len()
                    && r1.entries.iter().all(|(l, _)| r2.get(l).is_some())
                {
                    let field_mode = mode.relax_reach();
                    for (l, x) in &r1.entries {
                        if let Some(y) = r2.get(l) {
                            self.queue.push_back((field_mode, x.clone(), y.clone()));
                        }
                    }
                    true
                } else {
                    self.rows_compatible(
                        OverlapMode::Equal,
                        &Type::Row(r1.clone()),
                        &Type::Row(r2.clone()),
                        false,
                    )
                }
            }
            (Type::Row(_) | Type::Union(..), Type::Row(_) | Type::Union(..)) => {
                self.rows_compatible(mode, a, b, false)
            }
            (Type::App { .. }, Type::App { .. }) => self.decompose_app(mode, a, b),
            _ => a == b,
        }
    }

    fn decompose_app(&mut self, mode: OverlapMode, a: &Type, b: &Type) -> bool {
        let (h1, args1) = a.spine();
        let (h2, args2) = b.spine();
        if args1.len() != args2.len() {
            return false;
        }
        if h1.is_lang(Lang::Eff)
            && h2.is_lang(Lang::Eff)
            && let ([s1, e1, r1], [s2, e2, r2]) = (&args1[..], &args2[..])
        {
            if !self.rows_compatible(mode, s1, s2, true)
                || !self.rows_compatible(mode, e1, e2, false)
            {
                return false;
            }
            self.queue
                .push_back((mode.relax_reach(), (*r1).clone(), (*r2).clone()));
            return true;
        }
        if h1.is_lang(Lang::Layer)
            && h2.is_lang(Lang::Layer)
            && let ([i1, e1, o1], [i2, e2, o2]) = (&args1[..], &args2[..])
        {
            if !self.rows_compatible(mode, i1, i2, true)
                || !self.rows_compatible(mode, e1, e2, false)
            {
                return false;
            }
            self.queue
                .push_back((OverlapMode::Equal, (*o1).clone(), (*o2).clone()));
            return true;
        }
        let (
            Type::Con {
                module: m1,
                name: n1,
            },
            Type::Con {
                module: m2,
                name: n2,
            },
        ) = (h1, h2)
        else {
            // Higher-kinded application `f a` vs `g b`: exact unification.
            self.queue
                .push_back((OverlapMode::Equal, h1.clone(), h2.clone()));
            for (x, y) in args1.into_iter().zip(args2) {
                self.queue
                    .push_back((OverlapMode::Equal, x.clone(), y.clone()));
            }
            return true;
        };
        if m1 != m2 || n1 != n2 {
            return false;
        }
        let (m1, n1) = (m1.clone(), n1.clone());
        let args1: Vec<Type> = args1.into_iter().cloned().collect();
        let args2: Vec<Type> = args2.into_iter().cloned().collect();
        let signs = self.checker.app_variances(&m1, &n1, args1.len());
        let is_dict = n1.ends_with("$Dict");
        for (j, (x, y)) in args1.into_iter().zip(args2).enumerate() {
            let sign = signs
                .as_ref()
                .and_then(|s| s.get(j))
                .copied()
                .unwrap_or(Variance::Invariant);
            let arg_mode = match (mode, sign) {
                (OverlapMode::Equal, _) | (_, Variance::Invariant | Variance::Bivariant) => {
                    OverlapMode::Equal
                }
                (_, Variance::Covariant) => mode.relax_reach(),
                (_, Variance::Contravariant) => match (is_dict, mode) {
                    (true, OverlapMode::CommonUpper) => OverlapMode::CommonLowerReach,
                    _ => mode.flip(),
                },
            };
            self.queue.push_back((arg_mode, x, y));
        }
        true
    }
}
