//! Statements, certificates and the Lean source of a replay.

use std::fmt::Write as _;
use std::rc::Rc;

use flick_ir::Lit;

use super::{N, T, Tx, product};
use crate::logic::cert::{Cert, Rule};
use crate::logic::term::{Def, Pat, Term};
use crate::logic::{Data, Goal, Statement, TUPLES, Ty};

/// A constructor to split into: its fields' types, the names a proof gives
/// them, and the proof of its case.
type Case = (Vec<Ty>, Vec<Option<Rc<str>>>, Cert);

/// A variable of a statement as it is split: its name while a proof may
/// name it, its type in the logic while it may be split, and its `λPT` type.
#[derive(Clone)]
struct Entry {
    name: Option<Rc<str>>,
    ty: Option<Ty>,
    t: T,
}

/// The Lean source that replays `proofs`, each a statement with its
/// certificate, over the definitions `defs` in an order where each names
/// only those before it.
pub fn lean_replay(
    header: &str,
    namespace: &str,
    defs: &[(Rc<Def>, Term)],
    proofs: &[(Statement, Cert)],
    data: &Data,
) -> Result<String, String> {
    let mut tx = Tx::new(data);
    for (def, body) in defs {
        tx.global(def.clone(), body)
            .map_err(|e| format!("`{}`: {e}", def.shown()))?;
    }
    let mut items = Vec::new();
    for (s, c) in proofs {
        items.push(tx.item(s, c).map_err(|e| format!("`{}`: {e}", s.name))?);
    }
    let mut out = String::new();
    let _ = writeln!(out, "import Formal.Proofs.CheckSound\n");
    let _ = writeln!(out, "{header}\n");
    let _ = writeln!(out, "namespace {namespace}\n\nopen Flick.PT\n");
    let _ = writeln!(out, "/-- The definitions the proofs unfold, in order. -/");
    let _ = writeln!(out, "def prog : Prog := [");
    for (i, g) in tx.globals.iter().enumerate() {
        let body = tx.db(&g.body, &mut Vec::new())?;
        let scheme = tx.lean_ty(&g.scheme);
        let comma = if i + 1 < tx.globals.len() { "," } else { "" };
        let _ = writeln!(
            out,
            "  -- {}\n  ({body},\n   {scheme}){comma}",
            g.def.shown()
        );
    }
    let _ = writeln!(out, "]\n");
    let _ = writeln!(out, "/-- Each proof's statement with its certificate. -/");
    let _ = writeln!(out, "def items : List (Stmt × Cert) := [");
    for (i, (name, stmt, cert)) in items.iter().enumerate() {
        let comma = if i + 1 < items.len() { "," } else { "" };
        let _ = writeln!(out, "  -- {name}\n  ({stmt},\n   {cert}){comma}");
    }
    let _ = writeln!(out, "]\n");
    let _ = writeln!(
        out,
        "theorem prog_wf : prog.WF := Prog.wf?_sound (by decide)\n\n\
         theorem checked : checkAll prog 256 [] items = true := by decide\n\n\
         /-- Every statement holds of the definitions, at every instance of its type\n\
         variables and every values of its variables. -/\n\
         theorem sound : ∀ s ∈ items.map Prod.fst, s.Holds prog.bodies ∧ s.Typed prog.schemes :=\n  \
         checkAll_sound prog_wf items [] (by simp) checked\n\nend {namespace}"
    );
    Ok(out)
}

impl Tx<'_> {
    /// A proof as a statement and a certificate of `λPT`, in Lean.
    fn item(
        &mut self,
        stmt_in: &Statement,
        proof: &Cert,
    ) -> Result<(String, String, String), String> {
        self.rigid.clear();
        let mut env: Vec<(Rc<str>, T)> = Vec::new();
        let mut ctx: Vec<Entry> = Vec::new();
        for (param, ty) in &stmt_in.params {
            let t = self.ty(ty)?;
            env.push((param.clone(), t.clone()));
            ctx.insert(
                0,
                Entry {
                    name: Some(param.clone()),
                    ty: Some(ty.clone()),
                    t,
                },
            );
        }
        let mut facts = Vec::new();
        let mut fact_names = Vec::new();
        for (h, eq) in &stmt_in.premises {
            facts.push(self.equation(&eq.left, &eq.right, &eq.ty, &mut env)?);
            fact_names.push(h.clone());
        }
        let Goal::Equal(eq) = &stmt_in.goal else {
            return Err("a goal of types is not replayed".into());
        };
        let (lhs, rhs, goal_ty) = self.equation(&eq.left, &eq.right, &eq.ty, &mut env)?;
        let names: Vec<Rc<str>> = env.iter().map(|(n, _)| n.clone()).collect();
        let mut lean_facts = Vec::new();
        for (fl, fr, ft) in &facts {
            lean_facts.push(format!(
                "({}, {}, {})",
                self.db(fl, &mut names.clone())?,
                self.db(fr, &mut names.clone())?,
                self.lean_ty(ft)
            ));
        }
        let ctx_types: Vec<String> = ctx.iter().map(|e| self.lean_ty(&e.t)).collect();
        let stmt = format!(
            "{{ ctx := [{}],\n     facts := [{}],\n     lhs := {},\n     rhs := {},\n     ty := {} }}",
            ctx_types.join(", "),
            lean_facts.join(", "),
            self.db(&lhs, &mut names.clone())?,
            self.db(&rhs, &mut names.clone())?,
            self.lean_ty(&goal_ty)
        );
        let cert = self.cert(proof, &mut ctx, &fact_names)?;
        Ok((stmt_in.name.clone(), stmt, cert))
    }

    fn equation(
        &mut self,
        l: &Term,
        r: &Term,
        ty: &Ty,
        env: &mut Vec<(Rc<str>, T)>,
    ) -> Result<(N, N, T), String> {
        let t = self.ty(ty)?;
        let (nl, tl) = self.term(l, env)?;
        let (nr, tr) = self.term(r, env)?;
        self.unify(&tl, &t)?;
        self.unify(&tr, &t)?;
        Ok((nl, nr, t))
    }

    fn cert(
        &mut self,
        c: &Cert,
        ctx: &mut Vec<Entry>,
        facts: &[Rc<str>],
    ) -> Result<String, String> {
        Ok(match &c.kind {
            Rule::Hole(_) => return Err("an incomplete certificate cannot be replayed".into()),
            Rule::Refl => ".refl".into(),
            Rule::Symm(p) => format!("(.symm {})", self.cert(p, ctx, facts)?),
            Rule::Trans(m, p, q) => {
                let mut env: Vec<(Rc<str>, T)> = (ctx.iter().rev())
                    .filter_map(|e| Some((e.name.clone()?, e.t.clone())))
                    .collect();
                let (nm, _) = self.term(m, &mut env)?;
                let m = self.db(&nm, &mut names_of(ctx))?;
                format!(
                    "(.trans {m} {} {})",
                    self.cert(p, ctx, facts)?,
                    self.cert(q, ctx, facts)?
                )
            }
            Rule::Fact(h) => {
                let i = (facts.iter())
                    .position(|f| f == h)
                    .ok_or("a fact the replay has no premise for")?;
                format!("(.fact {i})")
            }
            Rule::Case(x, arms) => self.case(x, arms, ctx, facts)?,
            Rule::Cong(..) | Rule::Funext(..) | Rule::Let(..) | Rule::Lemma(..) => {
                return Err("its certificate uses a rule the replay does not translate yet".into());
            }
        })
    }

    /// A split of `x` by its constructors, then of each one's fields.
    fn case(
        &mut self,
        x: &str,
        arms: &[(Pat, Cert)],
        ctx: &mut Vec<Entry>,
        facts: &[Rc<str>],
    ) -> Result<String, String> {
        let idx = (ctx.iter())
            .position(|e| e.name.as_deref() == Some(x))
            .ok_or_else(|| format!("`{x}` is no variable a proof splits"))?;
        let ty = ctx[idx]
            .ty
            .clone()
            .ok_or("a variable without a type of the logic")?;
        let ctors = self.logic_ctors(&ty)?;
        let mut chosen = Vec::new();
        for (name, fields) in &ctors {
            let arm = (arms.iter()).find(|(p, _)| match p {
                Pat::Con(c, _) => c.name == *name,
                Pat::Lit(l) => {
                    matches!(l.0, Lit::Bool(b) if (if b { "true" } else { "false" }) == name)
                }
                Pat::Wild | Pat::Bind(_) => true,
            });
            let (pat, cert) = arm.ok_or_else(|| format!("no case for `{name}`"))?;
            let binders: Vec<Option<Rc<str>>> = match pat {
                Pat::Con(_, subs) => subs
                    .iter()
                    .map(|p| match p {
                        Pat::Bind(y) => Ok(Some(y.clone())),
                        Pat::Wild => Ok(None),
                        _ => Err("a split's fields are names".to_owned()),
                    })
                    .collect::<Result<_, _>>()?,
                _ => vec![None; fields.len()],
            };
            chosen.push((fields.clone(), binders, cert.clone()));
        }
        // the variable is now its case: no proof names it
        let saved = ctx[idx].name.take();
        let out = self.split_sum(idx, &chosen, ctx, facts);
        ctx[idx].name = saved;
        out
    }

    /// The constructors of a type of the logic, with their fields' types.
    fn logic_ctors(&mut self, ty: &Ty) -> Result<Vec<(String, Vec<Ty>)>, String> {
        match ty {
            Ty::Bool => Ok(vec![
                ("false".into(), Vec::new()),
                ("true".into(), Vec::new()),
            ]),
            Ty::Data { module, args, .. } if module == TUPLES => {
                Ok(vec![("(,)".into(), args.clone())])
            }
            Ty::Data { module, name, args } => {
                let key = (module.clone(), name.clone());
                let decl = self
                    .data
                    .get(&key)
                    .ok_or("a data type the replay has no declaration of")?;
                Ok((decl.constructors.iter())
                    .map(|(c, fs)| {
                        (
                            c.clone(),
                            fs.iter()
                                .map(|f| f.instantiate(&decl.params, args))
                                .collect(),
                        )
                    })
                    .collect())
            }
            _ => Err("a split of a variable that is no data type".into()),
        }
    }

    fn split_sum(
        &mut self,
        v: usize,
        cases: &[Case],
        ctx: &mut Vec<Entry>,
        facts: &[Rc<str>],
    ) -> Result<String, String> {
        let [(fields, binders, cert), rest @ ..] = cases else {
            return Err("a data type without constructors".into());
        };
        if rest.is_empty() {
            return self.split_fields(v, fields, binders, cert, ctx, facts);
        }
        let payload = self.payload(fields)?;
        ctx.insert(
            0,
            Entry {
                name: None,
                ty: None,
                t: payload,
            },
        );
        let left = self.split_fields(0, fields, binders, cert, ctx, facts);
        ctx.remove(0);
        let rest_ty = self.payload_sum(rest)?;
        ctx.insert(
            0,
            Entry {
                name: None,
                ty: None,
                t: rest_ty,
            },
        );
        let right = self.split_sum(0, rest, ctx, facts);
        ctx.remove(0);
        Ok(format!("(.split {v} {} {})", left?, right?))
    }

    fn payload(&mut self, fields: &[Ty]) -> Result<T, String> {
        let ts: Vec<T> = fields
            .iter()
            .map(|f| self.ty(f))
            .collect::<Result<_, _>>()?;
        Ok(product(&ts))
    }

    fn payload_sum(&mut self, cases: &[Case]) -> Result<T, String> {
        let ts: Vec<Vec<T>> = (cases.iter())
            .map(|(fs, ..)| fs.iter().map(|f| self.ty(f)).collect::<Result<_, _>>())
            .collect::<Result<_, _>>()?;
        Ok(super::sum(&ts))
    }

    /// The fields of variable `v`, a product, as variables named by `binders`.
    fn split_fields(
        &mut self,
        v: usize,
        fields: &[Ty],
        binders: &[Option<Rc<str>>],
        cert: &Cert,
        ctx: &mut Vec<Entry>,
        facts: &[Rc<str>],
    ) -> Result<String, String> {
        match fields {
            [] => Ok(format!(
                "(.split {v} {} .refl)",
                self.cert(cert, ctx, facts)?
            )),
            [f] => {
                let saved = ctx[v].clone();
                ctx[v] = Entry {
                    name: binders.first().cloned().flatten(),
                    ty: Some(f.clone()),
                    t: self.ty(f)?,
                };
                let out = self.cert(cert, ctx, facts);
                ctx[v] = saved;
                out
            }
            [f, rest @ ..] => {
                let first = Entry {
                    name: binders.first().cloned().flatten(),
                    ty: Some(f.clone()),
                    t: self.ty(f)?,
                };
                let second_t = self.payload(rest)?;
                ctx.insert(0, first);
                ctx.insert(
                    0,
                    Entry {
                        name: None,
                        ty: None,
                        t: second_t,
                    },
                );
                let inner =
                    self.split_fields(0, rest, binders.get(1..).unwrap_or(&[]), cert, ctx, facts);
                ctx.drain(..2);
                Ok(format!("(.split {v} {} .refl)", inner?))
            }
        }
    }

    fn lean_ty(&self, t: &T) -> String {
        match self.zonk(t) {
            T::Var(i) => format!("(.var {i})"),
            T::Meta(_) | T::Unit => ".unit".into(),
            T::Prod(a, b) => format!("(.prod {} {})", self.lean_ty(&a), self.lean_ty(&b)),
            T::Sum(a, b) => format!("(.sum {} {})", self.lean_ty(&a), self.lean_ty(&b)),
            T::Arrow(a, b) => format!("(.arrow {} {})", self.lean_ty(&a), self.lean_ty(&b)),
        }
    }

    /// `n` in de Bruijn form, the names of `scope` bound, innermost last.
    fn db(&self, term: &N, scope: &mut Vec<Rc<str>>) -> Result<String, String> {
        let under = |binder: &Rc<str>, body: &N, scope: &mut Vec<Rc<str>>| {
            scope.push(binder.clone());
            let out = self.db(body, scope);
            scope.pop();
            out
        };
        Ok(match term {
            N::Var(x) => {
                let i = (scope.iter().rev())
                    .position(|y| y == x)
                    .ok_or_else(|| format!("`{x}` is unbound in the replay"))?;
                format!("(.var {i})")
            }
            N::Glob(g, ts) => {
                let ts: Vec<String> = ts.iter().map(|t| self.lean_ty(t)).collect();
                format!("(.glob {g} [{}])", ts.join(", "))
            }
            N::Lam(x, t, b) => format!("(.lam {} {})", self.lean_ty(t), under(x, b, scope)?),
            N::App(f, x) => format!("(.app {} {})", self.db(f, scope)?, self.db(x, scope)?),
            N::Unit => ".unit".into(),
            N::Pair(a, b) => format!("(.pair {} {})", self.db(a, scope)?, self.db(b, scope)?),
            N::Split(e, x, y, b) => {
                let e = self.db(e, scope)?;
                scope.push(x.clone());
                let body = under(y, b, scope);
                scope.pop();
                format!("(.split {e} {})", body?)
            }
            N::Inl(x, t) => format!("(.inl {} {})", self.db(x, scope)?, self.lean_ty(t)),
            N::Inr(t, x) => format!("(.inr {} {})", self.lean_ty(t), self.db(x, scope)?),
            N::Case(e, l, lb, r, rb) => format!(
                "(.case {} {} {})",
                self.db(e, scope)?,
                under(l, lb, scope)?,
                under(r, rb, scope)?
            ),
            N::Let(x, e, b) => format!("(.let_ {} {})", self.db(e, scope)?, under(x, b, scope)?),
        })
    }
}

/// The names of a statement's variables in scope order, outermost first;
/// a variable a split replaced has none.
fn names_of(ctx: &[Entry]) -> Vec<Rc<str>> {
    (ctx.iter().rev())
        .map(|e| e.name.clone().unwrap_or_else(|| "%dead".into()))
        .collect()
}
