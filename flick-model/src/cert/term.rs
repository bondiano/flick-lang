//! The proof of one typing: the rules of `formal/Formal/Rules.lean` on the
//! terms the generator makes, with the types of the trace.

use crate::check::Inst;
use std::collections::BTreeMap;
use std::iter;

use crate::check::trace::{Node, Trace, key};
use crate::syntax::{Lit, Name, Term};
use crate::translate::lean::elide::{Env, caps, row, stage, ty};
use crate::types::{Caps, Fields, Row, Stage, Type};

use super::Unsupported;

/// The term `e` in the core, its variables by de Bruijn index (`ctx`
/// innermost first).
pub(super) fn tm(e: &Term, ctx: &[Name]) -> Result<String, Unsupported> {
    Ok(match e {
        Term::Var(x) => format!("(.var {})", index(ctx, x)?),
        Term::Lit(l) => format!("(.lit {})", lit(l)?),
        Term::Lam(x, b) => format!("(.lam {})", tm(b, &push(ctx, x))?),
        Term::App(f, a) => format!("(.app {} {})", tm(f, ctx)?, tm(a, ctx)?),
        Term::Let(x, _, e1, e2) => format!("(.let_ {} {})", tm(e1, ctx)?, tm(e2, &push(ctx, x))?),
        Term::Task(b) => format!("(.task {})", tm(b, ctx)?),
        Term::Await(x) => format!("(.await {})", tm(x, ctx)?),
        // a record of the core is a spine in the order of its labels, as
        // its type is written
        Term::Record(fs) => {
            let mut out = ".rnil".to_owned();
            for (l, x) in sorted(fs).into_iter().rev() {
                out = format!("(.rcons \"{l}\" {} {out})", tm(x, ctx)?);
            }
            out
        }
        Term::Field(r, l) => format!("(.proj {} \"{l}\")", tm(r, ctx)?),
        Term::Update(r, l, v) => format!("(.update {} \"{l}\" {})", tm(r, ctx)?, tm(v, ctx)?),
        other => return Err(form(other)),
    })
}

/// The fields of a record term by label.
fn sorted(fs: &[(Name, Term)]) -> Vec<(&Name, &Term)> {
    let mut fs: Vec<_> = fs.iter().map(|(l, x)| (l, x)).collect();
    fs.sort_by(|a, b| a.0.cmp(b.0));
    fs
}

/// The type of field `l` of a record type.
fn field<'a>(t: &'a Type, l: &str) -> Result<&'a Type, Unsupported> {
    let f = fields(t).ok_or("a field of no record")?;
    f.fields
        .get(l)
        .ok_or_else(|| format!("a field `{l}` the type does not have"))
}

/// The fields of a record type.
fn fields(t: &Type) -> Option<&Fields> {
    match t {
        Type::Record(f) if f.tail.is_none() => Some(f),
        Type::Captured(x, _) | Type::Bounded(x, _) => fields(x),
        _ => None,
    }
}

/// The computation `t` is: its rows, result and qualifier.
fn eff(t: &Type) -> Option<(&Row, &Row, &Type, Caps)> {
    match t {
        Type::Eff(r, e, a) => Some((r, e, a, Caps::default())),
        Type::Captured(x, c) => match &**x {
            Type::Eff(r, e, a) => Some((r, e, a, c.clone())),
            _ => None,
        },
        Type::Bounded(x, _) => eff(x),
        _ => None,
    }
}

fn push(ctx: &[Name], x: &Name) -> Vec<Name> {
    iter::once(x.clone()).chain(ctx.iter().cloned()).collect()
}

fn index(ctx: &[Name], x: &str) -> Result<usize, Unsupported> {
    ctx.iter()
        .position(|y| &**y == x)
        .ok_or_else(|| format!("`{x}` is not bound"))
}

fn lit(l: &Lit) -> Result<String, Unsupported> {
    Ok(match l {
        Lit::Unit => ".unit".into(),
        Lit::Bool(b) => format!("(.bool {b})"),
        Lit::Int(n) if *n < 0 => format!("(.int ({n}))"),
        Lit::Int(n) => format!("(.int {n})"),
        Lit::String(s) => format!("(.str {s:?})"),
        Lit::Float(f) => return Err(format!("the literal {f}")),
    })
}

/// What kind of term the certificates do not write yet.
fn form(e: &Term) -> Unsupported {
    let s = format!("{e:?}");
    format!("the form {}", s.split(['(', ' ']).next().unwrap_or(""))
}

/// The arrow of `t`: its parameter, stage, result and qualifier.
fn arrow(t: &Type) -> Option<(&Type, &Stage, &Type, Caps)> {
    match t {
        Type::Fun(p, s, r) => Some((p, s, r, Caps::default())),
        Type::Captured(f, c) => match &**f {
            Type::Fun(p, s, r) => Some((p, s, r, c.clone())),
            _ => None,
        },
        Type::Bounded(f, _) => arrow(f),
        _ => None,
    }
}

/// A proof of a typing, at the depth `col` of its line.
pub(super) struct Cx<'t> {
    trace: &'t Trace,
    /// The term variables, innermost first.
    terms: Vec<Name>,
    /// The type variables, innermost first.
    env: Env,
}

impl<'t> Cx<'t> {
    pub(super) fn new(trace: &'t Trace) -> Self {
        Cx {
            trace,
            terms: Vec::new(),
            env: Env::default(),
        }
    }

    fn node(&self, e: &Term) -> Result<&'t Node, Unsupported> {
        self.trace
            .get(&key(e))
            .ok_or_else(|| "a term the check did not type".to_owned())
    }

    fn ty(&self, t: &Type) -> Result<String, Unsupported> {
        ty(t, &self.env)
    }

    fn arrow_args(&self, t: &Type) -> Result<String, Unsupported> {
        let (p, s, r, c) = arrow(t).ok_or_else(|| format!("`{t}` is no arrow"))?;
        Ok(format!(
            "(a := {}) (ψ := {}) (t := {}) (C := {})",
            self.ty(p)?,
            stage(s, &self.env)?,
            self.ty(r)?,
            caps(&c, &self.env)?
        ))
    }

    fn eff_args(&self, t: &Type) -> Result<String, Unsupported> {
        let (r, e, a, c) = eff(t).ok_or_else(|| format!("`{t}` is no computation"))?;
        Ok(format!(
            "(r := {}) (ε := {}) (t := {}) (C := {})",
            row(r, &self.env)?,
            row(e, &self.env)?,
            self.ty(a)?,
            caps(&c, &self.env)?
        ))
    }

    fn inst(&self, i: &Inst) -> Result<String, Unsupported> {
        Ok(match i {
            Inst::Type(t) => self.ty(t)?,
            Inst::Row(r) => format!("(.row {})", row(r, &self.env)?),
            Inst::Caps(c) => format!("(.caps {})", caps(c, &self.env)?),
            Inst::Tail(_) => return Err("an open record".into()),
        })
    }

    /// A proof that `e` has type `t` where it stands.
    pub(super) fn typing(&mut self, e: &Term, t: &Type, col: usize) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        match e {
            Term::Lit(l) => lit(l).map(|_| ".lit".into()),
            Term::Var(x) => self.var(e, x),
            Term::Lam(x, body) => {
                let (_, _, r, _) = arrow(t).ok_or_else(|| format!("a λ at `{t}`"))?;
                let args = self.arrow_args(t)?;
                let b = self.under(x, |c| c.typing(body, r, col + 2))?;
                Ok(format!(
                    "(.lam {args}{pad}{b}{pad}(by simp [Tm.NoIns, Tm.Has]) (by le_live))"
                ))
            }
            Term::App(..) => self.app(e, col),
            Term::Task(body) => {
                let args = self.eff_args(t)?;
                let (_, _, a, _) = eff(t).ok_or("a task at no `Eff`")?;
                let b = self.typing(body, a, col + 2)?;
                Ok(format!(
                    "(.task {args}{pad}{b}{pad}(Stage.le_refl _) (by simp [Tm.NoIns, Tm.Has]) (by le_live))"
                ))
            }
            Term::Await(x) => {
                let at = self.inferred(x)?;
                let args = self.eff_args(&at)?;
                let x = self.typing(x, &at, col + 2)?;
                Ok(format!(
                    "(.await {args}{pad}{x}{pad}(by stage_le) (by le_live))"
                ))
            }
            Term::Record(fs) => self.record(fs, t, col),
            Term::Field(r, l) => {
                let at = self.inferred(r)?;
                let ft = field(&at, l)?;
                let (rt, ftp) = (self.ty(&at)?, self.ty(ft)?);
                let pr = self.typing(r, &at, col + 2)?;
                Ok(format!(
                    "(.proj (R := {rt}) (t := {ftp}){pad}{pr}{pad}rfl (by le_live))"
                ))
            }
            Term::Update(r, l, v) => {
                let ft = field(t, l)?;
                let (rt, ftp) = (self.ty(t)?, self.ty(ft)?);
                let pr = self.typing(r, t, col + 2)?;
                let pv = self.typing(v, ft, col + 2)?;
                Ok(format!(
                    "(.update (R := {rt}) (t := {ftp}){pad}{pr}{pad}rfl{pad}{pv})"
                ))
            }
            Term::Let(x, _, e1, e2) => self.let_(x, e1, e2, t, col),
            other => Err(form(other)),
        }
    }

    fn inferred(&self, e: &Term) -> Result<Type, Unsupported> {
        self.node(e)?
            .inferred
            .clone()
            .ok_or_else(|| "a term without its type".into())
    }

    /// `check` with `x` bound, innermost.
    fn under<T>(&mut self, x: &Name, check: impl FnOnce(&mut Self) -> T) -> T {
        self.terms.insert(0, x.clone());
        let out = check(self);
        self.terms.remove(0);
        out
    }

    /// (Var) at the instance the check took.
    fn var(&mut self, e: &Term, x: &str) -> Result<String, Unsupported> {
        index(&self.terms, x)?;
        let node = self.node(e)?;
        let (scheme, insts) = node
            .instance
            .as_ref()
            .ok_or_else(|| format!("`{x}` without its instance"))?;
        if !scheme.outlives.is_empty() {
            return Err("a scheme with facts".into());
        }
        let ts = insts
            .iter()
            .map(|i| self.inst(i))
            .collect::<Result<Vec<_>, _>>()?;
        let instance = self.ty(&self.inferred(e)?)?;
        Ok(format!(
            "(Cert.typ_var (ts := [{}]) (T' := {instance}) rfl rfl (Cert.holds_none _) (by inst_eq) (by le_live))",
            ts.join(", ")
        ))
    }

    /// (Rec): the fields by label, each at its type.
    fn record(&mut self, fs: &[(Name, Term)], t: &Type, col: usize) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let f = fields(t).ok_or("a record at no record type")?;
        let mut proof = ".rnil".to_owned();
        let mut rest = Fields {
            fields: BTreeMap::new(),
            tail: None,
        };
        for (l, x) in sorted(fs).into_iter().rev() {
            let ft = f.fields.get(l).ok_or("a field the type does not have")?;
            let px = self.typing(x, ft, col + 2)?;
            let r = self.ty(&Type::Record(rest.clone()))?;
            proof = format!("(.rcons (R := {r}) trivial (by decide){pad}{px}{pad}{proof})");
            rest.fields.insert(l.clone(), ft.clone());
        }
        Ok(proof)
    }

    /// (Let-Mono) or, for a scheme with variables, (Let-Gen).
    fn let_(
        &mut self,
        x: &Name,
        e1: &Term,
        e2: &Term,
        t: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let scheme = self
            .node(e1)?
            .scheme
            .clone()
            .ok_or("a `let` without its scheme")?;
        if !scheme.outlives.is_empty() {
            return Err("a scheme with facts".into());
        }
        let k = scheme.vars.len();
        let outer = self.env.clone();
        self.env.vars = scheme
            .vars
            .iter()
            .map(|(v, _)| v.to_string())
            .chain(outer.vars.iter().cloned())
            .collect();
        let a = self.ty(&scheme.body);
        let rhs = self.typing(e1, &scheme.body, col + 2);
        self.env = outer;
        let (a, rhs) = (a?, rhs?);
        let rest = self.under(x, |c| c.typing(e2, t, col + 2))?;
        Ok(if k == 0 {
            format!("(.let_ (a := {a}){pad}{rhs}{pad}{rest})")
        } else {
            format!("(.letGen (k := {k}) (Φ := Facts.none) (a := {a}){pad}{rhs}{pad}{rest})")
        })
    }

    /// `f a₁ … aₙ`: each call at the arrow the instance of `f` gives.
    fn app(&mut self, e: &Term, col: usize) -> Result<String, Unsupported> {
        let mut args = Vec::new();
        let mut head = e;
        while let Term::App(f, a) = head {
            args.push(&**a);
            head = f;
        }
        args.reverse();
        if !matches!(head, Term::Var(_)) {
            return Err(format!("a call of {}", form(head)));
        }
        let pad = format!("\n{}", " ".repeat(col + 2));
        let mut at = self.inferred(head)?;
        let mut proof = self.typing(head, &at, col + 2)?;
        for a in args {
            let (p, _, r, _) = arrow(&at).ok_or_else(|| format!("a call of `{at}`"))?;
            let (p, r) = (p.clone(), r.clone());
            let call = self.arrow_args(&at)?;
            let arg = self.typing(a, &p, col + 2)?;
            proof = format!("(.app {call}{pad}{proof}{pad}{arg}{pad}(by stage_le) (by le_live))");
            at = r;
        }
        Ok(proof)
    }
}
