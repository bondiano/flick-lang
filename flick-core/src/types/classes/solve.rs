//! The instance solver. A constraint is solved by an assumption in scope or
//! one of its superclasses, by a row primitive, by the compiler-owned
//! instance of a record or tuple, or by the one instance whose head matches
//! it; functional dependencies improve unknown types on the way. Every step
//! counts against the budget of the definition being checked.

use super::assumes;
use crate::diagnostics::codes;
use crate::numeric::NumLang;
use std::mem;

use super::matching::{Match, Subst};
use super::{Dict, RECORD_CLASSES, has_metas, split, super_slot, which};
use crate::numeric::{Num, default_num};
use crate::types::Checker;
use crate::types::env::Instance;
use crate::types::gadt::branch::as_field_given;
use crate::types::generalize::replace_metas;
use crate::types::lang::{self, Lang};
use crate::types::passes_dictionary;
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
        if report {
            // Grow any active subrows and holds before closing unconstrained
            // capture-row metas in wanted constraints (e.g. on a capture-free
            // constructor payload), so a live capture-row meta can never be
            // prematurely defaulted to `{}`.
            self.grow();
            let mut active_hold_metas = Vec::new();
            for hold in &self.cx.regions.holds {
                for tail in self.store.normalize(&hold.bound).tails {
                    if let Some(m) = tail.inference_meta_root()
                        && !active_hold_metas.contains(&m)
                    {
                        active_hold_metas.push(m);
                    }
                }
            }
            let mut open_caps = Vec::new();
            for w in &pending {
                self.zonk(&w.constraint).visit(&mut |t| {
                    if let Type::Captured { captures, .. } = t {
                        for tail in self.store.normalize(captures).tails {
                            if let Some(m) = tail.inference_meta_root()
                                && !active_hold_metas.contains(&m)
                                && !open_caps.contains(&m)
                            {
                                open_caps.push(m);
                            }
                        }
                    } else if let Some(m) = t.inference_meta_root()
                        && self.store.is_capture_row(m)
                        && !active_hold_metas.contains(&m)
                        && !open_caps.contains(&m)
                    {
                        open_caps.push(m);
                    }
                });
            }
            for m in open_caps {
                self.store.set(m, Type::empty_row());
            }
        }
        loop {
            let mut left = Vec::new();
            let mut progress = false;
            for mut w in pending {
                // under the equalities of the GADT branches it is from
                let outcome = self.under(&w.givens, |this| {
                    let previous = mem::replace(&mut this.classes.local, w.local.clone());
                    let givens = this.closure(&w.givens);
                    let c = this.canon(&w.constraint);
                    progress |= c != w.constraint;
                    w.constraint = c;
                    let site = this.call_site(&w.constraint, &givens, w.span);
                    let outcome = site.map_or_else(
                        || this.entail(&w.constraint, &givens, &mut Vec::new()),
                        Outcome::Solved,
                    );
                    this.classes.local = previous;
                    outcome
                });
                match outcome {
                    Outcome::Solved(dict) => {
                        progress = true;
                        if w.await_operand.is_some()
                            && let Some(args) = super::args_of(&w.constraint, Lang::Awaitable)
                            && let [_, _, _, result] = args[..]
                        {
                            self.record_await_worker(
                                super::super::typing::range(w.span),
                                &dict,
                                result,
                            );
                        }
                        // `Detached` and `NoControl` are the compiler's to decide: no dictionary at run time
                        if passes_dictionary(&w.constraint) {
                            self.record_dict(w.span, w.order, dict.clone());
                        }
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
                if let Some(m) = t.inference_meta_root()
                    && self.store.is_numeric(m)
                    && !open.iter().any(|(root, _)| *root == m)
                {
                    open.push((m, t.clone()));
                }
            });
        }
        let mut progress = false;
        for (m, variable) in &open {
            let m = *m;
            let fractional = self.store.is_fractional(m);
            let fallback = default_num(fractional);
            let mut chosen = fallback;
            for num in Num::ALL {
                if fractional && !num.fractional() {
                    continue;
                }
                let ty = Type::lang(num.lang());
                let mut fits = true;
                for c in wanted
                    .iter()
                    .filter(|c| c.contains(&mut |t| t.inference_meta_root() == Some(m)))
                {
                    if !self.may_have_instance(&replace_metas(c, &[(m, ty.clone())])) {
                        fits = false;
                        break;
                    }
                }
                if fits {
                    chosen = num;
                    break;
                }
            }
            progress |= self.unify(variable, &Type::lang(chosen.lang())).is_ok();
        }
        progress
    }

    /// Whether an instance head matches `c`, or its class has no declared
    /// instances to rule it out.
    fn may_have_instance(&mut self, c: &Type) -> bool {
        let Some(((cm, cn), args)) = split(c) else {
            return true;
        };
        let class = (cm.to_owned(), cn.to_owned());
        let args: Vec<Type> = args.into_iter().cloned().collect();
        let instances: Vec<Instance> = self
            .env
            .instances()
            .filter(|i| split(&i.head).is_some_and(|((m, n), _)| m == class.0 && n == class.1))
            .cloned()
            .collect();
        let signs = self.class_variances(&class.0, &class.1, args.len());
        instances.is_empty()
            || instances.iter().any(|i| {
                self.match_head_variance_with_signs(
                    &signs,
                    &i.vars,
                    &i.head,
                    &args,
                    &mut Subst::new(),
                ) != Match::No
            })
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
        for (constraint, dict) in self.classes.local.iter().rev() {
            visit(
                self,
                self.canon(constraint),
                dict.clone(),
                &mut path,
                &mut out,
            );
        }
        out
    }

    /// A part of a canonical constraint, canonical again once unknowns in
    /// it may have been solved meanwhile.
    pub(super) fn settled(&self, c: Type) -> Type {
        if has_metas(&c) { self.canon(&c) } else { c }
    }

    /// Solves a canonical constraint.
    pub(in crate::types) fn entail(
        &mut self,
        c: &Type,
        givens: &Givens,
        chain: &mut Vec<Type>,
    ) -> Outcome {
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
        if let Some((_, dict)) = givens
            .iter()
            .find(|(g, _)| g == c || self.given_serves(c, g) == Match::Yes)
        {
            return Outcome::Solved(dict.clone());
        }
        if let Some(dict) = key.as_ref().and_then(|k| self.classes.cache.get(k)) {
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
        } else if lang::is(Lang::Detached, &module, &name) {
            self.detached(c, &args, givens, Lang::Detached)
        } else if lang::is(Lang::NoControl, &module, &name) {
            self.detached(c, &args, givens, Lang::NoControl)
        } else if Self::structural(&module, &name)
            && matches!(args.first(), Some(Type::Record(_) | Type::Tuple(_)))
        {
            self.structural_instance(c, givens, chain)
        } else {
            self.by_instance(c, (&module, &name), &args, givens, chain)
        };
        if let (Some(key), Outcome::Solved(dict)) = (key, &outcome)
            && !assumes(dict)
        {
            self.classes.cache.insert(key, dict.clone());
        }
        outcome
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
        let signs = self.class_variances(class.0, class.1, args.len());
        let mut unknown = false;
        let mut matched: Vec<(&Instance, Subst)> = Vec::new();
        for instance in &instances {
            let mut subst = Subst::new();
            match self.match_head_variance_with_signs(
                &signs,
                &instance.vars,
                &instance.head,
                args,
                &mut subst,
            ) {
                Match::Yes => matched.push((instance, subst)),
                Match::Unknown => unknown = true,
                Match::No => {}
            }
        }
        // Only commit when a unique instance matches and no other instance is
        // still `Unknown`, so refining a metavariable can never silently switch
        // the chosen dictionary.
        if matched.len() == 1 && !unknown {
            let (instance, subst) = &matched[0];
            return self.by_context(instance, subst, givens, chain);
        }
        if matched.len() > 1 {
            // Two sibling imported instances can both match only after
            // `imported_conflicts` has already reported `FLICK-C003`; avoid
            // cascading a misleading `FLICK-C002` at every call site.
            let already_overlapping = matched.iter().any(|(instance, _)| {
                self.diagnostics
                    .iter()
                    .any(|d| d.code == codes::C003 && d.message.contains(&instance.name))
            });
            if already_overlapping {
                return Outcome::Solved(Dict::Error);
            }
            unknown = true;
        }
        if self.improve_by_instances(&instances, class, args) {
            let c = self.canon(c);
            return self.entail(&c, givens, chain);
        }
        // an assumption may still apply once the unknown types are known
        unknown |= givens
            .iter()
            .any(|(g, _)| self.given_serves(c, g) == Match::Unknown);
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
}
