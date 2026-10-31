//! The rules of `docs/formal/core.md` §2.3 that are not handlers: (Var),
//! (Lam), (App), the `let`s, records and fields, with (Sub) where an
//! expression meets the type it is checked against. Γ₀ (§1.5) is
//! `constant`.

use std::collections::BTreeMap;
use std::mem;
use std::rc::Rc;

use flick_core::numeric::Num;

use super::{Check, Checker, error};
use crate::syntax::{Const, Head, Lit, Name, Path, Prim, Term};
use crate::types::{Fields, Kind, Row, Scheme, Stage, Type, Var};

fn prelude(name: &str) -> Type {
    Type::con("Prelude", name)
}

fn list(t: Type) -> Type {
    Type::app(prelude("List"), t)
}

fn var(n: &str) -> Type {
    Type::Var(Var::Named(n.into()))
}

fn scheme(vars: &[(&str, Kind)], body: Type) -> Scheme {
    Scheme {
        vars: vars
            .iter()
            .map(|(n, k)| (Name::from(*n), k.clone()))
            .collect(),
        body,
    }
}

fn runtime(r: Row, e: Row) -> Stage {
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
            Term::Apply(head, args) => {
                let t = match head {
                    Head::Con(c) => self.constructor(&c.ty.module, &c.name)?,
                    Head::Const(c) => self.constant(c)?,
                };
                args.iter().try_fold(t, |t, a| self.apply(&t, a))
            }
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
        let expected = self.resolve(expected);
        match (e, &expected) {
            (_, Type::Forall(s)) => self.check_forall(e, s),
            (Term::Lam(x, body), Type::Fun(p, s, r)) => {
                let outer = mem::replace(&mut self.stage, Stage::Pure);
                self.locals.push((x.clone(), mono((**p).clone())));
                let checked = self.check(body, r);
                self.locals.pop();
                let inner = mem::replace(&mut self.stage, outer);
                checked?;
                self.stage_within(&inner, s)
            }
            (Term::Let(x, sig, e1, e2), _) => self
                .let_(x, sig.as_deref(), e1, e2, Some(&expected))
                .map(drop),
            (Term::LetRec(f, sig, lam, e2), _) => self
                .let_rec(f, sig.as_deref(), lam, e2, Some(&expected))
                .map(drop),
            (Term::Match(e, arms), _) => self.match_(e, arms, Some(&expected)).map(drop),
            (Term::Try(body, arms), _) => self.try_(body, arms, Some(&expected)).map(drop),
            (Term::Provide(l, body), _) => self.provide(l, body, Some(&expected)).map(drop),
            (Term::Task(body), Type::Eff(..)) => self.task(body, Some(&expected)).map(drop),
            (Term::Record(fields), Type::Record(f)) if f.tail.is_none() => {
                self.record_against(fields, f)
            }
            (Term::Record(fields), Type::Impl(r)) => {
                let f = self.implementation(r)?;
                self.record_against(fields, &f)
            }
            (Term::Record(fields), Type::Dict(c, args)) => self.dictionary(fields, c, args),
            (Term::With(..) | Term::Handle(..), _) => {
                self.runtime_form(e, Some(&expected)).map(drop)
            }
            (Term::App(f, a), _) if self.config && matches!(&**f, Term::Lam(..)) => {
                let Term::Lam(x, body) = &**f else {
                    unreachable!("a λ, matched above")
                };
                self.redex(x, body, a, Some(&expected)).map(drop)
            }
            // `bind_S (await v)` of a configuration: what the layer builds is
            // checked against `S.Impl`, as `Layer.effect` checked it
            (Term::Await(x), _) if self.config => {
                let (r, x_rows) = (self.fresh_row(), self.fresh_row());
                self.check(x, &Type::Eff(r.clone(), x_rows.clone(), Rc::new(expected)))?;
                self.perform(&Stage::Runtime(r, x_rows))
            }
            _ => {
                let t = self.infer(e)?;
                self.subsume(&t, &expected)
            }
        }
    }

    /// (Let-Sig) with `σ` a `∀`: the value is checked with rigid variables,
    /// and a runtime step while it is built is `FLICK-T013` (Р1).
    pub(super) fn check_forall(&mut self, e: &Term, s: &Scheme) -> Check<()> {
        let body = self.skolemize(s);
        let outer = mem::replace(&mut self.stage, Stage::Pure);
        let checked = self.check(e, &body);
        let inner = mem::replace(&mut self.stage, outer);
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
    fn global(&mut self, p: &Path) -> Check<Type> {
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
        Ok(Type::fun(p, inner, r?))
    }

    /// (App): `φ₁ ⊔ φ₂ ⊔ φ`. A callee of unknown type is a pure arrow (Р7).
    fn app(&mut self, f: &Term, a: &Term) -> Check<Type> {
        if let (true, Term::Lam(x, body)) = (self.config, f) {
            return self.redex(x, body, a, None);
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
            self.check_forall(e1, s)?;
            s.clone()
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
                // the result of a runtime step is not generalized
                self.perform(&inner)?;
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
                if let Some(t) = f.fields.get(l) {
                    return Ok(t.clone());
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

    /// The constants of Γ₀ (§1.5) and the operators.
    pub(super) fn constant(&mut self, c: &Const) -> Check<Type> {
        let none = Row::none;
        let a = || var("a");
        let s = match c {
            Const::RefMake => scheme(
                &[("a", Kind::Type)],
                Type::fun(a(), runtime(none(), none()), Type::Ref(Rc::new(a()))),
            ),
            Const::RefGet => scheme(
                &[("a", Kind::Type)],
                Type::fun(Type::Ref(Rc::new(a())), runtime(none(), none()), a()),
            ),
            Const::RefSet => scheme(
                &[("a", Kind::Type)],
                Type::fun(
                    Type::Ref(Rc::new(a())),
                    Stage::Pure,
                    Type::fun(a(), runtime(none(), none()), prelude("Unit")),
                ),
            ),
            Const::Raise => {
                let e = Row::var(Var::Named("e".into()));
                scheme(
                    &[("e", Kind::Errors), ("a", Kind::Type)],
                    Type::fun(Type::Errors(e.clone()), runtime(none(), e), a()),
                )
            }
            Const::Ask(s) => Scheme {
                vars: Vec::new(),
                body: Type::fun(
                    prelude("Unit"),
                    runtime(Row::one(s.clone()), none()),
                    Type::Impl(Row::one(s.clone())),
                ),
            },
            Const::LayerSucceed(s) => Scheme {
                vars: Vec::new(),
                body: Type::fun(
                    Type::Impl(Row::one(s.clone())),
                    Stage::Pure,
                    Type::Layer(none(), none(), Row::one(s.clone())),
                ),
            },
            Const::LayerEffect(s) => {
                let (r, e) = (
                    Row::var(Var::Named("r".into())),
                    Row::var(Var::Named("e".into())),
                );
                scheme(
                    &[("r", Kind::Services), ("e", Kind::Errors)],
                    Type::fun(
                        Type::Eff(
                            r.clone(),
                            e.clone(),
                            Rc::new(Type::Impl(Row::one(s.clone()))),
                        ),
                        Stage::Pure,
                        Type::Layer(r, e, Row::one(s.clone())),
                    ),
                )
            }
            Const::Prim(p) => prim(*p),
            other => {
                return error(
                    "Var",
                    format!("the constant {other:?} has no type in the model"),
                );
            }
        };
        Ok(self.instantiate(&s))
    }
}

pub(super) fn mono(t: Type) -> Scheme {
    Scheme {
        vars: Vec::new(),
        body: t,
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

fn num(n: Num) -> Type {
    prelude(n.name())
}

/// The operators: arithmetic on its type, comparisons on any.
fn prim(p: Prim) -> Scheme {
    let binary = |t: Type, r: Type| Type::fun(t.clone(), Stage::Pure, Type::fun(t, Stage::Pure, r));
    let a = var("a");
    let any = |body| scheme(&[("a", Kind::Type)], body);
    match p {
        Prim::Arith(_, n) => mono(binary(num(n), num(n))),
        Prim::Neg(n) => mono(Type::fun(num(n), Stage::Pure, num(n))),
        Prim::Eq
        | Prim::NotEq
        | Prim::Same
        | Prim::NotSame
        | Prim::Lt
        | Prim::Le
        | Prim::Gt
        | Prim::Ge => any(binary(a, prelude("Bool"))),
        Prim::Not => mono(Type::fun(prelude("Bool"), Stage::Pure, prelude("Bool"))),
        Prim::Concat => mono(binary(prelude("String"), prelude("String"))),
        Prim::ListConcat => any(binary(list(a.clone()), list(a))),
        Prim::TagOf => any(Type::fun(a, Stage::Pure, prelude("String"))),
        Prim::Atom => mono(Type::fun(prelude("String"), Stage::Pure, prelude("String"))),
    }
}
