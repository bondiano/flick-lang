//! The terms of the logic and their strict extraction from checked IR: a
//! form the domain has no meaning for is refused with the reason, never
//! read as something else.

use std::rc::Rc;

use flick_ir::{Dict, Expr, ExprKind, Lit, Pat as IrPat, Stage, Stmt};

use crate::source::Span;

/// A constructor: the type it builds and how many fields it takes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ctor {
    pub module: String,
    pub ty: String,
    pub name: String,
    pub arity: usize,
}

/// A literal, compared by what it spells: a `Float` by its bits.
#[derive(Debug, Clone)]
pub struct Literal(pub Lit);

impl PartialEq for Literal {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Lit::Int(a), Lit::Int(b)) => a == b,
            (Lit::Float(a), Lit::Float(b)) => a.to_bits() == b.to_bits(),
            (Lit::BigInt(a), Lit::BigInt(b)) | (Lit::String(a), Lit::String(b)) => a == b,
            (Lit::Bool(a), Lit::Bool(b)) => a == b,
            (Lit::Unit, Lit::Unit) => true,
            _ => false,
        }
    }
}

impl Eq for Literal {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    Var(Rc<str>),
    Lit(Literal),
    Lam(Rc<str>, Rc<Term>),
    App(Rc<Term>, Rc<Term>),
    /// A constructor as a function of its fields.
    Con(Rc<Ctor>),
    Match(Rc<Term>, Rc<[Arm]>),
    Let(Rc<str>, Rc<Term>, Rc<Term>),
    /// A definition the statement names: unfolded once admitted.
    Def(Rc<Def>),
}

/// A definition: a top-level value, or the method of an instance by the
/// dictionary the checker resolved for the use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Def {
    Value { module: String, name: String },
    Method { dict: Dict, name: String },
}

impl Def {
    /// How a message names it.
    pub fn shown(&self) -> String {
        match self {
            Def::Value { module, name } => format!("{module}.{name}"),
            Def::Method { name, .. } => name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    pub pat: Pat,
    pub body: Term,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pat {
    Wild,
    Bind(Rc<str>),
    Lit(Literal),
    Con(Rc<Ctor>, Vec<Pat>),
}

/// Why a form does not enter a proof, and where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub span: Span,
    pub why: String,
}

fn refuse<T>(span: Span, why: impl Into<String>) -> Result<T, Refused> {
    Err(Refused {
        span,
        why: why.into(),
    })
}

/// The module rules of a proof are written in: no source spells it.
pub const RULES: &str = "$proof";

/// `e` as a term of the domain.
pub fn term(e: &Expr) -> Result<Term, Refused> {
    Ok(match &e.kind {
        ExprKind::Lit(l) => Term::Lit(Literal(l.clone())),
        ExprKind::Local(name) => Term::Var(name.as_str().into()),
        ExprKind::Global { module, .. } if module == RULES => {
            return refuse(e.span, "a rule of a proof is no value of the statement");
        }
        ExprKind::Global { module, name } => Term::Def(Rc::new(Def::Value {
            module: module.clone(),
            name: name.clone(),
        })),
        ExprKind::Method { dict, name } => Term::Def(Rc::new(Def::Method {
            dict: dict.clone(),
            name: name.clone(),
        })),
        ExprKind::Con(c) => Term::Con(Rc::new(Ctor {
            module: c.module.clone(),
            ty: c.ty.clone(),
            name: c.name.clone(),
            arity: c.arity,
        })),
        ExprKind::App {
            func,
            arg,
            stage: Stage::Pure,
            ..
        } => Term::App(Rc::new(term(func)?), Rc::new(term(arg)?)),
        ExprKind::Lambda { params, body } => {
            let mut out = term(body)?;
            for (i, param) in params.iter().enumerate().rev() {
                if param.stage != Stage::Pure {
                    return refuse(e.span, "a function with effects is outside the domain");
                }
                let name: Rc<str> = match &param.pat {
                    IrPat::Bind(n) => n.as_str().into(),
                    // `()` has one value: matching it binds nothing and never fails
                    IrPat::Wild | IrPat::Lit(Lit::Unit) => format!("$unused{i}").into(),
                    _ => return refuse(e.span, "a parameter pattern is not supported in a proof"),
                };
                out = Term::Lam(name, Rc::new(out));
            }
            out
        }
        ExprKind::Block(stmts, result) => {
            let mut out = term(result)?;
            for stmt in stmts.iter().rev() {
                let Stmt::Let(IrPat::Bind(name), value) = stmt else {
                    return refuse(e.span, "only `let name = value` is supported in a proof");
                };
                out = Term::Let(name.as_str().into(), Rc::new(term(value)?), Rc::new(out));
            }
            out
        }
        ExprKind::If(c, t, f) => {
            let arm = |b, body: &Expr| -> Result<Arm, Refused> {
                Ok(Arm {
                    pat: Pat::Lit(Literal(Lit::Bool(b))),
                    body: term(body)?,
                })
            };
            Term::Match(
                Rc::new(term(c)?),
                vec![arm(true, t)?, arm(false, f)?].into(),
            )
        }
        ExprKind::Match(scrutinee, arms) => {
            let arms = arms
                .iter()
                .map(|a| {
                    if a.guard.is_some() {
                        return refuse(a.body.span, "a guard is not supported in a proof");
                    }
                    Ok(Arm {
                        pat: pat(&a.pat, a.body.span)?,
                        body: term(&a.body)?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Term::Match(Rc::new(term(scrutinee)?), arms.into())
        }
        ExprKind::App { .. } => {
            return refuse(e.span, "a call with effects is outside the domain");
        }
        other => return refuse(e.span, format!("{} is outside the domain", describe(other))),
    })
}

pub fn pat(p: &IrPat, span: Span) -> Result<Pat, Refused> {
    Ok(match p {
        IrPat::Wild => Pat::Wild,
        IrPat::Bind(n) => Pat::Bind(n.as_str().into()),
        IrPat::Lit(l) => Pat::Lit(Literal(l.clone())),
        IrPat::Con(c, fields) => Pat::Con(
            Rc::new(Ctor {
                module: c.module.clone(),
                ty: c.ty.clone(),
                name: c.name.clone(),
                arity: c.arity,
            }),
            fields
                .iter()
                .map(|f| pat(f, span))
                .collect::<Result<_, _>>()?,
        ),
        _ => return refuse(span, "this pattern is not supported in a proof"),
    })
}

/// What the domain calls a form it refuses.
fn describe(kind: &ExprKind) -> &'static str {
    match kind {
        ExprKind::Var(_) => "a mutable variable",
        ExprKind::Perform { .. } => "an operation of an effect",
        ExprKind::Generate { .. } | ExprKind::Each { .. } | ExprKind::Yield(_) => "a sequence",
        ExprKind::Loop { .. } | ExprKind::ControlLoop { .. } => "a loop",
        ExprKind::Tuple(_) => "a tuple",
        ExprKind::List(_) => "a list",
        ExprKind::Record(_) | ExprKind::Update(..) | ExprKind::Field(..) => "a record",
        ExprKind::Dictionary(_) => "a dictionary",
        ExprKind::Binary(..) | ExprKind::Not(_) | ExprKind::Neg(..) => "a primitive operation",
        ExprKind::Await(_) | ExprKind::Task(_) => "a runtime computation",
        _ => "this form",
    }
}
