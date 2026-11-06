//! The rules of `docs/formal/core.md` §2.3 that are not handlers: (Var),
//! (Lam), (App), the `let`s, records and fields, with (Sub) where an
//! expression meets the type it is checked against. Γ₀ (§1.5) is
//! `constant`.

use std::collections::BTreeMap;
use std::mem;
use std::rc::Rc;

use super::{Check, Checker, error};
use crate::syntax::{Head, Lit, Name, Path, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

pub(super) fn prelude(name: &str) -> Type {
    Type::con("Prelude", name)
}

pub(super) fn list(t: Type) -> Type {
    Type::app(prelude("List"), t)
}

pub(super) fn var(n: &str) -> Type {
    Type::Var(Var::Named(n.into()))
}

pub(super) fn scheme(vars: &[(&str, Kind)], body: Type) -> Scheme {
    Scheme {
        vars: vars
            .iter()
            .map(|(n, k)| (Name::from(*n), k.clone()))
            .collect(),
        body,
        outlives: Vec::new(),
    }
}

pub(super) fn runtime(r: Row, e: Row) -> Stage {
    Stage::Runtime(r, e)
}

impl Checker<'_> {
    /// `Γ ⊢ e : τ ! φ`, `φ` joined into `self.stage`.
    pub(super) fn infer(&mut self, e: &Term) -> Check<Type> {
        match e {
            Term::Var(x) => self.var(x),
            Term::Global(p) => self.global(p),
            Term::Lit(l) => Ok(lit(l)),
            Term::Lam(x, body) => self.lam(x, body),
            Term::App(f, a) => self.app(f, a),
            Term::Let(x, sig, e1, e2) => self.let_(x, sig.as_deref(), e1, e2, None),
            Term::LetRec(f, sig, lam, e2) => self.let_rec(f, sig.as_deref(), lam, e2, None),
            Term::Match(e, arms) => self.match_(e, arms, None),
            Term::Tuple(xs) => Ok(Type::Tuple(
                xs.iter().map(|x| self.infer(x)).collect::<Check<_>>()?,
            )),
            Term::List(xs) => {
                let t = self.fresh();
                for x in xs.iter() {
                    self.check(x, &t)?;
                }
                Ok(list(t))
            }
            Term::Record(fields) => self.record(fields),
            Term::Field(e, l) => {
                let t = self.infer(e)?;
                self.field(&t, l)
            }
            Term::Update(e, l, v) => {
                let t = self.infer(e)?;
                let f = self.field(&t, l)?;
                self.check(v, &f)?;
                Ok(t)
            }
            Term::Task(body) => self.task(body, None),
            Term::Await(e) => self.await_(e),
            Term::Try(body, arms) => self.try_(body, arms, None),
            Term::Provide(l, body) => self.provide(l, body, None),
            Term::Handler(h) => self.handler(h, None),
            Term::Apply(Head::Con(c), args) if args.len() == c.arity => self.construct(c, args),
            Term::Apply(head, args) => {
                let t = match head {
                    Head::Con(c) => self.constructor(&c.ty.module, &c.name)?,
                    Head::Const(c) => self.constant(c)?,
                };
                args.iter().try_fold(t, |t, a| self.apply(&t, a))
            }
            Term::Scope(s, body) => self.scope(s, body, None),
            Term::Tag(s) => Ok(Type::Tag(Row::one(s.clone()))),
            Term::Structural(class, fields) => {
                let t = self.fresh();
                let d = Type::Dict(Path::new("Prelude", class), Rc::from([t]));
                self.structural
                    .push((class.clone(), fields.clone(), d.clone()));
                Ok(d)
            }
            Term::Stuck(why) => error("Translation", why.to_string()),
            Term::Die(_) => Ok(self.fresh()),
            other => self.runtime_form(other, None),
        }
    }

    /// `Γ ⊢ e : τ'` with `τ ≤ τ'` (Sub): the expected type is pushed into
    /// λs, `let` bodies and arms, where it fixes stages and parameters.
    pub(super) fn check(&mut self, e: &Term, expected: &Type) -> Check<()> {
        let full = self.follow(expected);
        let expected = self.resolve(&full);
        match (e, &expected) {
            (_, Type::Forall(s)) => {
                let qualifier = self.qualifier(&full);
                self.check_qualified_forall(e, s, qualifier)
            }
            // (Lam) against `(τ₁ →φ τ₂)^C` takes `C`, against a bare arrow `∅`
            (Term::Lam(x, body), Type::Fun(p, s, r)) => {
                let outer = mem::replace(&mut self.stage, Stage::Pure);
                self.locals.push((x.clone(), mono((**p).clone())));
                let checked = self.check(body, r);
                self.locals.pop();
                let inner = mem::replace(&mut self.stage, outer);
                checked?;
                self.stage_within(&inner, s)?;
                let qualifier = self.qualifier(&full);
                self.close_over(e, expected.clone(), Some(qualifier));
                Ok(())
            }
            (Term::Scope(s, body), _) => self.scope(s, body, Some(&full)).map(drop),
            (Term::Apply(Head::Con(c), args), _) if args.len() == c.arity => {
                let t = self.construct(c, args)?;
                self.subsume(&t, &full)
            }
            (Term::Let(x, sig, e1, e2), _) => {
                self.let_(x, sig.as_deref(), e1, e2, Some(&full)).map(drop)
            }
            (Term::LetRec(f, sig, lam, e2), _) => self
                .let_rec(f, sig.as_deref(), lam, e2, Some(&full))
                .map(drop),
            (Term::Match(e, arms), _) => self.match_(e, arms, Some(&full)).map(drop),
            (Term::Try(body, arms), _) => self.try_(body, arms, Some(&full)).map(drop),
            (Term::Provide(l, body), _) => self.provide(l, body, Some(&full)).map(drop),
            (Term::Handler(h), _) => self.handler(h, Some(&full)).map(drop),
            (Term::Task(body), Type::Eff(..)) => self.task(body, Some(&full)).map(drop),
            (Term::Record(fields), Type::Record(f)) if f.tail.is_none() => {
                self.record_against(fields, f)
            }
            (Term::Record(fields), Type::Impl(r)) => {
                let f = self.implementation(r)?;
                self.record_against(fields, &f)
            }
            (Term::Record(fields), Type::Dict(c, args)) => self.dictionary(fields, c, args),
            (Term::With(..) | Term::Handle(..) | Term::Inside(..), _) => {
                self.runtime_form(e, Some(&full)).map(drop)
            }
            (Term::App(f, a), _) if self.config && matches!(&**f, Term::Lam(..)) => {
                let Term::Lam(x, body) = &**f else {
                    unreachable!("a λ, matched above")
                };
                self.redex(x, body, a, Some(&full)).map(drop)
            }
            // `bind_S (await v)` of a configuration: what the layer builds is
            // checked against `S.Impl`, as `Layer.effect` checked it; the
            // task may hold what it likes
            (Term::Await(x), _) if self.config => {
                let (r, x_rows) = (self.fresh_row(), self.fresh_row());
                let task = Type::Eff(r.clone(), x_rows.clone(), Rc::new(full));
                let held = self.fresh_caps();
                self.check(x, &Type::Captured(Rc::new(task), held))?;
                self.perform(&Stage::Runtime(r, x_rows))
            }
            _ => {
                let t = self.infer(e)?;
                self.subsume(&t, &full)
            }
        }
    }

    /// (Let-Sig) with `σ` a `∀`: the value is checked with rigid variables,
    /// and a runtime step while it is built is `FLICK-T013` (`docs/formal/core.md#an-annotation-with-forall-bypasses-let-sig`).
    pub(super) fn check_forall(&mut self, e: &Term, s: &Scheme) -> Check<()> {
        self.check_qualified_forall(e, s, Caps::default())
    }

    /// (Let-Sig) against `(∀ᾱ. τ)^C`: the value has `τ^C`.
    fn check_qualified_forall(&mut self, e: &Term, s: &Scheme, qualifier: Caps) -> Check<()> {
        let (facts, givens) = (self.regions.facts.len(), self.givens());
        let mut body = self.skolemize(s);
        if !qualifier.is_empty() {
            body = Type::Captured(Rc::new(body), qualifier);
        }
        let outer = mem::replace(&mut self.stage, Stage::Pure);
        let checked = self.check(e, &body);
        let inner = mem::replace(&mut self.stage, outer);
        self.regions.facts.truncate(facts);
        self.forget_givens(givens);
        checked?;
        if !s.vars.is_empty() && inner != Stage::Pure {
            return error(
                "Let-Sig",
                format!(
                    "a runtime step `{}` builds a polymorphic value",
                    self.zonk_stage(&inner)
                ),
            );
        }
        self.perform(&inner)
    }

    /// (Var).
    fn var(&mut self, x: &str) -> Check<Type> {
        match self.locals.iter().rev().find(|(n, _)| &**n == x) {
            Some((_, s)) => {
                let s = s.clone();
                Ok(self.instantiate(&s))
            }
            None => error("Var", format!("`{x}` is not bound")),
        }
    }

    /// (Var) on the top-level values of §2.6 and Γ₀.
    pub(super) fn global(&mut self, p: &Path) -> Check<Type> {
        let s = self
            .decls
            .values
            .get(p)
            .or_else(|| self.decls.instances.get(p));
        match s {
            Some(s) => {
                let s = s.clone();
                Ok(self.instantiate(&s))
            }
            None => error("Var", format!("no type for `{p}`")),
        }
    }

    pub(super) fn constructor(&mut self, module: &str, name: &str) -> Check<Type> {
        match self.decls.constructors.get(&Path::new(module, name)) {
            Some(s) => {
                let s = s.clone();
                Ok(self.instantiate(&s))
            }
            None => error("Var", format!("no constructor `{module}.{name}`")),
        }
    }

    /// (Lam): without an expected type the arrow has the least stage, that
    /// of the body.
    fn lam(&mut self, x: &Name, body: &Term) -> Check<Type> {
        let p = self.fresh();
        let outer = mem::replace(&mut self.stage, Stage::Pure);
        self.locals.push((x.clone(), mono(p.clone())));
        let r = self.infer(body);
        self.locals.pop();
        let inner = mem::replace(&mut self.stage, outer);
        let lam = Term::Lam(x.clone(), Rc::new(body.clone()));
        Ok(self.close_over(&lam, Type::fun(p, inner, r?), None))
    }

    /// (App): `φ₁ ⊔ φ₂ ⊔ φ`. A callee of unknown type is a pure arrow (`docs/formal/core.md#a-callee-of-unknown-type-is-a-pure-arrow`).
    /// A constructor with all its fields is (Con).
    fn app(&mut self, f: &Term, a: &Term) -> Check<Type> {
        if let (true, Term::Lam(x, body)) = (self.config, f) {
            return self.redex(x, body, a, None);
        }
        let mut args = vec![a.clone()];
        let mut head = f;
        while let Term::App(g, b) = head {
            args.push((**b).clone());
            head = g;
        }
        if let Term::Apply(Head::Con(c), given) = head
            && given.len() + args.len() == c.arity
        {
            args.reverse();
            let all: Vec<Term> = given.iter().cloned().chain(args).collect();
            return self.construct(c, &all);
        }
        let t = self.infer(f)?;
        self.apply(&t, a)
    }

    pub(super) fn apply(&mut self, f: &Type, a: &Term) -> Check<Type> {
        match self.resolve(f) {
            Type::Fun(p, s, r) => {
                self.check(a, &p)?;
                self.perform(&s)?;
                Ok((*r).clone())
            }
            Type::Forall(s) => {
                let t = self.instantiate(&s);
                self.apply(&t, a)
            }
            t @ Type::Var(Var::Meta(_)) => {
                let (p, r) = (self.fresh(), self.fresh());
                self.unify(&t, &Type::fun(p.clone(), Stage::Pure, r.clone()))?;
                self.check(a, &p)?;
                Ok(r)
            }
            other => error(
                "App",
                format!(
                    "this is not a function: it has type `{}`",
                    self.zonk(&other)
                ),
            ),
        }
    }

    /// (Let-Gen), (Let-Mono) and (Let-Sig).
    fn let_(
        &mut self,
        x: &Name,
        sig: Option<&Scheme>,
        e1: &Term,
        e2: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let s = if let Some(s) = sig {
            let s = self.local_signature(s);
            let (bare, qualifier) = match &s.body {
                Type::Captured(t, c) => ((**t).clone(), c.clone()),
                t => (t.clone(), Caps::default()),
            };
            let bare = Scheme {
                body: bare,
                ..s.clone()
            };
            self.check_qualified_forall(e1, &bare, qualifier)?;
            s
        } else {
            self.level += 1;
            let outer = mem::replace(&mut self.stage, Stage::Pure);
            let t1 = self.infer(e1);
            let inner = mem::replace(&mut self.stage, outer);
            self.level -= 1;
            let t1 = t1?;
            if inner == Stage::Pure {
                self.generalize(&t1)
            } else {
                // the result of a runtime step is not generalized, by a
                // later `let` either
                self.perform(&inner)?;
                self.lower(&t1, self.level);
                mono(t1)
            }
        };
        self.body(x, s, e2, expected)
    }

    fn body(&mut self, x: &Name, s: Scheme, e2: &Term, expected: Option<&Type>) -> Check<Type> {
        self.locals.push((x.clone(), s));
        let t = match expected {
            Some(t) => self.check(e2, t).map(|()| t.clone()),
            None => self.infer(e2),
        };
        self.locals.pop();
        t
    }

    /// A local signature of a function: its own qualifier is inferred, a
    /// fresh capture row, unless written (`regions.md` §2.6, `docs/formal/regions.md#local-signatures-infer-only-the-outer-qualifier`).
    fn local_signature(&mut self, s: &Scheme) -> Scheme {
        let s = &self.in_scopes(s);
        match &s.body {
            Type::Fun(..) => {
                let q = self.fresh_caps();
                Scheme {
                    body: Type::Captured(Rc::new(s.body.clone()), q),
                    ..s.clone()
                }
            }
            _ => s.clone(),
        }
    }

    /// (Let-Rec): without a signature the recursive calls are monomorphic.
    pub(super) fn let_rec(
        &mut self,
        f: &Name,
        sig: Option<&Scheme>,
        lam: &Term,
        e2: &Term,
        expected: Option<&Type>,
    ) -> Check<Type> {
        let s = if let Some(s) = sig {
            let s = &self.in_scopes(s);
            self.locals.push((f.clone(), s.clone()));
            let checked = self.check_forall(lam, s);
            self.locals.pop();
            checked?;
            s.clone()
        } else {
            self.level += 1;
            let t = self.fresh();
            self.locals.push((f.clone(), mono(t.clone())));
            let inferred = self.infer(lam);
            self.locals.pop();
            self.level -= 1;
            self.unify(&t, &inferred?)?;
            self.generalize(&t)
        };
        self.body(f, s, e2, expected)
    }

    /// (Rec): distinct labels, an exact record.
    fn record(&mut self, fields: &[(Name, Term)]) -> Check<Type> {
        let mut out = BTreeMap::new();
        for (l, x) in fields {
            let t = self.infer(x)?;
            if out.insert(l.clone(), t).is_some() {
                return error("Rec", format!("field `{l}` is given more than once"));
            }
        }
        Ok(Type::Record(Fields {
            fields: out,
            tail: None,
        }))
    }

    /// (Field): of a record, an implementation or a dictionary (§1.6).
    pub(super) fn field(&mut self, t: &Type, l: &Name) -> Check<Type> {
        match self.resolve(t) {
            Type::Record(f) => {
                let f = self.zonk_fields(&f);
                if let Some(part) = f.fields.get(l) {
                    return Ok(self.project_part(t, part.clone()));
                }
                match f.tail {
                    Some(Var::Meta(_)) => {
                        let t = self.fresh();
                        let more = Fields {
                            fields: BTreeMap::from([(l.clone(), t.clone())]),
                            tail: Some(Var::Meta(self.meta())),
                        };
                        self.unify(&Type::Record(f), &Type::Record(more))?;
                        Ok(t)
                    }
                    _ => error("Field", format!("`{}` has no field `{l}`", Type::Record(f))),
                }
            }
            m @ Type::Var(Var::Meta(_)) => {
                let t = self.fresh();
                let open = Fields {
                    fields: BTreeMap::from([(l.clone(), t.clone())]),
                    tail: Some(Var::Meta(self.meta())),
                };
                self.unify(&m, &Type::Record(open))?;
                Ok(t)
            }
            Type::Impl(r) => {
                let s = self.service(&r)?;
                let op = self.decls.operation(&s, l).cloned();
                match op {
                    Some(op) => Ok(self.instantiate(&op)),
                    None => error("Field", format!("`{}` has no operation `{l}`", s.name)),
                }
            }
            Type::Dict(c, args) => self.method(&c, &args, l).map(|s| self.instantiate(&s)),
            other => error(
                "Field",
                format!("`{}` has no field `{l}`", self.zonk(&other)),
            ),
        }
    }

    /// (Rec) against a closed record type: each field against its type, so
    /// a field's λ takes the stage the record asks for.
    fn record_against(&mut self, fields: &[(Name, Term)], expected: &Fields) -> Check<()> {
        // equal counts and every expected label given: the labels are distinct
        let same = fields.len() == expected.fields.len()
            && expected
                .fields
                .keys()
                .all(|l| fields.iter().any(|(m, _)| m == l));
        if !same {
            let t = self.record(fields)?;
            return self.subsume(&t, &Type::Record(expected.clone()));
        }
        fields
            .iter()
            .try_for_each(|(l, x)| self.check(x, &expected.fields[l]))
    }
}

pub(super) fn mono(t: Type) -> Scheme {
    Scheme {
        vars: Vec::new(),
        body: t,
        outlives: Vec::new(),
    }
}

fn lit(l: &Lit) -> Type {
    prelude(match l {
        Lit::Int(_) => "Int",
        Lit::Float(_) => "Float",
        Lit::String(_) => "String",
        Lit::Bool(_) => "Bool",
        Lit::Unit => "Unit",
    })
}
