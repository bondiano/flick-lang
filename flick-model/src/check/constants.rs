//! The constants of Γ₀ (`docs/formal/core.md` §1.5) and the operators.

use std::rc::Rc;
use std::slice;

use flick_core::numeric::{Num, NumLang};

use super::expr::{list, mono, option, prelude, runtime, scheme, seq, var};
use super::generalize::{params, substitute};
use super::{Check, Checker, error};
use crate::syntax::{Const, Path, Prim, RegionOp, Term};
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
            Const::SeqIter
            | Const::SeqThunk
            | Const::SeqNext
            | Const::SeqSync
            | Const::SeqFlatMapSync => seq_constant(c),
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
            // Like Ref's primitives, Arena implements its declarations,
            // not recursive source references to them. Take a fresh full
            // scheme: pinning only its capture variables to `own` would
            // lose the bound on a freshly instantiated `a`.
            Const::Region(p, RegionOp::ArenaMake | RegionOp::ArenaAlloc | RegionOp::ArenaGet) => {
                return self.declared_global(p);
            }
            // Preserve the existing region-binding rule, including its
            // owner/callback row matching. Normalizing all foreign schemes
            // independently still needs a general row-instantiation rule.
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

/// The scheme of a sequence primitive constant (`SeqIter`, `SeqThunk`, `SeqNext`,
/// `SeqSync`, `SeqFlatMapSync`).
fn seq_constant(c: &Const) -> Scheme {
    let a = || var("a");
    let cap = |n: &str| Caps::var(Var::Named(n.into()));
    let cap_seq = |elem: Type, q: Caps| Type::Captured(Rc::new(seq(elem)), q);
    let r_stage = || runtime(Row::var(Var::Named("r".into())), Row::none());
    match c {
        Const::SeqIter => {
            let s = cap_seq(a(), cap("d"));
            scheme(
                &[("d", Kind::Captures), ("a", Kind::Type)],
                Type::fun(s.clone(), Stage::Pure, s),
            )
        }
        Const::SeqThunk => {
            let d = cap("d");
            let s = cap_seq(a(), d.clone());
            let step = Type::Captured(
                Rc::new(Type::fun(
                    prelude("Unit"),
                    Stage::Pure,
                    option(Type::Tuple(Rc::from([a(), s.clone()]))),
                )),
                d,
            );
            scheme(
                &[("d", Kind::Captures), ("a", Kind::Type)],
                Type::fun(step, Stage::Pure, s),
            )
        }
        Const::SeqNext => {
            let s = cap_seq(a(), cap("d"));
            let step = option(Type::Tuple(Rc::from([a(), s.clone()])));
            scheme(
                &[("d", Kind::Captures), ("a", Kind::Type)],
                Type::fun(s, Stage::Pure, step),
            )
        }
        Const::SeqSync => {
            let (c, d, stage) = (cap("c"), cap("d"), r_stage());
            let thunk = Type::Captured(
                Rc::new(Type::fun(
                    prelude("Unit"),
                    stage.clone(),
                    cap_seq(a(), d.clone()),
                )),
                c.clone(),
            );
            scheme(
                &[
                    ("c", Kind::Captures),
                    ("d", Kind::Captures),
                    ("r", Kind::Services),
                    ("a", Kind::Type),
                ],
                Type::fun(thunk, stage, cap_seq(a(), c.union(&d))),
            )
        }
        Const::SeqFlatMapSync => {
            let (c, d, e, stage) = (cap("c"), cap("d"), cap("e"), r_stage());
            let all = c.union(&d).union(&e);
            let f = Type::Captured(
                Rc::new(Type::fun(a(), stage.clone(), cap_seq(var("b"), e))),
                c,
            );
            scheme(
                &[
                    ("c", Kind::Captures),
                    ("d", Kind::Captures),
                    ("e", Kind::Captures),
                    ("r", Kind::Services),
                    ("a", Kind::Type),
                    ("b", Kind::Type),
                ],
                Type::fun(
                    f,
                    Stage::Pure,
                    Type::fun(cap_seq(a(), d), stage, cap_seq(var("b"), all)),
                ),
            )
        }
        _ => unreachable!("non-sequence constant passed to `seq_constant`"),
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
const ORDERED: [&str; 12] = [
    "Int", "Int8", "Int16", "Int64", "UInt8", "UInt16", "UInt32", "UInt64", "Float", "Float32",
    "BigInt", "String",
];

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

    /// Checks `SeqIter source`: accepts `List a`, `Seq a`, or `f a` with an
    /// `Iterable f` given in scope (deferring if the source type is still a
    /// meta), and reports any other collection source as a `(Translation)` gap.
    pub(super) fn seq_iter(&mut self, source: &Term) -> Check<Type> {
        let src_ty = self.infer(source)?;
        let givens = self.iterable_givens();
        if let Some(elem) = self.iterable_element(&src_ty, &givens, false)? {
            return Ok(self.rewrap_qualifiers(&src_ty, seq(elem)));
        }
        let elem = self.fresh();
        self.iterables.push((src_ty.clone(), elem.clone(), givens));
        let q = self.fresh_caps();
        let out = seq(elem);
        self.hold(&[src_ty], slice::from_ref(&out), &q, "SeqIter");
        Ok(Type::Captured(Rc::new(out), q))
    }

    /// Type constructors `f` for which an `Iterable f` dictionary is in scope,
    /// directly or as a superclass of a dictionary parameter in `self.locals`.
    fn iterable_givens(&self) -> Vec<Type> {
        let iterable = Path::new("Prelude", "Iterable");
        let mut out = Vec::new();
        let mut work: Vec<Type> = self
            .locals
            .iter()
            .filter(|(_, s)| s.vars.is_empty())
            .map(|(_, s)| self.resolve(&s.body))
            .collect();
        let mut seen = Vec::new();
        while let Some(t) = work.pop() {
            let Type::Dict(class, args) = t else {
                continue;
            };
            let key = (class.clone(), args.clone());
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            if class == iterable && args.len() == 1 {
                out.push(self.zonk(&args[0]));
            }
            if let Some(def) = self.decls.classes.get(&class) {
                let map = params(&def.params, &args);
                for sup in &def.supers {
                    work.push(self.resolve(&substitute(sup, &map)));
                }
            }
        }
        out
    }

    /// Extracts the element type of a supported `for` source (`List a`, `Seq a`,
    /// or `f a` with `Iterable f` in `givens`). Returns `Ok(None)` when the
    /// container is still an unsolved meta and `final_check` is false; reports
    /// a `(Translation)` gap for any other collection source.
    pub(super) fn iterable_element(
        &mut self,
        src_ty: &Type,
        givens: &[Type],
        final_check: bool,
    ) -> Check<Option<Type>> {
        let bare = self.resolve(src_ty);
        match &bare {
            Type::Var(Var::Meta(_)) => {
                if final_check {
                    error(
                        "Translation",
                        format!("a `for` over `{}` has no λFlick form", self.zonk(src_ty)),
                    )
                } else {
                    Ok(None)
                }
            }
            Type::App(head, elem) => {
                let head_res = self.resolve(head);
                if head_res == prelude("List") || head_res == prelude("Seq") {
                    return Ok(Some((**elem).clone()));
                }
                if matches!(head_res, Type::Var(Var::Meta(_))) {
                    return if final_check {
                        error(
                            "Translation",
                            format!("a `for` over `{}` has no λFlick form", self.zonk(src_ty)),
                        )
                    } else {
                        Ok(None)
                    };
                }
                let head_zonked = self.zonk(&head_res);
                if givens
                    .iter()
                    .any(|g| self.zonk(&self.resolve(g)) == head_zonked)
                {
                    return Ok(Some((**elem).clone()));
                }
                error(
                    "Translation",
                    format!("a `for` over `{}` has no λFlick form", self.zonk(&bare)),
                )
            }
            _ => error(
                "Translation",
                format!("a `for` over `{}` has no λFlick form", self.zonk(&bare)),
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
