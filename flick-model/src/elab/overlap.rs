//! Symmetric K1' overlap solver (`Formal/Elab/Hyp.lean`: `Coherent.k1'`).

use std::collections::{HashMap, HashSet, VecDeque};

use super::sub::{live_service, split_caps};
use super::{
    Constraint, InstanceDecl, SubstVal, TypeSubst, class_sign, data_sign, reach_of, subst_type,
};
use crate::decls::Decls;
use crate::syntax::Name;
use crate::types::{Caps, Kind, Row, Stage, Type, Var};
use crate::variance::{Sign, occurs, spine};

// ---------------------------------------------------------------------------
// K1' symmetric overlap check (`Formal/Elab/Hyp.lean`: `Coherent.k1'`)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OverlapMode {
    /// `∃ σ. t₁[σ] = t₂[σ]` (invariant or `bi` class parameter).
    Equal,
    /// `∃ σ, Q. Q ≤ t₁[σ] ∧ Q ≤ t₂[σ] ∧ ⌈t₁[σ]⌉ = ⌈Q⌉ = ⌈t₂[σ]⌉` (`-` class parameter).
    CommonLowerReach,
    /// `∃ σ, Q. Q ≤ t₁[σ] ∧ Q ≤ t₂[σ]` (contravariant position without outer reach equality).
    CommonLower,
    /// `∃ σ, Q. t₁[σ] ≤ Q ∧ t₂[σ] ≤ Q` (`+` class parameter).
    CommonUpper,
    /// `t₁` and `t₂` can both constrain the same head variable (compatible type skeleton).
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

    /// Drops the outer reach-equality check (`CommonLowerReach` ->
    /// `CommonLower`) when descending into covariant sub-positions: reach
    /// equality applies to the outer `-` class argument, not individually to
    /// each covariant child when a sibling subterm supplies the remaining reach.
    fn relax_reach(self) -> Self {
        match self {
            OverlapMode::CommonLowerReach => OverlapMode::CommonLower,
            other => other,
        }
    }
}

struct OverlapSolver<'a> {
    decls: &'a Decls,
    vars: HashSet<Name>,
    /// Union-find / equality bindings for fresh head variables.
    bindings: HashMap<Name, Type>,
    /// Deferred non-equality constraints on unbound variables, checked for
    /// pairwise shape and reach compatibility once all equalities propagate.
    lower_reach: HashMap<Name, Vec<Type>>,
    lower: HashMap<Name, Vec<Type>>,
    upper: HashMap<Name, Vec<Type>>,
    compat: HashMap<Name, Vec<Type>>,
    queue: VecDeque<(OverlapMode, Type, Type)>,
    seen_compat: HashSet<(Name, Name)>,
}

impl<'a> OverlapSolver<'a> {
    fn new(decls: &'a Decls, vars: HashSet<Name>) -> Self {
        Self {
            decls,
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

    fn is_var(&self, t: &Type) -> Option<Name> {
        match t {
            Type::Var(Var::Named(n)) if self.vars.contains(n) => Some(n.clone()),
            _ => None,
        }
    }

    fn resolve(&self, t: &Type) -> Type {
        let mut cur = t.clone();
        let mut guard = 0;
        while let Type::Var(Var::Named(n)) = &cur {
            let Some(next) = self.bindings.get(n) else {
                break;
            };
            cur = next.clone();
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        cur
    }

    fn apply_bindings(&self, t: &Type) -> Type {
        let sigma: TypeSubst = self
            .bindings
            .iter()
            .map(|(k, v)| (k.clone(), SubstVal::Type(self.resolve(v))))
            .collect();
        subst_type(self.decls, t, &sigma)
    }

    fn has_any_head_var(&self, t: &Type) -> bool {
        self.vars.iter().any(|v| occurs(v, t))
    }

    fn bind_var(&mut self, v: &Name, t: &Type) -> bool {
        let t = self.apply_bindings(t);
        if self.is_var(&t).as_ref() == Some(v) {
            return true;
        }
        if occurs(v, &t) {
            return false;
        }
        self.bindings.insert(v.clone(), t.clone());
        if let Some(xs) = self.lower_reach.remove(v) {
            for x in xs {
                self.queue
                    .push_back((OverlapMode::CommonLowerReach, t.clone(), x));
            }
        }
        if let Some(xs) = self.lower.remove(v) {
            for x in xs {
                self.queue
                    .push_back((OverlapMode::CommonLower, t.clone(), x));
            }
        }
        if let Some(xs) = self.upper.remove(v) {
            for x in xs {
                self.queue
                    .push_back((OverlapMode::CommonUpper, t.clone(), x));
            }
        }
        if let Some(xs) = self.compat.remove(v) {
            for x in xs {
                self.queue
                    .push_back((OverlapMode::ShapeCompat, t.clone(), x));
            }
        }
        true
    }

    fn record_var_constraint(&mut self, mode: OverlapMode, v: Name, other: Type) {
        if self.is_var(&other).as_ref() == Some(&v) {
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
                if !self.has_any_head_var(&prev) && !self.has_any_head_var(&other) {
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

    fn solve(mut self) -> bool {
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

            // Reach equality check for `-` (`CommonLowerReach`): when neither
            // side mentions a head variable (e.g. rigid regions), a common
            // lower bound keeping reach requires equal reach sets.
            if mode == OverlapMode::CommonLowerReach
                && !self.has_any_head_var(&a)
                && !self.has_any_head_var(&b)
                && reach_of(self.decls, &a) != reach_of(self.decls, &b)
            {
                return false;
            }

            let (ba, ca) = split_caps(&a);
            let (bb, cb) = split_caps(&b);
            if mode == OverlapMode::Equal && !self.caps_unifiable(&ca, &cb) {
                return false;
            }

            if !self.decompose(mode, ba, bb) {
                return false;
            }
        }
        true
    }

    fn caps_unifiable(&self, a: &Caps, b: &Caps) -> bool {
        let a_has_var = a
            .vars
            .iter()
            .any(|v| matches!(v, Var::Named(n) if self.vars.contains(n)));
        let b_has_var = b
            .vars
            .iter()
            .any(|v| matches!(v, Var::Named(n) if self.vars.contains(n)));
        if a_has_var || b_has_var {
            return true;
        }
        a == b
    }

    fn rows_compatible(&mut self, mode: OverlapMode, r1: &Row, r2: &Row) -> bool {
        let r1_open = r1
            .vars
            .iter()
            .any(|v| matches!(v, Var::Named(n) if self.vars.contains(n)));
        let r2_open = r2
            .vars
            .iter()
            .any(|v| matches!(v, Var::Named(n) if self.vars.contains(n)));
        match mode {
            OverlapMode::Equal => {
                if !r1_open && !r2_open && (r1.elems.len() != r2.elems.len() || r1.vars != r2.vars)
                {
                    return false;
                }
                for (k, a1) in &r1.elems {
                    if let Some(a2) = r2.elems.get(k) {
                        if a1.len() != a2.len() {
                            return false;
                        }
                        for (x, y) in a1.iter().zip(a2.iter()) {
                            self.queue
                                .push_back((OverlapMode::Equal, x.clone(), y.clone()));
                        }
                    } else if !r2_open {
                        return false;
                    }
                }
                for k in r2.elems.keys() {
                    if !r1.elems.contains_key(k) && !r1_open {
                        return false;
                    }
                }
                true
            }
            // Any two service/error rows share `Row::none()` as a lower bound.
            OverlapMode::CommonLowerReach | OverlapMode::CommonLower | OverlapMode::ShapeCompat => {
                true
            }
            // Common upper bound exists iff shared service/error names have
            // unifiable type arguments (`Row::sum`).
            OverlapMode::CommonUpper => {
                for (k, a1) in &r1.elems {
                    if *k == live_service() {
                        continue;
                    }
                    if let Some(a2) = r2.elems.get(k) {
                        if a1.len() != a2.len() {
                            return false;
                        }
                        for (x, y) in a1.iter().zip(a2.iter()) {
                            self.queue
                                .push_back((OverlapMode::Equal, x.clone(), y.clone()));
                        }
                    }
                }
                true
            }
        }
    }

    fn stages_compatible(&mut self, mode: OverlapMode, s1: &Stage, s2: &Stage) -> bool {
        match (mode, s1, s2) {
            (OverlapMode::Equal, Stage::Pure, Stage::Pure)
            | (
                OverlapMode::CommonLowerReach | OverlapMode::CommonLower | OverlapMode::ShapeCompat,
                _,
                _,
            )
            | (OverlapMode::CommonUpper, Stage::Pure, _)
            | (OverlapMode::CommonUpper, _, Stage::Pure) => true,
            (OverlapMode::Equal, Stage::Runtime(r1, e1), Stage::Runtime(r2, e2)) => {
                self.rows_compatible(OverlapMode::Equal, r1, r2)
                    && self.rows_compatible(OverlapMode::Equal, e1, e2)
            }
            (OverlapMode::Equal, _, _) => false,
            (OverlapMode::CommonUpper, Stage::Runtime(r1, e1), Stage::Runtime(r2, e2)) => {
                self.rows_compatible(OverlapMode::CommonUpper, r1, r2)
                    && self.rows_compatible(OverlapMode::CommonUpper, e1, e2)
            }
        }
    }

    fn decompose(&mut self, mode: OverlapMode, a: &Type, b: &Type) -> bool {
        match (a, b) {
            (Type::Fun(p1, s1, r1), Type::Fun(p2, s2, r2)) => {
                if !self.stages_compatible(mode, s1, s2) {
                    return false;
                }
                self.queue
                    .push_back((mode.flip(), (**p1).clone(), (**p2).clone()));
                self.queue
                    .push_back((mode.relax_reach(), (**r1).clone(), (**r2).clone()));
                true
            }
            (Type::Eff(r1, e1, t1), Type::Eff(r2, e2, t2)) => {
                if !self.rows_compatible(mode, r1, r2) || !self.rows_compatible(mode, e1, e2) {
                    return false;
                }
                self.queue
                    .push_back((mode.relax_reach(), (**t1).clone(), (**t2).clone()));
                true
            }
            (Type::Tuple(xs), Type::Tuple(ys)) if xs.len() == ys.len() => {
                let elem_mode = mode.relax_reach();
                for (x, y) in xs.iter().zip(ys.iter()) {
                    self.queue.push_back((elem_mode, x.clone(), y.clone()));
                }
                true
            }
            (Type::Record(f), Type::Record(g))
                if f.tail.is_none() && g.tail.is_none() && f.fields.keys().eq(g.fields.keys()) =>
            {
                let field_mode = mode.relax_reach();
                for (x, y) in f.fields.values().zip(g.fields.values()) {
                    self.queue.push_back((field_mode, x.clone(), y.clone()));
                }
                true
            }
            (Type::App(..), Type::App(..)) => {
                let (Some((c1, xs)), Some((c2, ys))) = (spine(a), spine(b)) else {
                    return a == b;
                };
                if c1 != c2 || xs.len() != ys.len() {
                    return false;
                }
                for (j, (x, y)) in xs.iter().zip(ys.iter()).enumerate() {
                    let arg_mode = match (mode, data_sign(self.decls, c1, j)) {
                        (OverlapMode::Equal, _) | (_, Sign::Inv | Sign::Bi) => OverlapMode::Equal,
                        (_, Sign::Pos) => mode.relax_reach(),
                        (_, Sign::Neg) => mode.flip(),
                    };
                    self.queue.push_back((arg_mode, (*x).clone(), (*y).clone()));
                }
                true
            }
            (Type::Dict(c1, xs), Type::Dict(c2, ys)) if c1 == c2 && xs.len() == ys.len() => {
                for (j, (x, y)) in xs.iter().zip(ys.iter()).enumerate() {
                    let arg_mode = match (mode, class_sign(self.decls, c1, j)) {
                        (OverlapMode::Equal, _) | (_, Sign::Inv | Sign::Bi) => OverlapMode::Equal,
                        (_, Sign::Pos) => mode.relax_reach(),
                        (_, Sign::Neg) => match mode {
                            OverlapMode::CommonUpper => OverlapMode::CommonLowerReach,
                            other => other.flip(),
                        },
                    };
                    self.queue.push_back((arg_mode, x.clone(), y.clone()));
                }
                true
            }
            (Type::Ref(x), Type::Ref(y)) => {
                self.queue
                    .push_back((OverlapMode::Equal, (**x).clone(), (**y).clone()));
                true
            }
            (Type::Errors(r1), Type::Errors(r2))
            | (Type::Impl(r1), Type::Impl(r2))
            | (Type::Tag(r1), Type::Tag(r2))
            | (Type::Env(r1), Type::Env(r2)) => self.rows_compatible(OverlapMode::Equal, r1, r2),
            _ => a == b,
        }
    }
}

fn freshen_instance(
    decls: &Decls,
    inst: &InstanceDecl,
    prefix: &str,
) -> (HashSet<Name>, Constraint) {
    let mut vars = HashSet::new();
    let mut sigma = TypeSubst::new();
    for (i, (v, k)) in inst.vars.iter().enumerate() {
        let fresh = Name::from(format!("{prefix}${i}"));
        vars.insert(fresh.clone());
        let val = match k {
            Kind::Services | Kind::Errors => SubstVal::Row(Row::var(Var::Named(fresh))),
            Kind::Captures => SubstVal::Caps(Caps::var(Var::Named(fresh))),
            _ => SubstVal::Type(Type::Var(Var::Named(fresh))),
        };
        sigma.insert(v.clone(), val);
    }
    (vars, inst.head.subst(decls, &sigma))
}

/// Symmetric K1' overlap predicate: returns `true` iff `a` and `b` are
/// instances of the same class whose instantiated heads can both serve a
/// common query `Q` under [`serves`].
pub fn instances_overlap(decls: &Decls, a: &InstanceDecl, b: &InstanceDecl) -> bool {
    if a.head.class != b.head.class || a.head.args.len() != b.head.args.len() {
        return false;
    }
    let (mut vars, head_a) = freshen_instance(decls, a, "L");
    let (vars_b, head_b) = freshen_instance(decls, b, "R");
    vars.extend(vars_b);
    let mut solver = OverlapSolver::new(decls, vars);
    for (j, (x, y)) in head_a.args.into_iter().zip(head_b.args).enumerate() {
        let mode = match class_sign(decls, &a.head.class, j) {
            Sign::Pos => OverlapMode::CommonUpper,
            Sign::Neg => OverlapMode::CommonLowerReach,
            Sign::Inv | Sign::Bi => OverlapMode::Equal,
        };
        solver.queue.push_back((mode, x, y));
    }
    solver.solve()
}
