//! The rules of a proof, each checked against the goal it proves.

use std::mem;
use std::rc::Rc;

use super::defs::{Admitted, Program, admit};
use super::eval::{Env, Machine, Nf, STEPS, Stop, Value, distinct};
use super::show::show;
use super::term::{Pat, Refused, TUPLES, Term};
use super::{Arg, Cert, Data, Equation, Failure, Goal, Lemma, Rule, Statement, Ty};
use crate::source::Span;

mod cases;

struct Checker<'a> {
    data: &'a Data,
    /// The definitions the statement and its proof name, admitted.
    defs: Admitted,
    /// The proofs checked before this one.
    lemmas: &'a [Lemma],
    /// Whether definitions unfold in normal forms.
    unfold: bool,
    /// The variables in scope and their types.
    vars: Vec<(Rc<str>, Ty)>,
    /// What case analysis knows of variables.
    refined: Vec<(Rc<str>, Value)>,
    /// The equations premises and `let`s name.
    facts: Vec<(Rc<str>, Equation)>,
    steps: u64,
    fresh: u32,
}

pub(super) fn prove(
    statement: &Statement,
    proof: &Cert,
    data: &Data,
    program: &dyn Program,
    lemmas: &[Lemma],
) -> Result<(), Failure> {
    let mut terms: Vec<&Term> = statement
        .premises
        .iter()
        .flat_map(|(_, eq)| [&eq.left, &eq.right])
        .collect();
    if let Goal::Equal(eq) = &statement.goal {
        terms.extend([&eq.left, &eq.right]);
    }
    proof.terms(&mut terms);
    // what the lemmas it uses state is unfolded where their premises are proved
    let mut used = Vec::new();
    proof.lemmas(&mut used);
    for lemma in lemmas
        .iter()
        .filter(|l| used.contains(&l.statement.name.as_str()))
    {
        let statement = &lemma.statement;
        terms.extend(
            statement
                .premises
                .iter()
                .flat_map(|(_, eq)| [&eq.left, &eq.right]),
        );
        if let Goal::Equal(eq) = &statement.goal {
            terms.extend([&eq.left, &eq.right]);
        }
    }
    let defs = admit(&terms, program, statement.span).map_err(Failure::Refused)?;
    let mut checker = Checker {
        data,
        defs,
        lemmas,
        unfold: true,
        vars: Vec::new(),
        refined: Vec::new(),
        facts: Vec::new(),
        steps: STEPS,
        fresh: 0,
    };
    for (x, ty) in &statement.params {
        checker.admit(ty, statement.span)?;
        checker.vars.push((x.clone(), ty.clone()));
    }
    for (h, eq) in &statement.premises {
        checker.admit_equation(eq, statement.span)?;
        checker.facts.push((h.clone(), eq.clone()));
    }
    match &statement.goal {
        Goal::Equal(eq) => {
            checker.admit_equation(eq, statement.span)?;
            checker.check(eq, proof)
        }
        Goal::SameType(a, b) => {
            checker.admit(a, statement.span)?;
            checker.admit(b, statement.span)?;
            if !matches!(proof.kind, Rule::Refl) {
                return Err(Failure::Invalid {
                    span: proof.span,
                    why: "the proof of `same_type` is `refl`".into(),
                });
            }
            if a == b {
                return Ok(());
            }
            let (left, right) = (a.to_string(), b.to_string());
            Err(if different_heads(a, b) {
                Failure::Disproved {
                    span: proof.span,
                    left,
                    right,
                }
            } else {
                Failure::Unproved {
                    span: proof.span,
                    left,
                    right,
                    note: None,
                }
            })
        }
    }
}

/// Types whose outermost constructors differ.
fn different_heads(a: &Ty, b: &Ty) -> bool {
    match (a, b) {
        (
            Ty::Data { module, name, args },
            Ty::Data {
                module: m,
                name: n,
                args: xs,
            },
        ) => (module, name) != (m, n) || args.iter().zip(xs).any(|(a, b)| different_heads(a, b)),
        (Ty::Fun(a, b), Ty::Fun(c, d)) => different_heads(a, c) || different_heads(b, d),
        (Ty::Rigid(_) | Ty::Refused(_), _) | (_, Ty::Rigid(_) | Ty::Refused(_)) => false,
        _ => mem::discriminant(a) != mem::discriminant(b),
    }
}

fn refused<T>(span: Span, why: impl Into<String>) -> Result<T, Failure> {
    Err(Failure::Refused(Refused {
        span,
        why: why.into(),
    }))
}

fn invalid<T>(span: Span, why: impl Into<String>) -> Result<T, Failure> {
    Err(Failure::Invalid {
        span,
        why: why.into(),
    })
}

impl Checker<'_> {
    /// Whether `ty` is a type of the domain.
    fn admit(&self, ty: &Ty, span: Span) -> Result<(), Failure> {
        match ty {
            Ty::Rigid(_) | Ty::Bool | Ty::Int => Ok(()),
            Ty::Refused(why) => refused(span, why.clone()),
            Ty::Fun(a, b) => {
                self.admit(a, span)?;
                self.admit(b, span)
            }
            Ty::Data { module, name, args } => {
                self.admit_data(module, name, span)?;
                args.iter().try_for_each(|a| self.admit(a, span))
            }
        }
    }

    fn admit_data(&self, module: &str, name: &str, span: Span) -> Result<(), Failure> {
        // a tuple is data of its elements, whose types are admitted apart
        if module == TUPLES {
            return Ok(());
        }
        match self.data.get(&(module.to_owned(), name.to_owned())) {
            Some(d) if d.recursive => refused(
                span,
                format!("`{name}` is recursive; v1 proofs are about non-recursive types"),
            ),
            Some(_) => Ok(()),
            None => refused(span, format!("the type `{name}` is outside the domain")),
        }
    }

    fn admit_equation(&self, eq: &Equation, span: Span) -> Result<(), Failure> {
        self.admit(&eq.ty, span)?;
        self.admit_term(&eq.left, span)?;
        self.admit_term(&eq.right, span)
    }

    /// Each constructor `t` names builds a type of the domain.
    fn admit_term(&self, t: &Term, span: Span) -> Result<(), Failure> {
        match t {
            Term::Var(_) | Term::Lit(_) | Term::Def(_) => Ok(()),
            Term::Con(c) => self.admit_data(&c.module, &c.ty, span),
            Term::Lam(_, b) => self.admit_term(b, span),
            Term::App(f, a) | Term::Let(_, f, a) => {
                self.admit_term(f, span)?;
                self.admit_term(a, span)
            }
            Term::Match(s, arms) => {
                self.admit_term(s, span)?;
                arms.iter().try_for_each(|a| {
                    self.admit_pat(&a.pat, span)?;
                    self.admit_term(&a.body, span)
                })
            }
        }
    }

    fn admit_pat(&self, p: &Pat, span: Span) -> Result<(), Failure> {
        match p {
            Pat::Con(c, fields) => {
                self.admit_data(&c.module, &c.ty, span)?;
                fields.iter().try_for_each(|f| self.admit_pat(f, span))
            }
            _ => Ok(()),
        }
    }

    fn nf(&mut self, t: &Term, span: Span) -> Result<Nf, Failure> {
        let mut machine = Machine::new(&self.refined, &self.defs, self.steps);
        machine.unfold = self.unfold;
        let found = machine
            .eval(t, &Env::default())
            .and_then(|v| machine.read(v, 0));
        self.steps = machine.steps;
        found.map_err(|stop| match stop {
            Stop::Budget => Failure::Exhausted { span },
            Stop::Missed => Failure::Refused(Refused {
                span,
                why: "a `match` here has no arm for its value".into(),
            }),
            Stop::Unadmitted => Failure::Refused(Refused {
                span,
                why: "it names a definition that was not admitted".into(),
            }),
        })
    }

    /// `ok` when `a` and `b` have the same normal form.
    fn same(
        &mut self,
        a: &Term,
        b: &Term,
        span: Span,
        note: Option<String>,
    ) -> Result<(), Failure> {
        let (l, r) = (self.nf(a, span)?, self.nf(b, span)?);
        if l == r {
            return Ok(());
        }
        let (left, right) = (show(&l), show(&r));
        Err(if note.is_none() && distinct(&l, &r) {
            Failure::Disproved { span, left, right }
        } else {
            Failure::Unproved {
                span,
                left,
                right,
                note,
            }
        })
    }

    fn check(&mut self, goal: &Equation, cert: &Cert) -> Result<(), Failure> {
        let span = cert.span;
        match &cert.kind {
            Rule::Refl => self.same(&goal.left, &goal.right, span, None),
            Rule::Symm(p) => self.check(&goal.swapped(), p),
            Rule::Trans(m, p, q) => {
                self.admit_term(m, span)?;
                let to = |left: &Term, right: &Term| Equation {
                    ty: goal.ty.clone(),
                    left: left.clone(),
                    right: right.clone(),
                };
                self.check(&to(&goal.left, m), p)?;
                self.check(&to(m, &goal.right), q)
            }
            Rule::Cong(x, e, p) => {
                self.admit_term(e, span)?;
                let eq = self.synth(p)?;
                let at =
                    |side: &Term| Term::Let(x.clone(), Rc::new(side.clone()), Rc::new(e.clone()));
                let note =
                    || Some("`cong` gives the context applied to both sides of its proof".into());
                self.same(&at(&eq.left), &goal.left, span, note())?;
                self.same(&at(&eq.right), &goal.right, span, note())
            }
            Rule::Funext(x, p) => {
                let Ty::Fun(a, b) = &goal.ty else {
                    return invalid(
                        span,
                        format!(
                            "`funext` proves functions equal; this goal is of `{}`",
                            goal.ty
                        ),
                    );
                };
                let mark = self.bind(x, a, span)?;
                let applied =
                    |f: &Term| Term::App(Rc::new(f.clone()), Rc::new(Term::Var(x.clone())));
                let eq = Equation {
                    ty: (**b).clone(),
                    left: applied(&goal.left),
                    right: applied(&goal.right),
                };
                let result = self.check(&eq, p);
                self.vars.truncate(mark);
                result
            }
            Rule::Case(v, arms) => self.case(goal, v, arms, span),
            Rule::Fact(h) => {
                let eq = self.fact(h, span)?;
                let note = Some(format!(
                    "`{h}` proves `{} = {}`",
                    self.show_term(&eq.left),
                    self.show_term(&eq.right)
                ));
                self.same(&eq.left, &goal.left, span, note.clone())?;
                self.same(&eq.right, &goal.right, span, note)
            }
            Rule::Lemma(name, args) => {
                let eq = self.lemma(name, args, span)?;
                // its instance is the goal itself, definitions folded
                let unfold = mem::replace(&mut self.unfold, false);
                let note = Some(format!(
                    "`{name}` proves `{} = {}` of exactly the definitions it names",
                    self.show_term(&eq.left),
                    self.show_term(&eq.right)
                ));
                let result = self
                    .same(&eq.left, &goal.left, span, note.clone())
                    .and_then(|()| self.same(&eq.right, &goal.right, span, note));
                self.unfold = unfold;
                result
            }
            Rule::Let(h, p, rest) => {
                let eq = self.synth(p)?;
                self.facts.push((h.clone(), eq));
                let result = self.check(goal, rest);
                self.facts.pop();
                result
            }
        }
    }

    /// The equation a proof proves, where it can be read off the proof.
    fn synth(&mut self, cert: &Cert) -> Result<Equation, Failure> {
        match &cert.kind {
            Rule::Fact(h) => self.fact(h, cert.span),
            Rule::Lemma(name, args) => self.lemma(name, args, cert.span),
            Rule::Symm(p) => Ok(self.synth(p)?.swapped()),
            Rule::Trans(middle, first, second) => {
                let (first, second) = (self.synth(first)?, self.synth(second)?);
                self.same(&first.right, middle, cert.span, None)?;
                self.same(middle, &second.left, cert.span, None)?;
                Ok(Equation {
                    ty: first.ty,
                    left: first.left,
                    right: second.right,
                })
            }
            _ => invalid(
                cert.span,
                "what this proof proves is not written here: name a premise, or `let h = …` a proof of a goal",
            ),
        }
    }

    /// What lemma `name` proves of `args`, once its premises are proved of
    /// them: each side applied to the values, so that they are bound apart
    /// from the names of this proof.
    fn lemma(&mut self, name: &str, args: &[Arg], span: Span) -> Result<Equation, Failure> {
        let Some(lemma) = self.lemmas.iter().find(|l| l.statement.name == name) else {
            return invalid(
                span,
                format!("`{name}` is no proof checked before this one"),
            );
        };
        let Goal::Equal(conclusion) = &lemma.statement.goal else {
            return invalid(
                span,
                format!("`{name}` proves two types the same, not an equation"),
            );
        };
        let params: Vec<&Rc<str>> = (lemma.order.iter())
            .filter(|(_, p)| !p)
            .map(|(x, _)| x)
            .collect();
        let values: Vec<&Term> = (args.iter())
            .filter_map(|a| match a {
                Arg::Value(t) => Some(t),
                Arg::Proof(_) => None,
            })
            .collect();
        let at = |side: &Term| {
            let lam = params
                .iter()
                .rev()
                .fold(side.clone(), |b, x| Term::Lam((*x).clone(), Rc::new(b)));
            values
                .iter()
                .fold(lam, |f, a| Term::App(Rc::new(f), Rc::new((*a).clone())))
        };
        let instance = |eq: &Equation| Equation {
            ty: eq.ty.clone(),
            left: at(&eq.left),
            right: at(&eq.right),
        };
        let (premises, conclusion) = (lemma.statement.premises.clone(), instance(conclusion));
        let proofs = (lemma.order.iter().zip(args)).filter_map(|((x, _), a)| match a {
            Arg::Proof(p) => Some((x, p)),
            Arg::Value(_) => None,
        });
        for (h, proof) in proofs {
            let eq = premises
                .iter()
                .find(|(n, _)| n == h)
                .map(|(_, eq)| instance(eq));
            let eq = eq.ok_or_else(|| Failure::Invalid {
                span,
                why: format!("`{h}` is no premise of `{name}`"),
            })?;
            self.check(&eq, proof)?;
        }
        Ok(conclusion)
    }

    fn fact(&self, h: &str, span: Span) -> Result<Equation, Failure> {
        match self.facts.iter().rev().find(|(n, _)| &**n == h) {
            Some((_, eq)) => Ok(eq.clone()),
            None => invalid(span, format!("`{h}` is no premise and no proof named here")),
        }
    }

    /// Brings `x : ty` into scope; the mark to drop it at.
    fn bind(&mut self, x: &Rc<str>, ty: &Ty, span: Span) -> Result<usize, Failure> {
        if self.vars.iter().any(|(n, _)| n == x) {
            return invalid(
                span,
                format!("`{x}` is already a variable of the statement"),
            );
        }
        let mark = self.vars.len();
        self.vars.push((x.clone(), ty.clone()));
        Ok(mark)
    }
}

impl Checker<'_> {
    fn show_term(&self, t: &Term) -> String {
        let mut machine = Machine::new(&self.refined, &self.defs, STEPS);
        machine.unfold = self.unfold;
        machine
            .eval(t, &Env::default())
            .and_then(|v| machine.read(v, 0))
            .map_or_else(|_| "…".into(), |nf| show(&nf))
    }
}
