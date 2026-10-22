//! The instance solver. A constraint is solved by an assumption in scope or
//! one of its superclasses, by a row primitive, by the compiler-owned
//! instance of a record or tuple, or by the one instance whose head matches
//! it; functional dependencies improve unknown types on the way. Every step
//! counts against the budget of the definition being checked.

use std::mem;

use super::matching::{Match, Subst, match_type};
use super::{Dict, RECORD, Wanted, has_metas, split, vars};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::source::Span;
use crate::types::Checker;
use crate::types::env::Instance;
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
            let mut solved = false;
            for w in pending {
                let givens = self.closure(&w.givens);
                let c = self.canon(&w.constraint);
                match self.entail(&c, &givens, &mut Vec::new()) {
                    Outcome::Solved(dict) => {
                        solved = true;
                        self.classes.evidence.push((w.span, dict));
                    }
                    outcome => left.push((w, outcome)),
                }
            }
            if !solved || left.is_empty() {
                if report {
                    self.report_classes(left);
                } else {
                    self.classes.wanted.extend(left.into_iter().map(|(w, _)| w));
                }
                return;
            }
            pending = left.into_iter().map(|(w, _)| w).collect();
        }
    }

    /// Whether `c` follows from `givens` and the instances in scope.
    pub fn entails(&mut self, c: &Type, givens: &[Type]) -> Outcome {
        let givens = self.closure(givens);
        let c = self.canon(c);
        self.entail(&c, &givens, &mut Vec::new())
    }

    /// The assumptions with their superclasses, transitively.
    pub fn closure(&self, givens: &[Type]) -> Vec<(Type, Dict)> {
        let mut out: Vec<(Type, Dict)> = Vec::new();
        let mut stack: Vec<(Type, Dict)> = givens
            .iter()
            .map(|g| {
                let g = self.canon(g);
                let shown = self.show(&g);
                (g, Dict::Given(shown))
            })
            .collect();
        while let Some((g, dict)) = stack.pop() {
            if out.iter().any(|(x, _)| *x == g) {
                continue;
            }
            for s in self.supers_of(&g) {
                let s = self.canon(&s);
                let class = split(&s)
                    .map(|((_, n), _)| n.to_owned())
                    .unwrap_or_default();
                let dict = Dict::Super {
                    dict: Box::new(dict.clone()),
                    class,
                };
                stack.push((s, dict));
            }
            out.push((g, dict));
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
            let c = self.canon(c);
            return self.entail_one(&c, givens, chain);
        }
        let outcome = if module == RECORD {
            self.primitive(c, &name, &args, givens, chain)
        } else if Self::structural(&module, &name)
            && matches!(args.first(), Some(Type::Record(_) | Type::Tuple(_)))
        {
            self.structural_instance(c, givens, chain)
        } else {
            self.by_instance(c, (&module, &name), &args, givens, chain)
        };
        if let (Some(key), Outcome::Solved(dict)) = (key, &outcome) {
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
            .instances
            .iter()
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
            return self.entail_one(&c, givens, chain);
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
        let mut args = Vec::new();
        let mut stuck = false;
        for context in &instance.context {
            let c = self.settled(context.substitute(subst));
            match self.entail(&c, givens, chain) {
                Outcome::Solved(dict) => args.push(dict),
                Outcome::Stuck => stuck = true,
                other => return other,
            }
        }
        if stuck {
            return Outcome::Stuck;
        }
        Outcome::Solved(Dict::Instance {
            module: instance.module.clone(),
            name: instance.name.clone(),
            args,
        })
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

    fn report_classes(&mut self, left: Vec<(Wanted, Outcome)>) {
        let mut budget = false;
        for (w, outcome) in left {
            if self.explained(w.span) {
                continue;
            }
            let constraint = self.canon(&w.constraint);
            match outcome {
                Outcome::Missing(part, reason) => {
                    self.missing(&constraint, &part, reason.as_deref(), w.span);
                }
                Outcome::Stuck => {
                    let shown = self.show(&constraint);
                    let candidates = self.candidates(&constraint);
                    self.diagnostics.push(
                        Diagnostic::error(
                            codes::C002,
                            format!("cannot choose an instance for `{shown}`: a type in it is not known here; {candidates}"),
                            w.span,
                        )
                        .with_fix("annotate the type of the value", Vec::new()),
                    );
                }
                Outcome::Budget(chain) if !budget => {
                    budget = true;
                    self.over_budget(&constraint, &chain, w.span);
                }
                _ => {}
            }
        }
    }

    /// An error inside or around `span` already explains a failure there.
    fn explained(&self, span: Span) -> bool {
        self.diagnostics.iter().any(|d| {
            d.severity == Severity::Error
                && d.span.is_some_and(|s| {
                    s.file == span.file && s.start <= span.end && span.start <= s.end
                })
        })
    }

    fn missing(&mut self, constraint: &Type, part: &Type, reason: Option<&str>, span: Span) {
        let (whole, shown) = (self.show(constraint), self.show(part));
        let rigid = part.contains(&mut |t| matches!(t, Type::Skolem(_) | Type::Var(_)));
        let mut message = match (&reason, rigid) {
            (Some(reason), _) => format!("`{shown}` does not hold: {reason}"),
            (None, true) => format!("cannot deduce `{shown}` from the constraints in scope"),
            (None, false) => format!("no instance for `{shown}`"),
        };
        if whole != shown {
            message = format!("{message}, needed for `{whole}`");
        }
        let mut diagnostic = if reason.is_none() && !rigid {
            let candidates = self.candidates(part);
            Diagnostic::error(codes::C001, format!("{message}; {candidates}"), span)
        } else {
            Diagnostic::error(codes::C001, message, span)
        };
        if rigid && reason.is_none() {
            diagnostic =
                diagnostic.with_fix(format!("add `{shown} =>` to the signature"), Vec::new());
        }
        self.diagnostics.push(diagnostic);
    }

    /// The instances of the class of `c`, for a diagnostic.
    fn candidates(&self, c: &Type) -> String {
        let Some((class, _)) = split(c) else {
            return String::new();
        };
        let names: Vec<String> = self
            .env
            .instances
            .iter()
            .filter(|i| split(&i.head).is_some_and(|(k, _)| k == class))
            .map(|i| format!("`{}`", i.name))
            .collect();
        match names.len() {
            0 => format!("`{}` has no instances", class.1),
            n if n <= 6 => format!("instances of `{}`: {}", class.1, names.join(", ")),
            n => format!(
                "instances of `{}`: {}, and {} more",
                class.1,
                names[..5].join(", "),
                n - 5
            ),
        }
    }

    fn over_budget(&mut self, constraint: &Type, chain: &[Type], span: Span) {
        let limits = self.options.limits;
        let what = if chain.len() > limits.depth as usize {
            format!("{} nested constraints", limits.depth)
        } else {
            format!("{} steps", limits.steps)
        };
        let short = |t: &Type| {
            let s = self.show(t);
            match s.char_indices().nth(60) {
                Some((i, _)) => format!("`{}…`", &s[..i]),
                None => format!("`{s}`"),
            }
        };
        let mut links: Vec<String> = chain.iter().take(3).map(short).collect();
        if chain.len() > 5 {
            links.push("…".to_owned());
        }
        links.extend(
            chain
                .iter()
                .skip(3.max(chain.len().saturating_sub(2)))
                .map(short),
        );
        let shown = short(constraint);
        self.diagnostics.push(
            Diagnostic::error(
                codes::C009,
                format!(
                    "resolving {shown} exceeds the budget of {what}: {}",
                    links.join(" -> ")
                ),
                span,
            )
            .with_fix(
                "simplify the types involved, or raise `solver-steps` or `solver-depth` in `[build]` of `flick.toml`",
                Vec::new(),
            ),
        );
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
