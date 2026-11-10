//! The constants of Γ₀ (`docs/formal/core.md` §1.5) and the operators.

use std::rc::Rc;

use flick_core::numeric::{Num, NumLang};

use super::expr::{list, mono, prelude, runtime, scheme, var};
use super::{Check, Checker, error};
use crate::syntax::{Const, Prim};
use crate::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

impl Checker<'_> {
    /// The constants of Γ₀ (§1.5) and the operators.
    pub(super) fn constant(&mut self, c: &Const) -> Check<Type> {
        let none = Row::none;
        let a = || var("a");
        let s = match c {
            // with the capacity `c` of `regions.md` §1.5 inside the
            // invariant argument of `Ref`
            Const::RefMake => scheme(
                &[("c", Kind::Captures), ("a", Kind::Type)],
                Type::fun(
                    bounded(a()),
                    runtime(none(), none()),
                    Type::Ref(Rc::new(bounded(a()))),
                ),
            ),
            Const::RefGet => scheme(
                &[("c", Kind::Captures), ("a", Kind::Type)],
                Type::fun(
                    captured(Type::Ref(Rc::new(a()))),
                    runtime(none(), none()),
                    bounded(a()),
                ),
            ),
            Const::RefSet => scheme(
                &[("c", Kind::Captures), ("a", Kind::Type)],
                Type::fun(
                    captured(Type::Ref(Rc::new(a()))),
                    Stage::Pure,
                    captured(Type::fun(
                        bounded(a()),
                        runtime(none(), none()),
                        prelude("Unit"),
                    )),
                ),
            ),
            Const::Raise => {
                let e = Row::var(Var::Named("e".into()));
                scheme(
                    &[("e", Kind::Errors), ("a", Kind::Type)],
                    Type::fun(Type::Errors(e.clone()), runtime(none(), e), a()),
                )
            }
            // `∀p̄. Unit →R⟨S p̄⟩ S.Impl p̄`, and so the layers of `S`
            Const::Ask(s) => {
                let (vars, sp) = self.service_row(s);
                Scheme::new(
                    vars,
                    Type::fun(prelude("Unit"), runtime(sp.clone(), none()), Type::Impl(sp)),
                )
            }
            Const::LayerSucceed(s) => {
                let (vars, sp) = self.service_row(s);
                Scheme::new(
                    vars,
                    Type::fun(
                        Type::Impl(sp.clone()),
                        Stage::Pure,
                        Type::Layer(none(), none(), sp),
                    ),
                )
            }
            Const::LayerEffect(s) => {
                // named apart from the service's parameters
                let (mut vars, sp) = self.service_row(s);
                let (r, e) = (
                    Row::var(Var::Named("%r".into())),
                    Row::var(Var::Named("%e".into())),
                );
                vars.extend([("%r".into(), Kind::Services), ("%e".into(), Kind::Errors)]);
                Scheme::new(
                    vars,
                    Type::fun(
                        Type::Eff(r.clone(), e.clone(), Rc::new(Type::Impl(sp.clone()))),
                        Stage::Pure,
                        Type::Layer(r, e, sp),
                    ),
                )
            }
            Const::Prim(p @ (Prim::Lt | Prim::Le | Prim::Gt | Prim::Ge)) => {
                let t = self.instance_of(prim(*p));
                if let Type::Fun(operand, ..) = &t {
                    self.ordered.push((**operand).clone());
                }
                return Ok(t);
            }
            Const::Prim(p) => prim(*p),
            // a binding of regions has the scheme of the global it stands for
            Const::Region(p, _) => return self.global(p),
            other => {
                return error(
                    "Var",
                    format!("the constant {other:?} has no type in the model"),
                );
            }
        };
        let s = self.live_scheme(&s);
        Ok(self.instance_of(s))
    }
}

/// The scheme of an operator, a global of a certificate (`crate::cert`).
pub(crate) fn prim_scheme(p: Prim) -> Scheme {
    prim(p)
}

/// `τ^c`, `c` the capacity of a cell.
fn captured(t: Type) -> Type {
    Type::Captured(Rc::new(t), Caps::var(Var::Named("c".into())))
}

/// `a^c` as a value bound.
fn bounded(t: Type) -> Type {
    Type::Bounded(Rc::new(t), Caps::var(Var::Named("c".into())))
}

fn num(n: Num) -> Type {
    prelude(n.name())
}

/// The types `<`, `<=`, `>`, `>=` order (§1.5).
const ORDERED: [&str; 4] = ["Int", "Float", "BigInt", "String"];

impl Checker<'_> {
    /// An operand of an ordering comparison: one of `ORDERED`, `Int` when
    /// nothing chose it, as flick-core's `settle_operators`.
    // shortcut: a rigid variable is rejected even under a given `Ord`, which flick-core accepts; the generator makes no such program, check its `Ord` when it does
    pub(super) fn ordered_operand(&mut self, t: &Type) -> Check<()> {
        match self.zonk(&self.resolve(t)) {
            Type::Var(Var::Meta(_)) => self.unify(t, &prelude("Int")),
            Type::Con(p) if &*p.module == "Prelude" && ORDERED.contains(&&*p.name) => Ok(()),
            other => error(
                "Prim",
                format!("a comparison works on Int, Float, BigInt or String, not on `{other}`"),
            ),
        }
    }
}

/// The operators: arithmetic on its type, equality on any, ordering on
/// the types of `ORDERED`.
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
