//! Bounded editor suggestions, replayed by the certificate checker at the
//! active obligation. The probes have their own budget and retain only text.

use std::collections::BTreeMap;
use std::rc::Rc;

use super::super::show::source_name;
use super::{
    Arg, Cert, Checker, Equation, Goal, Lemma, LemmaAliases, Program, Rule, Span, Statement, Term,
    Ty, probe,
};

const MAX_ATTEMPTS: usize = 128;
const MAX_NORMALIZATION: u64 = 12_000;
const ATTEMPTS_PER_GOAL: usize = 20;
const LEMMAS_PER_GOAL: usize = 64;
const INSTANTIATIONS_PER_LEMMA: usize = 8;
const MAX_ARGUMENTS: usize = 8;
const MAX_CHECKED: usize = 8;

pub(super) struct Candidates<'a> {
    program: &'a dyn Program,
    aliases: &'a LemmaAliases,
    attempts: usize,
    normalization: u64,
}

impl<'a> Candidates<'a> {
    pub fn new(program: &'a dyn Program, aliases: &'a LemmaAliases) -> Self {
        Self {
            program,
            aliases,
            attempts: MAX_ATTEMPTS,
            normalization: MAX_NORMALIZATION,
        }
    }

    pub fn at(
        &mut self,
        checker: &Checker<'_>,
        goal: Result<&Equation, (&Ty, &Ty)>,
        span: Span,
        synthesizing: bool,
    ) -> Vec<String> {
        if self.attempts == 0 || self.normalization == 0 {
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
            if local.fact && local.ty == &goal.ty {
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
        testing.lemmas(goal, &locals, refl, symm);
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
            && self.attempted < ATTEMPTS_PER_GOAL
            && self.found.len() < MAX_CHECKED
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
        let accepted = probe(
            self.statement,
            &proof,
            self.checker.data,
            self.budget.program,
            self.checker.lemmas,
            &self.checker.refined,
            &mut self.budget.normalization,
        );
        if accepted && !self.found.iter().any(|suggestion| suggestion == text) {
            self.found.push(text.to_owned());
        }
        accepted
    }

    fn lemmas(&mut self, goal: &Equation, locals: &Locals<'_>, refl: bool, symm: bool) {
        for lemma in self.checker.lemmas.iter().rev().take(LEMMAS_PER_GOAL) {
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
            for application in applications(lemma, goal, locals, refl, symm, self.statement.span) {
                let text = format!("{spelling}{}", application.source);
                if self.test(Rule::Lemma(name.as_str().into(), application.args), &text)
                    && let Some(aliases) = self.budget.aliases.get(name)
                {
                    for alias in aliases.iter().skip(1) {
                        if self.found.len() == MAX_CHECKED {
                            break;
                        }
                        self.found.push(format!("{alias}{}", application.source));
                    }
                }
            }
        }
    }
}

fn is_rule(name: &str) -> bool {
    matches!(name, "refl" | "symm" | "trans" | "cong" | "funext")
}

#[derive(Clone, Copy)]
struct Local<'a> {
    unique: &'a Rc<str>,
    ty: &'a Ty,
    fact: bool,
}

type Locals<'a> = BTreeMap<&'a str, Local<'a>>;

fn locals<'a>(checker: &'a Checker<'_>) -> Locals<'a> {
    let mut locals = BTreeMap::new();
    let variables = checker.vars.iter().map(|(unique, ty)| Local {
        unique,
        ty,
        fact: false,
    });
    let facts = checker.facts.iter().map(|(unique, equation)| Local {
        unique,
        ty: &equation.ty,
        fact: true,
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
) -> Vec<Application> {
    if lemma.order.len() > MAX_ARGUMENTS {
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
            .filter(|(_, l)| l.fact == *premise)
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
                if next.len() >= INSTANTIATIONS_PER_LEMMA {
                    break;
                }
            }
            if *premise && refl && next.len() < INSTANTIATIONS_PER_LEMMA {
                next.push(app.with(
                    "refl",
                    Arg::Proof(Cert {
                        kind: Rule::Refl,
                        span,
                    }),
                    app.types.clone(),
                ));
            }
            if next.len() >= INSTANTIATIONS_PER_LEMMA {
                break;
            }
        }
        next.truncate(INSTANTIATIONS_PER_LEMMA);
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
