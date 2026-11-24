//! Bounded editor suggestions, replayed by the certificate checker at the
//! active obligation. The probes have their own budget and retain only text.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::super::show::source_name;
use super::super::{SEARCH_CANDIDATES, SEARCH_NORMALIZATION, SEARCH_PROBE_STEPS};
use super::{
    Arg, Cert, Checker, Equation, Goal, Lemma, LemmaAliases, Probe, Program, ProofSearchResult,
    ProofSearchStatus, Rule, Span, Statement, Term, Ty, probe,
};

#[derive(Clone, Copy)]
struct Limits {
    candidates: usize,
    normalization: u64,
    per_goal: usize,
    lemmas: usize,
    instantiations: usize,
    arguments: usize,
}

const COMPLETION: Limits = Limits {
    candidates: 128,
    normalization: 12_000,
    per_goal: 20,
    lemmas: 64,
    instantiations: 8,
    arguments: 8,
};

const EXPLICIT: Limits = Limits {
    candidates: SEARCH_CANDIDATES,
    normalization: SEARCH_NORMALIZATION,
    per_goal: SEARCH_CANDIDATES,
    lemmas: 160,
    instantiations: 32,
    arguments: 12,
};

const MIDDLES: usize = 16;
const FACTS: usize = 64;
const MAX_CHECKED: usize = 8;

pub(super) struct Candidates<'a> {
    program: &'a dyn Program,
    aliases: &'a LemmaAliases,
    attempts: usize,
    normalization: u64,
    limits: Limits,
    cancelled: Option<&'a AtomicBool>,
    limited: bool,
}

impl<'a> Candidates<'a> {
    pub fn new(program: &'a dyn Program, aliases: &'a LemmaAliases) -> Self {
        Self::bounded(program, aliases, COMPLETION, None)
    }

    pub fn explicit(
        program: &'a dyn Program,
        aliases: &'a LemmaAliases,
        cancelled: &'a AtomicBool,
    ) -> Self {
        Self::bounded(program, aliases, EXPLICIT, Some(cancelled))
    }

    fn bounded(
        program: &'a dyn Program,
        aliases: &'a LemmaAliases,
        limits: Limits,
        cancelled: Option<&'a AtomicBool>,
    ) -> Self {
        Self {
            program,
            aliases,
            attempts: limits.candidates,
            normalization: limits.normalization,
            limits,
            cancelled,
            limited: false,
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    pub fn search(
        &mut self,
        checker: &Checker<'_>,
        goal: Result<&Equation, (&Ty, &Ty)>,
        span: Option<Span>,
        declaration: Span,
        synthesizing: bool,
    ) -> ProofSearchResult {
        let suggestions = self.at(checker, goal, span.unwrap_or(declaration), synthesizing);
        let status = if self.is_cancelled() {
            ProofSearchStatus::Cancelled
        } else if !suggestions.is_empty() {
            ProofSearchStatus::Found
        } else if self.attempts == 0 || self.normalization == 0 || self.limited {
            ProofSearchStatus::BudgetExhausted
        } else {
            ProofSearchStatus::NoCandidate
        };
        ProofSearchResult {
            status,
            suggestions: if status == ProofSearchStatus::Cancelled {
                Vec::new()
            } else {
                suggestions
            },
            candidates: self.limits.candidates - self.attempts,
            normalization_steps: self.limits.normalization - self.normalization,
            span,
        }
    }

    pub fn at(
        &mut self,
        checker: &Checker<'_>,
        goal: Result<&Equation, (&Ty, &Ty)>,
        span: Span,
        synthesizing: bool,
    ) -> Vec<String> {
        if self.attempts == 0 || self.normalization == 0 || self.is_cancelled() {
            return Vec::new();
        }
        let locals = locals(checker);
        let statement = Statement {
            name: String::new(),
            span,
            params: checker.vars.clone(),
            premises: checker.facts.clone(),
            goal: match goal {
                Ok(eq) => Goal::Equal(eq.clone()),
                Err((a, b)) => Goal::SameType(a.clone(), b.clone()),
            },
        };
        let mut testing = Testing {
            budget: self,
            checker,
            statement: &statement,
            attempted: 0,
            found: Vec::new(),
        };
        let refl = !locals.contains_key("refl");
        let symm = !locals.contains_key("symm");
        if refl && !synthesizing {
            testing.test(Rule::Refl, "refl");
        }
        let Ok(goal) = goal else {
            return testing.found;
        };
        for (name, local) in &locals {
            if !testing.available() {
                break;
            }
            if local.equation.is_some() && local.ty == &goal.ty {
                testing.test(Rule::Fact(local.unique.clone()), name);
                if symm {
                    testing.test(
                        Rule::Symm(Box::new(Cert {
                            kind: Rule::Fact(local.unique.clone()),
                            span,
                        })),
                        &format!("symm {name}"),
                    );
                }
            }
        }
        let explicit = testing.budget.cancelled.is_some();
        if explicit && !locals.contains_key("trans") {
            testing.transitivity(goal, &locals, symm);
        }
        testing.lemmas(goal, &locals, refl, symm, false);
        if explicit && symm {
            testing.lemmas(goal, &locals, refl, symm, true);
        }
        testing.found
    }
}

struct Testing<'a, 'p, 'v> {
    budget: &'a mut Candidates<'p>,
    checker: &'a Checker<'v>,
    statement: &'a Statement,
    attempted: usize,
    found: Vec<String>,
}

impl Testing<'_, '_, '_> {
    fn available(&self) -> bool {
        self.budget.attempts > 0
            && self.budget.normalization > 0
            && self.attempted < self.budget.limits.per_goal
            && self.found.len() < MAX_CHECKED
            && !self.budget.is_cancelled()
    }

    fn test(&mut self, rule: Rule, text: &str) -> bool {
        if !self.available() {
            return false;
        }
        self.attempted += 1;
        self.budget.attempts -= 1;
        let proof = Cert {
            kind: rule,
            span: self.statement.span,
        };
        let before = if self.budget.cancelled.is_some() {
            self.budget.normalization.min(SEARCH_PROBE_STEPS)
        } else {
            self.budget.normalization
        };
        let mut remaining = before;
        let checked = probe(
            self.statement,
            &proof,
            self.checker.data,
            self.budget.program,
            self.checker.lemmas,
            &self.checker.refined,
            &mut remaining,
        );
        self.budget.normalization -= before - remaining;
        self.budget.limited |= checked == Probe::Exhausted;
        let accepted = checked == Probe::Accepted;
        if accepted && !self.found.iter().any(|suggestion| suggestion == text) {
            self.found.push(text.to_owned());
        }
        accepted
    }

    fn lemmas(
        &mut self,
        goal: &Equation,
        locals: &Locals<'_>,
        refl: bool,
        symm: bool,
        reverse: bool,
    ) {
        let wrap = |name: &str, args: &str| match (reverse, args.is_empty()) {
            (true, true) => format!("symm {name}"),
            (true, false) => format!("symm ({name}{args})"),
            (false, _) => format!("{name}{args}"),
        };
        let span = self.statement.span;
        for lemma in self
            .checker
            .lemmas
            .iter()
            .rev()
            .take(self.budget.limits.lemmas)
        {
            if !self.available() {
                break;
            }
            let name = &lemma.statement.name;
            let aliases = self.budget.aliases.get(name);
            let spelling = match aliases {
                Some(names) => names.first().map(String::as_str),
                None if !name.contains('.') && !locals.contains_key(name.as_str()) => {
                    (!is_rule(name)).then_some(name.as_str())
                }
                None => None,
            };
            let Some(spelling) = spelling else { continue };
            for application in
                applications(lemma, goal, locals, refl, symm, span, self.budget.limits)
            {
                let text = wrap(spelling, &application.source);
                let direct = Rule::Lemma(name.as_str().into(), application.args);
                let rule = if reverse {
                    Rule::Symm(Box::new(Cert { kind: direct, span }))
                } else {
                    direct
                };
                if self.test(rule, &text)
                    && let Some(aliases) = self.budget.aliases.get(name)
                {
                    for alias in aliases.iter().skip(1) {
                        if self.found.len() == MAX_CHECKED {
                            break;
                        }
                        self.found.push(wrap(alias, &application.source));
                    }
                }
            }
        }
    }

    fn transitivity(&mut self, goal: &Equation, locals: &Locals<'_>, symm: bool) {
        let span = self.statement.span;
        let middles = locals
            .iter()
            .filter(|(_, local)| local.equation.is_none() && local.ty == &goal.ty)
            .take(MIDDLES);
        for (name, local) in middles {
            if !self.available() {
                break;
            }
            let middle = Term::Var(local.unique.clone());
            if let Some((left, p)) = witness(locals, goal, &goal.left, &middle, span, symm)
                && let Some((right, q)) = witness(locals, goal, &middle, &goal.right, span, symm)
            {
                self.test(
                    Rule::Trans(middle, Box::new(left), Box::new(right)),
                    &format!("trans {name} {p} {q}"),
                );
            }
        }
    }
}

fn witness(
    locals: &Locals<'_>,
    goal: &Equation,
    left: &Term,
    right: &Term,
    span: Span,
    symm: bool,
) -> Option<(Cert, String)> {
    locals
        .iter()
        .filter_map(|(&name, local)| Some((name, local.unique, local.equation?)))
        .take(FACTS)
        .find_map(|(name, unique, equation)| {
            if equation.ty != goal.ty {
                return None;
            }
            let fact = Cert {
                kind: Rule::Fact(unique.clone()),
                span,
            };
            if &equation.left == left && &equation.right == right {
                Some((fact, name.to_owned()))
            } else if symm && &equation.left == right && &equation.right == left {
                Some((
                    Cert {
                        kind: Rule::Symm(Box::new(fact)),
                        span,
                    },
                    format!("(symm {name})"),
                ))
            } else {
                None
            }
        })
}

fn is_rule(name: &str) -> bool {
    matches!(name, "refl" | "symm" | "trans" | "cong" | "funext")
}

#[derive(Clone, Copy)]
struct Local<'a> {
    unique: &'a Rc<str>,
    ty: &'a Ty,
    equation: Option<&'a Equation>,
}

type Locals<'a> = BTreeMap<&'a str, Local<'a>>;

fn locals<'a>(checker: &'a Checker<'_>) -> Locals<'a> {
    let mut locals = BTreeMap::new();
    let variables = checker.vars.iter().map(|(unique, ty)| Local {
        unique,
        ty,
        equation: None,
    });
    let facts = checker.facts.iter().map(|(unique, equation)| Local {
        unique,
        ty: &equation.ty,
        equation: Some(equation),
    });
    for local in variables
        .chain(facts)
        .filter(|l| !l.unique.starts_with('$'))
    {
        let name = source_name(local.unique);
        let scope_rank = |unique: &str| {
            unique
                .rsplit_once('$')
                .and_then(|(_, n)| n.parse::<u32>().ok())
                .unwrap_or_default()
        };
        let innermost = locals.entry(name).or_insert(local);
        if scope_rank(innermost.unique) < scope_rank(local.unique) {
            *innermost = local;
        }
    }
    locals
}

#[derive(Clone, Default)]
struct Application {
    args: Vec<Arg>,
    source: String,
    types: BTreeMap<String, Ty>,
}

impl Application {
    fn with(&self, source: &str, arg: Arg, types: BTreeMap<String, Ty>) -> Self {
        let mut args = self.args.clone();
        args.push(arg);
        Self {
            args,
            source: format!("{} {source}", self.source),
            types,
        }
    }
}

fn applications(
    lemma: &Lemma,
    goal: &Equation,
    locals: &Locals<'_>,
    refl: bool,
    symm: bool,
    span: Span,
    limits: Limits,
) -> Vec<Application> {
    if lemma.order.len() > limits.arguments {
        return Vec::new();
    }
    let Goal::Equal(conclusion) = &lemma.statement.goal else {
        return Vec::new();
    };
    let mut first = Application::default();
    if !matches_type(&conclusion.ty, &goal.ty, &mut first.types) {
        return Vec::new();
    }
    let mut variants = vec![first];
    for (formal, premise) in &lemma.order {
        let ty = if *premise {
            lemma
                .statement
                .premises
                .iter()
                .find(|(n, _)| n == formal)
                .map(|(_, eq)| &eq.ty)
        } else {
            lemma
                .statement
                .params
                .iter()
                .find(|(n, _)| n == formal)
                .map(|(_, ty)| ty)
        };
        let Some(ty) = ty else { return Vec::new() };
        let mut options: Vec<_> = locals
            .iter()
            .map(|(&name, &local)| (name, local))
            .filter(|(_, l)| l.equation.is_some() == *premise)
            .collect();
        options.sort_by_key(|(name, _)| *name != source_name(formal));
        let mut next = Vec::new();
        for app in &variants {
            for &(name, local) in &options {
                let mut types = app.types.clone();
                if !matches_type(ty, local.ty, &mut types) {
                    continue;
                }
                if *premise {
                    let fact = Cert {
                        kind: Rule::Fact(local.unique.clone()),
                        span,
                    };
                    next.push(app.with(name, Arg::Proof(fact.clone()), types.clone()));
                    if symm {
                        let sym = Cert {
                            kind: Rule::Symm(Box::new(fact)),
                            span,
                        };
                        next.push(app.with(&format!("(symm {name})"), Arg::Proof(sym), types));
                    }
                } else {
                    next.push(app.with(name, Arg::Value(Term::Var(local.unique.clone())), types));
                }
                if next.len() >= limits.instantiations {
                    break;
                }
            }
            if *premise && refl && next.len() < limits.instantiations {
                next.push(app.with(
                    "refl",
                    Arg::Proof(Cert {
                        kind: Rule::Refl,
                        span,
                    }),
                    app.types.clone(),
                ));
            }
            if next.len() >= limits.instantiations {
                break;
            }
        }
        next.truncate(limits.instantiations);
        if next.is_empty() {
            return next;
        }
        variants = next;
    }
    variants
}

fn matches_type(formal: &Ty, actual: &Ty, types: &mut BTreeMap<String, Ty>) -> bool {
    match (formal, actual) {
        (Ty::Refused(_), _) | (_, Ty::Refused(_)) => false,
        (Ty::Rigid(name), actual) => {
            let known = types.entry(name.clone()).or_insert_with(|| actual.clone());
            known == actual
        }
        (Ty::Fun(a, b), Ty::Fun(c, d)) => matches_type(a, c, types) && matches_type(b, d, types),
        (
            Ty::Data { module, name, args },
            Ty::Data {
                module: m,
                name: n,
                args: xs,
            },
        ) => {
            module == m
                && name == n
                && args.len() == xs.len()
                && args.iter().zip(xs).all(|(a, b)| matches_type(a, b, types))
        }
        _ => formal == actual,
    }
}
