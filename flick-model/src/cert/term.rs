//! The proof of one typing: the rules of `formal/Formal/Rules.lean` on the
//! terms the generator makes, with the types of the trace.

use crate::check::Inst;
use std::collections::HashMap;
use std::iter;
use std::rc::Rc;

use crate::check::trace::{Node, Trace, key};
use crate::syntax::{Const, Head, Lit, Name, Term};
use crate::translate::lean::elide::{Env, caps, row, stage, ty};
use crate::types::{Caps, Fields, Row, Scheme, Stage, Type, Var};

use super::Unsupported;
use super::list::list_tm;
use super::pat::{arms_tm, match_tm};
use super::provide::{operation, provide_tm};
use super::raise::is_raise;
use super::sig::{Key, Sig, bare, kept, prim_key};
use crate::check::substitute;

/// The term `e` in the core, its variables by de Bruijn index (`ctx`
/// innermost first).
pub(super) fn tm(e: &Term, ctx: &[Name], sig: &Sig) -> Result<String, Unsupported> {
    let tm = |e: &Term, ctx: &[Name]| tm(e, ctx, sig);
    Ok(match e {
        Term::Var(x) => format!("(.var {})", index(ctx, x)?),
        Term::Global(p) => format!("(.glob {})", global(sig, &Key::Value(p.clone()))?.0),
        Term::Apply(Head::Const(Const::Prim(p)), xs) if xs.is_empty() => {
            format!("(.glob {})", global(sig, &prim_key(*p))?.0)
        }
        Term::Apply(Head::Con(c), xs) => {
            let k = c.ty.module.to_string() + "." + &c.name;
            xs.iter().try_fold(format!("(.ctor {k:?})"), |f, x| {
                Ok::<_, Unsupported>(format!("(.app {f} {})", tm(x, ctx)?))
            })?
        }
        Term::Tuple(xs) => {
            let mut xs = xs.iter();
            let first = tm(xs.next().ok_or("an empty tuple")?, ctx)?;
            xs.try_fold(first, |a, b| {
                Ok::<_, Unsupported>(format!(
                    "(.app (.app (.ctor \"Tuple\") {a}) {})",
                    tm(b, ctx)?
                ))
            })?
        }
        Term::Lit(l) => format!("(.lit {})", lit(l)?),
        Term::App(f, a) if is_raise(f) => format!("(.raise {})", tm(a, ctx)?),
        Term::Try(b, arms) => format!("(.try_ {} {})", tm(b, ctx)?, arms_tm(arms, ctx, sig)?),
        Term::Lam(x, b) => format!("(.lam {})", tm(b, &push(ctx, x))?),
        Term::App(f, a) => format!("(.app {} {})", tm(f, ctx)?, tm(a, ctx)?),
        Term::Let(x, _, e1, e2) => format!("(.let_ {} {})", tm(e1, ctx)?, tm(e2, &push(ctx, x))?),
        Term::LetRec(f, _, lam, e2) => {
            let Term::Lam(x, body) = &**lam else {
                return Err("a `let rec` of no λ".into());
            };
            let inner = push(&push(ctx, f), x);
            format!(
                "(.let_ (.fix {}) {})",
                tm(body, &inner)?,
                tm(e2, &push(ctx, f))?
            )
        }
        Term::Task(b) => format!("(.task {})", tm(b, ctx)?),
        Term::Await(x) => format!("(.await {})", tm(x, ctx)?),
        // a record of the core is a spine in the order of its labels, as
        // its type is written
        Term::List(xs) => list_tm(xs, ctx, sig)?,
        Term::Record(fs) => {
            let mut out = ".rnil".to_owned();
            for (l, x) in sorted(fs).into_iter().rev() {
                out = format!("(.rcons \"{l}\" {} {out})", tm(x, ctx)?);
            }
            out
        }
        Term::Field(..) if operation(e).is_some() => {
            let (svc, o) = operation(e).ok_or("no operation")?;
            format!("(.const (.perform {svc:?} {:?} .tail))", o.to_string())
        }
        Term::Provide(l, b) => provide_tm(l, b, ctx, sig)?,
        Term::Field(r, l) => format!("(.proj {} \"{l}\")", tm(r, ctx)?),
        Term::Update(r, l, v) => format!("(.update {} \"{l}\" {})", tm(r, ctx)?, tm(v, ctx)?),
        Term::Match(s, arms) => match_tm(s, arms, ctx, sig)?,
        other => return Err(form(other)),
    })
}

/// The fields of a record term by label.
pub(super) fn sorted(fs: &[(Name, Term)]) -> Vec<(&Name, &Term)> {
    let mut fs: Vec<_> = fs.iter().map(|(l, x)| (l, x)).collect();
    fs.sort_by(|a, b| a.0.cmp(b.0));
    fs
}

/// The type of field `l` of a record type.
pub(super) fn field<'a>(t: &'a Type, l: &str) -> Result<&'a Type, Unsupported> {
    let f = fields(t).ok_or("a field of no record")?;
    f.fields
        .get(l)
        .ok_or_else(|| format!("a field `{l}` the type does not have"))
}

/// The fields of a record type.
pub(super) fn fields(t: &Type) -> Option<&Fields> {
    match t {
        // a rest the check left open is empty (`Env::ground`)
        Type::Record(f) if matches!(f.tail, None | Some(Var::Meta(_))) => Some(f),
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

pub(super) fn index(ctx: &[Name], x: &str) -> Result<usize, Unsupported> {
    ctx.iter()
        .position(|y| &**y == x)
        .ok_or_else(|| format!("`{x}` is not bound"))
}

pub(super) fn lit(l: &Lit) -> Result<String, Unsupported> {
    Ok(match l {
        Lit::Unit => ".unit".into(),
        Lit::Bool(b) => format!("(.bool {b})"),
        Lit::Int(n) if *n < 0 => format!("(.int ({n}))"),
        Lit::Int(n) => format!("(.int {n})"),
        Lit::String(s) => format!("(.str {s:?})"),
        Lit::Float(f) => return Err(format!("the literal {f}")),
    })
}

/// The arrows of a constructor's scheme without the qualifiers of its
/// fields and value, which the core's data has not.
fn plain(t: &Type) -> Type {
    match bare(t) {
        Type::Fun(p, s, r) => Type::Fun(Rc::new(bare(p).clone()), s.clone(), Rc::new(plain(r))),
        t => t.clone(),
    }
}

/// The index and scheme of a global of the signature.
fn global<'s>(sig: &'s Sig, key: &Key) -> Result<(usize, &'s Scheme), Unsupported> {
    sig.glob(key)
        .ok_or_else(|| format!("the global {key:?}, not in the signature"))
}

/// What kind of term the certificates do not write yet.
fn form(e: &Term) -> Unsupported {
    let first = |s: String| s.split(['(', ' ', '{']).next().unwrap_or("").to_owned();
    match e {
        Term::Apply(Head::Const(c), _) => format!("the constant {}", first(format!("{c:?}"))),
        e => format!("the form {}", first(format!("{e:?}"))),
    }
}

/// The arrow of `t`: its parameter, stage, result and qualifier.
pub(super) fn arrow(t: &Type) -> Option<(&Type, &Stage, &Type, Caps)> {
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
    pub(super) sig: &'t Sig,
    /// The term variables, innermost first.
    pub(super) terms: Vec<Name>,
    /// The type variables, innermost first.
    pub(super) env: Env,
    /// Where in `terms`, from the outermost, a `fix` binds itself, with the
    /// rigid variables of its signature.
    pub(super) fixed: Vec<(usize, Vec<Name>)>,
}

impl<'t> Cx<'t> {
    pub(super) fn new(trace: &'t Trace, sig: &'t Sig) -> Self {
        Cx {
            trace,
            sig,
            terms: Vec::new(),
            env: Env {
                ground: true,
                classes: sig.classes.clone(),
                ..Env::default()
            },
            fixed: Vec::new(),
        }
    }

    pub(super) fn node(&self, e: &Term) -> Result<&'t Node, Unsupported> {
        self.trace
            .get(&key(e))
            .ok_or_else(|| "a term the check did not type".to_owned())
    }

    pub(super) fn ty(&self, t: &Type) -> Result<String, Unsupported> {
        ty(t, &self.env)
    }

    pub(super) fn arrow_args(&self, t: &Type) -> Result<String, Unsupported> {
        self.arrow_named(t, "t")
    }

    /// The implicit arguments of a rule at the arrow `t`, its result named
    /// `result`.
    pub(super) fn arrow_named(&self, t: &Type, result: &str) -> Result<String, Unsupported> {
        let (p, s, r, c) = arrow(t).ok_or_else(|| format!("`{t}` is no arrow"))?;
        Ok(format!(
            "(a := {}) (ψ := {}) ({result} := {}) (C := {})",
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

    pub(super) fn inst(&self, i: &Inst) -> Result<String, Unsupported> {
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
            Term::Global(_) | Term::Apply(Head::Const(Const::Prim(_)), _) => self.glob(e),

            Term::Lam(x, body) => {
                let (_, _, r, _) = arrow(t).ok_or_else(|| format!("a λ at `{t}`"))?;
                let args = self.arrow_args(t)?;
                let b = self.under(x, |c| c.typing(body, r, col + 2))?;
                Ok(format!(
                    "(.lam {args}{pad}{b}{pad}(by simp [Tm.NoIns, Tm.Has]) (by le_live))"
                ))
            }
            Term::App(f, a) if is_raise(f) => self.raise(a, t, col),
            Term::Try(body, arms) => self.try_(body, arms, t, col),
            Term::App(..) | Term::Apply(Head::Con(_), _) | Term::Tuple(_) => self.app(e, col),
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
                self.await_(x, &at, col)
            }
            Term::Provide(l, b) => self.provide(l, b, t, col),
            Term::Field(..) if operation(e).is_some() => Err("an operation not called".into()),
            Term::Record(fs) => self.record(fs, t, col),
            Term::List(xs) => self.list(xs, t, col),
            Term::Field(r, l) => self.field(e, r, l, col),
            Term::Update(r, l, v) => self.update(r, l, v, t, col),
            Term::Let(x, _, e1, e2) => self.let_(x, e1, e2, t, false, col),
            Term::LetRec(f, _, lam, e2) => self.let_(f, lam, e2, t, true, col),
            Term::Match(s, arms) => self.match_(s, arms, t, col),
            other => Err(form(other)),
        }
    }

    /// (Await) of `x` at the computation `at`.
    pub(super) fn await_(
        &mut self,
        x: &Term,
        at: &Type,
        col: usize,
    ) -> Result<String, Unsupported> {
        let pad = format!("\n{}", " ".repeat(col + 2));
        let args = self.eff_args(at)?;
        let x = self.typing(x, at, col + 2)?;
        Ok(format!(
            "(.await {args}{pad}{x}{pad}(by stage_le) (by le_live))"
        ))
    }

    pub(super) fn inferred(&self, e: &Term) -> Result<Type, Unsupported> {
        self.node(e)?
            .inferred
            .clone()
            .ok_or_else(|| "a term without its type".into())
    }

    /// `check` with `x` bound, innermost.
    pub(super) fn under<T>(&mut self, x: &Name, check: impl FnOnce(&mut Self) -> T) -> T {
        self.terms.insert(0, x.clone());
        let out = check(self);
        self.terms.remove(0);
        out
    }

    /// The instance a variable, global or constructor took, its types.
    fn instance(&self, e: &Term) -> Result<(&'t Scheme, Vec<String>), Unsupported> {
        let (scheme, insts) = self
            .node(e)?
            .instance
            .as_ref()
            .ok_or("a term without its instance")?;
        if !scheme.outlives.is_empty() {
            return Err("a scheme with facts".into());
        }
        let ts = insts
            .iter()
            .map(|i| self.inst(i))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((scheme, ts))
    }

    /// (Glob) of a global or an operator at the instance the check took.
    fn glob(&mut self, e: &Term) -> Result<String, Unsupported> {
        let key = match e {
            Term::Global(p) => Key::Value(p.clone()),
            Term::Apply(Head::Const(Const::Prim(p)), _) => prim_key(*p),
            other => return Err(form(other)),
        };
        let (g, _) = global(self.sig, &key)?;
        let (_, ts) = self.instance(e)?;
        let instance = self.ty(&self.inferred(e)?)?;
        Ok(format!(
            "(Cert.typ_glob (ts := [{}]) (T' := {instance}) ((hS.glob {g} rfl).trans rfl) rfl (by holds) (by inst_eq) (by le_live))",
            ts.join(", ")
        ))
    }

    /// (Con) of the ordinary constructor `k` at the instance `us`.
    fn ctor(&self, k: &str, us: &[String]) -> Result<String, Unsupported> {
        if !self.sig.ctors.contains_key(k) {
            return Err(format!("the constructor `{k}`, not in the signature"));
        }
        Ok(format!(
            "(Typ.ctor_plain S (K := {k:?}) (us := [{}]) ((hS.ctor _ rfl).trans rfl) ((hS.res _ rfl).trans rfl) rfl LeLive.refl)",
            us.join(", ")
        ))
    }

    /// The head of a call, its proof and type: a constructor and a tuple
    /// take their fields as arguments, before those of the call.
    fn head<'e>(
        &mut self,
        head: &'e Term,
        args: &mut Vec<&'e Term>,
        col: usize,
    ) -> Result<(String, Type), Unsupported> {
        match head {
            Term::Var(_) | Term::Global(_) | Term::Apply(Head::Const(Const::Prim(_)), _) => {
                let at = self.inferred(head)?;
                Ok((self.typing(head, &at, col)?, at))
            }
            Term::Apply(Head::Con(c), given) => {
                args.splice(0..0, given.iter());
                let (scheme, insts) = self
                    .node(head)?
                    .instance
                    .as_ref()
                    .ok_or("a constructor without its instance")?;
                let map: HashMap<Name, Inst> = scheme
                    .vars
                    .iter()
                    .map(|(v, _)| v.clone())
                    .zip(insts.iter().cloned())
                    .collect();
                let at = substitute(&plain(&scheme.body), &map);
                let us = kept(scheme)
                    .into_iter()
                    .zip(insts)
                    .filter(|(k, _)| *k)
                    .map(|(_, i)| self.inst(i))
                    .collect::<Result<Vec<_>, _>>()?;
                let k = format!("{}.{}", c.ty.module, c.name);
                Ok((self.ctor(&k, &us)?, at))
            }
            h @ Term::Field(..) if operation(h).is_some() => self.operation(h),
            Term::Field(d, l) if self.is_dict(d) => {
                let at = self.inferred(head)?;
                Ok((self.dproj(head, d, l, col)?, at))
            }
            Term::Tuple(xs) => {
                let [x, y] = &xs[..] else {
                    return Err("a tuple of more than two".into());
                };
                args.splice(0..0, [x, y]);
                let (tx, ty_) = (self.inferred(x)?, self.inferred(y)?);
                let at = Type::fun(
                    tx.clone(),
                    Stage::Pure,
                    Type::fun(
                        ty_.clone(),
                        Stage::Pure,
                        Type::Tuple(Rc::from([tx.clone(), ty_.clone()])),
                    ),
                );
                let us = [self.ty(&tx)?, self.ty(&ty_)?];
                Ok((self.ctor("Tuple", &us)?, at))
            }
            other => Err(format!("a call of {}", form(other))),
        }
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
        let pad = format!("\n{}", " ".repeat(col + 2));
        let (mut proof, mut at) = self.head(head, &mut args, col + 2)?;
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
