//! A proof as a finite certificate, read from the IR of its body: each node
//! is one rule. A body of any other form is no certificate.

use std::rc::Rc;

use flick_ir::{Expr, ExprKind, Pat as IrPat, Stmt};

use super::term::{LEMMAS, Pat, RULES, Term, pat, term};
use super::{Failure, Lemma};
use crate::source::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cert {
    pub kind: Rule,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    Refl,
    Symm(Box<Cert>),
    /// `trans m p q`: `p` proves `l = m`, `q` proves `m = r`.
    Trans(Term, Box<Cert>, Box<Cert>),
    /// `cong (fun x -> e) p`: `p` proves `a = b`, so `e[a/x] = e[b/x]`.
    Cong(Rc<str>, Term, Box<Cert>),
    /// `funext (fun x -> p)`: `p` proves `f x = g x` for a fresh `x`.
    Funext(Rc<str>, Box<Cert>),
    /// `match v with …`: one proof for each constructor of `v`'s type.
    Case(Rc<str>, Vec<(Pat, Cert)>),
    /// A premise or a proof a `let` named.
    Fact(Rc<str>),
    /// `let h = p` before the rest of the proof.
    Let(Rc<str>, Box<Cert>, Box<Cert>),
    /// A proof checked before this one, given a value for each of its
    /// parameters and a proof for each of its premises.
    Lemma(Rc<str>, Vec<Arg>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    Value(Term),
    Proof(Cert),
}

fn invalid<T>(span: Span, why: impl Into<String>) -> Result<T, Failure> {
    Err(Failure::Invalid {
        span,
        why: why.into(),
    })
}

/// The certificate the body `e` of a proof spells.
pub fn cert(e: &Expr, lemmas: &[Lemma]) -> Result<Cert, Failure> {
    let span = e.span;
    let kind = match &e.kind {
        ExprKind::Local(h) => Rule::Fact(h.as_str().into()),
        ExprKind::Match(scrutinee, arms) => {
            let ExprKind::Local(v) = &scrutinee.kind else {
                return invalid(
                    scrutinee.span,
                    "a proof splits a variable: `match v with …`",
                );
            };
            let arms = arms
                .iter()
                .map(|a| {
                    if a.guard.is_some() {
                        return invalid(a.body.span, "an arm of a proof has no guard");
                    }
                    Ok((
                        pat(&a.pat, a.body.span).map_err(Failure::Refused)?,
                        cert(&a.body, lemmas)?,
                    ))
                })
                .collect::<Result<_, _>>()?;
            Rule::Case(v.as_str().into(), arms)
        }
        ExprKind::Block(stmts, rest) => {
            let mut out = cert(rest, lemmas)?;
            for stmt in stmts.iter().rev() {
                let Stmt::Let {
                    pat: IrPat::Bind(h),
                    value: p,
                    ..
                } = stmt
                else {
                    return invalid(span, "a `let` in a proof names a proof: `let h = …`");
                };
                out = Cert {
                    kind: Rule::Let(h.as_str().into(), Box::new(cert(p, lemmas)?), Box::new(out)),
                    span: p.span,
                };
            }
            return Ok(out);
        }
        _ => rule(e, lemmas)?,
    };
    Ok(Cert { kind, span })
}

/// A rule applied to its arguments.
fn rule(e: &Expr, lemmas: &[Lemma]) -> Result<Rule, Failure> {
    let mut args = Vec::new();
    let mut head = e;
    while let ExprKind::App { func, arg, .. } = &head.kind {
        args.push(&**arg);
        head = func;
    }
    args.reverse();
    let ExprKind::Global { module, name, .. } = &head.kind else {
        return invalid(
            e.span,
            "this is no proof: a proof is `refl`, a rule applied, a `match` on a variable or a premise",
        );
    };
    if module == LEMMAS {
        return lemma(e, name, &args, lemmas);
    }
    if module != RULES {
        return invalid(e.span, format!("`{name}` is no rule of a proof"));
    }
    let boxed = |e: &Expr| cert(e, lemmas).map(Box::new);
    Ok(match (name.as_str(), args.as_slice()) {
        ("refl", []) => Rule::Refl,
        ("symm", [p]) => Rule::Symm(boxed(p)?),
        ("trans", [m, p, q]) => {
            Rule::Trans(term(m).map_err(Failure::Refused)?, boxed(p)?, boxed(q)?)
        }
        ("cong", [context, p]) => {
            let (x, body) = binder(context)?;
            Rule::Cong(x, term(body).map_err(Failure::Refused)?, boxed(p)?)
        }
        ("funext", [p]) => {
            let (x, body) = binder(p)?;
            Rule::Funext(x, boxed(body)?)
        }
        (rule, _) => return invalid(e.span, format!("`{rule}` {}", arity(rule))),
    })
}

/// `name args` of a lemma proved before: a value for each parameter and a
/// proof for each premise, in the order it declares them.
fn lemma(e: &Expr, name: &str, args: &[&Expr], lemmas: &[Lemma]) -> Result<Rule, Failure> {
    let Some(lemma) = lemmas.iter().find(|l| l.statement.name == name) else {
        return invalid(
            e.span,
            format!("`{name}` is no proof checked before this one"),
        );
    };
    if args.len() != lemma.order.len() {
        return invalid(
            e.span,
            format!(
                "`{name}` takes {} arguments: a value for each parameter, a proof for each premise",
                lemma.order.len()
            ),
        );
    }
    let args = (lemma.order.iter().zip(args))
        .map(|((_, premise), arg)| match premise {
            true => cert(arg, lemmas).map(Arg::Proof),
            false => term(arg).map(Arg::Value).map_err(Failure::Refused),
        })
        .collect::<Result<_, _>>()?;
    Ok(Rule::Lemma(name.into(), args))
}

/// `fun x -> body` of one parameter.
fn binder(e: &Expr) -> Result<(Rc<str>, &Expr), Failure> {
    match &e.kind {
        ExprKind::Lambda { params, body } if params.len() == 1 => match &params[0].pat {
            IrPat::Bind(x) => Ok((x.as_str().into(), body)),
            _ => invalid(e.span, "the parameter of this rule is a name"),
        },
        _ => invalid(e.span, "this rule takes `fun x -> …` of one parameter"),
    }
}

fn arity(rule: &str) -> &'static str {
    match rule {
        "refl" => "takes no argument",
        "symm" => "takes one proof",
        "trans" => "takes the middle value and two proofs",
        "cong" => "takes `fun x -> e` and one proof",
        "funext" => "takes `fun x -> proof`",
        _ => "is no rule of a proof",
    }
}

impl Cert {
    /// The lemmas the certificate uses.
    pub fn lemmas<'a>(&'a self, out: &mut Vec<&'a str>) {
        match &self.kind {
            Rule::Lemma(name, args) => {
                out.push(name);
                for a in args {
                    if let Arg::Proof(p) = a {
                        p.lemmas(out);
                    }
                }
            }
            Rule::Symm(p) | Rule::Funext(_, p) | Rule::Cong(_, _, p) => p.lemmas(out),
            Rule::Trans(_, p, q) | Rule::Let(_, p, q) => {
                p.lemmas(out);
                q.lemmas(out);
            }
            Rule::Case(_, arms) => arms.iter().for_each(|(_, p)| p.lemmas(out)),
            Rule::Refl | Rule::Fact(_) => {}
        }
    }

    /// The terms the certificate writes: what its rules name too.
    pub fn terms<'a>(&'a self, out: &mut Vec<&'a Term>) {
        match &self.kind {
            Rule::Refl | Rule::Fact(_) => {}
            Rule::Symm(p) | Rule::Funext(_, p) => p.terms(out),
            Rule::Trans(m, p, q) => {
                out.push(m);
                p.terms(out);
                q.terms(out);
            }
            Rule::Cong(_, e, p) => {
                out.push(e);
                p.terms(out);
            }
            Rule::Case(_, arms) => arms.iter().for_each(|(_, p)| p.terms(out)),
            Rule::Let(_, p, rest) => {
                p.terms(out);
                rest.terms(out);
            }
            Rule::Lemma(_, args) => args.iter().for_each(|a| match a {
                Arg::Value(t) => out.push(t),
                Arg::Proof(p) => p.terms(out),
            }),
        }
    }
}
