//! The rules of a proof, each checked against the goal it proves.

use std::mem;
use std::rc::Rc;

use super::eval::{Env, Machine, Nf, STEPS, Stop, Value, distinct};
use super::show::show;
use super::term::{Literal, Pat, Refused, Term};
use super::{Cert, Data, Equation, Failure, Goal, Rule, Statement, Ty};
use crate::source::Span;
use flick_ir::Lit;

struct Checker<'a> {
    data: &'a Data,
    /// The variables in scope and their types.
    vars: Vec<(Rc<str>, Ty)>,
    /// What case analysis knows of variables.
    refined: Vec<(Rc<str>, Value)>,
    /// The equations premises and `let`s name.
    facts: Vec<(Rc<str>, Equation)>,
    steps: u64,
    fresh: u32,
}

pub(super) fn prove(statement: &Statement, proof: &Cert, data: &Data) -> Result<(), Failure> {
    let mut checker = Checker {
        data,
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
            Ty::Rigid(_) | Ty::Bool => Ok(()),
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
            Term::Var(_) | Term::Lit(_) => Ok(()),
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
        let mut machine = Machine::new(&self.refined, self.steps);
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
                    show_term(&eq.left),
                    show_term(&eq.right)
                ));
                self.same(&eq.left, &goal.left, span, note.clone())?;
                self.same(&eq.right, &goal.right, span, note)
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

    fn case(
        &mut self,
        goal: &Equation,
        v: &Rc<str>,
        arms: &[(Pat, Cert)],
        span: Span,
    ) -> Result<(), Failure> {
        let Some((_, ty)) = self.vars.iter().rev().find(|(n, _)| n == v).cloned() else {
            return invalid(span, format!("`{v}` is no variable of the statement"));
        };
        if self.refined.iter().any(|(n, _)| n == v) {
            return invalid(span, format!("`{v}` is already split"));
        }
        let cases: Vec<(Pat, Vec<Ty>)> = match &ty {
            Ty::Bool => [true, false]
                .map(|b| (Pat::Lit(Literal(Lit::Bool(b))), Vec::new()))
                .into(),
            Ty::Data { module, name, args } => {
                self.admit_data(module, name, span)?;
                let decl = self
                    .data
                    .get(&(module.clone(), name.clone()))
                    .expect("admitted above");
                decl.constructors
                    .iter()
                    .map(|(c, fields)| {
                        let fields = fields
                            .iter()
                            .map(|f| f.instantiate(&decl.params, args))
                            .collect();
                        (Pat::Bind(c.as_str().into()), fields)
                    })
                    .collect()
            }
            _ => {
                return invalid(
                    span,
                    format!("`{v}` is of `{ty}`, which has no constructors to split"),
                );
            }
        };
        let mut covered = vec![false; cases.len()];
        for (pat, proof) in arms {
            let (index, binders) = self.arm(pat, &cases, &covered, proof.span)?;
            let Some(index) = index else {
                // a variable or `_`: every case not covered yet, with `v` as it is
                covered.fill(true);
                self.check(goal, proof)?;
                continue;
            };
            covered[index] = true;
            let mark = self.vars.len();
            let mut fields = Vec::new();
            for (b, field_ty) in binders.into_iter().zip(&cases[index].1) {
                self.bind(&b, field_ty, proof.span)?;
                fields.push(Value::Sym(b));
            }
            let value = match pat {
                Pat::Lit(l) => Value::Lit(l.clone()),
                Pat::Con(c, _) => Value::Con(c.clone(), fields.into()),
                _ => unreachable!("an arm of a case is a constructor or a literal"),
            };
            self.refined.push((v.clone(), value));
            let result = self.check(goal, proof);
            self.refined.pop();
            self.vars.truncate(mark);
            result?;
        }
        if let Some(i) = covered.iter().position(|c| !c) {
            return invalid(
                span,
                format!("the split of `{v}` misses `{}`", case_name(&cases[i].0)),
            );
        }
        Ok(())
    }

    /// Which case an arm proves, and the names of its fields.
    fn arm(
        &mut self,
        pat: &Pat,
        cases: &[(Pat, Vec<Ty>)],
        covered: &[bool],
        span: Span,
    ) -> Result<(Option<usize>, Vec<Rc<str>>), Failure> {
        let index = match pat {
            Pat::Wild | Pat::Bind(_) => return Ok((None, Vec::new())),
            Pat::Lit(l) => cases.iter().position(|(p, _)| *p == Pat::Lit(l.clone())),
            Pat::Con(c, _) => cases.iter().position(|(p, _)| case_name(p) == c.name),
        };
        let Some(index) = index else {
            return invalid(span, "this arm is no case of the variable's type");
        };
        if covered[index] {
            return invalid(
                span,
                format!("`{}` is split twice", case_name(&cases[index].0)),
            );
        }
        let mut binders = Vec::new();
        if let Pat::Con(_, fields) = pat {
            for field in fields {
                binders.push(match field {
                    Pat::Bind(b) => b.clone(),
                    Pat::Wild => {
                        self.fresh += 1;
                        format!("$field{}", self.fresh).into()
                    }
                    _ => return invalid(span, "a split names each field: `C x y`, nested patterns are a split of a field"),
                });
            }
        }
        Ok((Some(index), binders))
    }
}

fn case_name(p: &Pat) -> String {
    match p {
        Pat::Bind(c) => c.to_string(),
        Pat::Lit(Literal(Lit::Bool(b))) => b.to_string(),
        _ => "a case".into(),
    }
}

fn show_term(t: &Term) -> String {
    let mut machine = Machine::new(&[], STEPS);
    machine
        .eval(t, &Env::default())
        .and_then(|v| machine.read(v, 0))
        .map_or_else(|_| "…".into(), |nf| show(&nf))
}
