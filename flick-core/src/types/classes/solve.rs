//! The instance solver. A constraint is solved by an assumption in scope or
//! one of its superclasses, by a row primitive, by the compiler-owned
//! instance of a record or tuple, or by the one instance whose head matches
//! it; functional dependencies improve unknown types on the way. Every step
//! counts against the budget of the definition being checked.

use std::mem;

use super::matching::{Match, Subst, match_type};
use super::{Dict, RECORD_CLASSES, has_metas, split, super_slot, vars, which};
use crate::numeric::{Num, default_num};
use crate::types::Checker;
use crate::types::gadt::branch::as_field_given;
use crate::types::env::Instance;
use crate::types::generalize::replace_metas;
use crate::types::ty::Type;

#[derive(Debug, Clone)]
pub enum Outcome {
    Solved(Dict),
    /// Unknown types keep it open; it may resolve once inference learns more.
    Stuck,
    /// No instance for this part of the constraint asked for; with the
    /// reason when a primitive gives one.
    Missing(Type, Option<String>),
    /// The budget ran out along this chain of constraints.
    Budget(Vec<Type>),
}

/// Assumptions with their dictionaries, superclasses included.
pub type Givens = [(Type, Dict)];

impl Checker<'_> {
    /// Solves the constraints that can be solved now; with `report`, every
    /// other one is an error, otherwise it waits for more information.
    pub fn solve_classes(&mut self, report: bool) {
        let mut pending = mem::take(&mut self.classes.wanted);
        loop {
            let mut left = Vec::new();
            let mut progress = false;
            for mut w in pending {
                // under the equalities of the GADT branches it is from
                let outcome = self.under(&w.givens, |this| {
                    let givens = this.closure(&w.givens);
                    let c = this.canon(&w.constraint);
                    progress |= c != w.constraint;
                    w.constraint = c;
                    let site = this.call_site(&w.constraint, &givens, w.span);
                    site.map_or_else(
                        || this.entail(&w.constraint, &givens, &mut Vec::new()),
                        Outcome::Solved,
                    )
                });
                match outcome {
                    Outcome::Solved(dict) => {
                        progress = true;
                        self.record_dict(w.span, w.order, dict.clone());
                        self.classes.evidence.push((w.span, dict));
                    }
                    outcome => left.push((w, outcome)),
                }
            }
            progress |= left
                .iter()
                .any(|(w, _)| self.canon(&w.constraint) != w.constraint);
            if !progress && report {
                let open: Vec<Type> = left.iter().map(|(w, _)| w.constraint.clone()).collect();
                if self.default_numeric(&open) {
                    pending = left.into_iter().map(|(w, _)| w).collect();
                    continue;
                }
            }
            if !progress || left.is_empty() {
                if report {
                    self.report_classes(left);
                    // the operators and literals the classes held back
                    self.settle_operators();
                } else {
                    self.classes.wanted.extend(left.into_iter().map(|(w, _)| w));
                }
                return;
            }
            pending = left.into_iter().map(|(w, _)| w).collect();
        }
    }

    /// Gives each integer literal's type that `wanted` leaves open the first
    /// numeric type every one of them has an instance for, else `Int`;
    /// whether there was one.
    fn default_numeric(&mut self, wanted: &[Type]) -> bool {
        let wanted: Vec<Type> = wanted.iter().map(|c| self.zonk(c)).collect();
        let mut open = Vec::new();
        for c in &wanted {
            c.visit(&mut |t| {
                if let Type::Meta(m) = t
                    && self.store.is_numeric(*m)
                    && !open.contains(m)
                {
                    open.push(*m);
                }
            });
        }
        for &m in &open {
            let fractional = self.store.is_fractional(m);
            let fits = |num: &Num| {
                if fractional && !num.fractional() {
                    return false;
                }
                let ty = Type::lang(num.lang());
                wanted
                    .iter()
                    .filter(|c| c.contains(&mut |t| *t == Type::Meta(m)))
                    .all(|c| self.may_have_instance(&replace_metas(c, &[(m, ty.clone())])))
            };
            let fallback = default_num(fractional);
            let num = Num::ALL.iter().find(|n| fits(n)).unwrap_or(&fallback);
            let _ = self.unify(&Type::Meta(m), &Type::lang(num.lang()));
        }
        !open.is_empty()
    }

    /// Whether an instance head matches `c`, or its class has no declared
    /// instances to rule it out.
    fn may_have_instance(&self, c: &Type) -> bool {
        let Some((class, args)) = split(c) else {
            return true;
        };
        let args: Vec<Type> = args.into_iter().cloned().collect();
        let mut instances = self
            .env
            .instances()
            .filter(|i| split(&i.head).is_some_and(|(k, _)| k == class))
            .peekable();
        instances.peek().is_none()
            || instances.any(|i| head_match(&i.head, &args, &mut Subst::new()) != Match::No)
    }

    /// Whether `c` follows from `givens` and the instances in scope.
    pub fn entails(&mut self, c: &Type, givens: &[Type]) -> Outcome {
        let givens = self.closure(givens);
        let c = self.canon(c);
        self.entail(&c, &givens, &mut Vec::new())
    }

    /// The assumptions with their superclasses, transitively.
    pub fn closure(&self, givens: &[Type]) -> Vec<(Type, Dict)> {
        fn visit(
            checker: &Checker<'_>,
            g: Type,
            dict: Dict,
            path: &mut Vec<(String, String)>,
            out: &mut Vec<(Type, Dict)>,
        ) {
            if out.iter().any(|(x, _)| *x == g) {
                return;
            }
            let Some(((module, name), _)) = split(&g) else {
                out.push((g, dict));
                return;
            };
            if path.iter().any(|(m, n)| m == module && n == name) {
                return;
            }
            path.push((module.to_owned(), name.to_owned()));
            let supers = checker.supers_of(&g);
            let index = out.len();
            out.push((g, dict));
            for (i, s) in supers.iter().enumerate().rev() {
                let class = super_slot(&supers, i);
                let s = checker.canon(s);
                let dict = Dict::Super {
                    dict: Box::new(out[index].1.clone()),
                    class,
                };
                visit(checker, s, dict, path, out);
            }
            path.pop();
        }

        let mut out = Vec::new();
        let mut path = Vec::new();
        for g in givens.iter().rev() {
            if let Some((local, c)) = as_field_given(g) {
                let c = self.canon(c);
                visit(self, c, Dict::Field(local.to_owned()), &mut path, &mut out);
                continue;
            }
            let g = self.canon(g);
            let shown = self.show(&g);
            visit(self, g, Dict::Given(shown), &mut path, &mut out);
        }
        out
    }

    /// A part of a canonical constraint, canonical again once unknowns in
    /// it may have been solved meanwhile.
    fn settled(&self, c: Type) -> Type {
        if has_metas(&c) { self.canon(&c) } else { c }
    }

    /// Solves a canonical constraint.
    pub(super) fn entail(&mut self, c: &Type, givens: &Givens, chain: &mut Vec<Type>) -> Outcome {
        let limits = self.options.limits;
        self.classes.steps = self.classes.steps.saturating_add(1);
        chain.push(c.clone());
        let outcome = if self.classes.steps > limits.steps || chain.len() > limits.depth as usize {
            Outcome::Budget(chain.clone())
        } else {
            self.entail_one(c, givens, chain)
        };
        chain.pop();
        outcome
    }

    fn entail_one(&mut self, c: &Type, givens: &Givens, chain: &mut Vec<Type>) -> Outcome {
        let Some(((module, name), args)) = split(c) else {
            return Outcome::Solved(Dict::Error);
        };
        let (module, name) = (module.to_owned(), name.to_owned());
        let args: Vec<Type> = args.into_iter().cloned().collect();
        if c.contains(&mut |t| *t == Type::Error) {
            return Outcome::Solved(Dict::Error);
        }
        let ground =
            !c.contains(&mut |t| matches!(t, Type::Meta(_) | Type::Skolem(_) | Type::Var(_)));
        let key = ground.then(|| c.clone());
        if let Some(dict) = key.as_ref().and_then(|k| self.classes.cache.get(k)) {
            return Outcome::Solved(dict.clone());
        }
        if let Some((_, dict)) = givens.iter().find(|(g, _)| g == c) {
            return Outcome::Solved(dict.clone());
        }
        if self.improve_by_givens(&module, &name, &args, givens) {
            let improved = self.canon(c);
            // an assumption with an error in it unifies yet fixes nothing:
            // that error is reported already
            if improved == *c {
                return Outcome::Solved(Dict::Error);
            }
            return self.entail(&improved, givens, chain);
        }
        let outcome = if let Some(class) = which(&RECORD_CLASSES, &module, &name) {
            self.primitive(c, class, &args, givens, chain)
        } else if Self::structural(&module, &name)
            && matches!(args.first(), Some(Type::Record(_) | Type::Tuple(_)))
        {
            self.structural_instance(c, givens, chain)
        } else {
            self.by_instance(c, (&module, &name), &args, givens, chain)
        };
        if let (Some(key), Outcome::Solved(dict)) = (key, &outcome)
            && !dict.assumes()
        {
            self.classes.cache.insert(key, dict.clone());
        }
        outcome
    }

    /// An assumption of the same class that agrees on the determining
    /// arguments of a functional dependency fixes the determined ones.
    fn improve_by_givens(
        &mut self,
        module: &str,
        name: &str,
        args: &[Type],
        givens: &Givens,
    ) -> bool {
        let Some(info) = self.class_info(module, name) else {
            return false;
        };
        for (g, _) in givens {
            let Some((class, gargs)) = split(g) else {
                continue;
            };
            if class != (module, name) {
                continue;
            }
            for fd in &info.fundeps {
                let agree = fd
                    .from
                    .iter()
                    .all(|&i| gargs.get(i).copied() == args.get(i));
                let differ = fd.to.iter().any(|&j| gargs.get(j).copied() != args.get(j));
                if agree
                    && differ
                    && fd.to.iter().all(|&j| match (args.get(j), gargs.get(j)) {
                        (Some(a), Some(b)) => self.unify(a, b).is_ok(),
                        _ => false,
                    })
                {
                    return true;
                }
            }
        }
        false
    }

    fn by_instance(
        &mut self,
        c: &Type,
        class: (&str, &str),
        args: &[Type],
        givens: &Givens,
        chain: &mut Vec<Type>,
    ) -> Outcome {
        let instances: Vec<Instance> = self
            .env
            .instances()
            .filter(|i| split(&i.head).is_some_and(|(k, _)| k == class))
            .cloned()
            .collect();
        let mut unknown = false;
        for instance in &instances {
            let mut subst = Subst::new();
            match head_match(&instance.head, args, &mut subst) {
                Match::Yes => return self.by_context(instance, &subst, givens, chain),
                Match::Unknown => unknown = true,
                Match::No => {}
            }
        }
        if self.improve_by_instances(&instances, class, args) {
            let c = self.canon(c);
            return self.entail(&c, givens, chain);
        }
        // an assumption may still apply once the unknown types are known
        unknown |= givens
            .iter()
            .any(|(g, _)| head_match(g, args, &mut Subst::new()) == Match::Unknown);
        if unknown {
            Outcome::Stuck
        } else {
            Outcome::Missing(c.clone(), None)
        }
    }

    fn by_context(
        &mut self,
        instance: &Instance,
        subst: &Subst,
        givens: &Givens,
        chain: &mut Vec<Type>,
    ) -> Outcome {
        let mut contexts: Vec<(Type, Option<Dict>)> = instance
            .context
            .iter()
            .map(|context| (context.substitute(subst), None))
            .collect();
        loop {
            let mut progress = false;
            let mut stuck = false;
            for (context, dict) in &mut contexts {
                if dict.is_some() {
                    continue;
                }
                let c = self.canon(context);
                progress |= c != *context;
                *context = c;
                match self.entail(context, givens, chain) {
                    Outcome::Solved(solved) => {
                        *dict = Some(solved);
                        progress = true;
                    }
                    Outcome::Stuck => stuck = true,
                    other => return other,
                }
            }
            if !stuck {
                return Outcome::Solved(Dict::Instance {
                    module: instance.module.clone(),
                    name: instance.name.clone(),
                    args: contexts.into_iter().filter_map(|(_, dict)| dict).collect(),
                });
            }
            progress |= contexts
                .iter()
                .any(|(context, dict)| dict.is_none() && self.canon(context) != *context);
            if !progress {
                return Outcome::Stuck;
            }
        }
    }

    /// An instance whose head matches the determining arguments of a
    /// functional dependency fixes the determined ones.
    fn improve_by_instances(
        &mut self,
        instances: &[Instance],
        class: (&str, &str),
        args: &[Type],
    ) -> bool {
        let Some(info) = self.class_info(class.0, class.1) else {
            return false;
        };
        for instance in instances {
            let Some((_, pattern)) = split(&instance.head) else {
                continue;
            };
            for fd in &info.fundeps {
                let mut subst = Subst::new();
                let determined = fd
                    .from
                    .iter()
                    .all(|&i| match (pattern.get(i), args.get(i)) {
                        (Some(p), Some(t)) => match_type(p, t, &mut subst) == Match::Yes,
                        _ => false,
                    });
                if !determined {
                    continue;
                }
                if fd.to.iter().any(|&j| match (pattern.get(j), args.get(j)) {
                    (Some(p), Some(t)) => match_type(p, t, &mut subst) == Match::No,
                    _ => true,
                }) {
                    continue;
                }
                let mut changed = false;
                for &j in &fd.to {
                    let (Some(p), Some(t)) = (pattern.get(j), args.get(j)) else {
                        continue;
                    };
                    for v in vars(p) {
                        if !subst.iter().any(|(n, _)| *n == v) {
                            let fresh = self.fresh();
                            subst.push((v, fresh));
                        }
                    }
                    let target = p.substitute(&subst);
                    if self.unify(t, &target).is_ok() && self.canon(t) != *t {
                        changed = true;
                    }
                }
                if changed {
                    return true;
                }
            }
        }
        false
    }

    /// `C { a : A; b : B }` from `C A` and `C B`, `C (A, B)` likewise.
    fn structural_instance(&mut self, c: &Type, givens: &Givens, chain: &mut Vec<Type>) -> Outcome {
        let (head, args) = c.spine();
        let class = head.clone();
        let items: Vec<Type> = match args.first() {
            Some(Type::Tuple(items)) => items.clone(),
            Some(Type::Record(row)) => match row.tail.as_deref() {
                None => row.entries.iter().map(|(_, t)| t.clone()).collect(),
                Some(Type::Meta(_)) => return Outcome::Stuck,
                Some(_) => {
                    return Outcome::Missing(
                        c.clone(),
                        Some("the record is open, so its other fields are not known; require the constraint for the whole record".to_owned()),
                    );
                }
            },
            _ => return Outcome::Missing(c.clone(), None),
        };
        let mut fields = Vec::new();
        let mut stuck = false;
        for item in items {
            let c = self.settled(Type::app(class.clone(), item));
            match self.entail(&c, givens, chain) {
                Outcome::Solved(dict) => fields.push(dict),
                Outcome::Stuck => stuck = true,
                other => return other,
            }
        }
        if stuck {
            return Outcome::Stuck;
        }
        let class = split(c).map(|((_, n), _)| n.to_owned()).unwrap_or_default();
        Outcome::Solved(Dict::Structural { class, fields })
    }
}

/// Matches the arguments of an instance head against those of a constraint.
pub fn head_match(head: &Type, args: &[Type], subst: &mut Subst) -> Match {
    let Some((_, pattern)) = split(head) else {
        return Match::No;
    };
    if pattern.len() != args.len() {
        return Match::No;
    }
    pattern
        .iter()
        .zip(args)
        .fold(Match::Yes, |m, (p, t)| m.and(|| match_type(p, t, subst)))
}
