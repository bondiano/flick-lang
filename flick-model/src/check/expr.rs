//! The rules of `docs/formal/core.md` §2.3 that are not handlers: (Var),
//! (Lam), (App), records and fields (the `let`s are `bindings.rs`), with (Sub) where an
//! expression meets the type it is checked against. Γ₀ (§1.5) is
//! `constant`.

use std::collections::BTreeMap;
use std::mem;
use std::rc::Rc;

use super::dicts::at_args;
use super::generalize::own_regions;
use super::regions::live_stage;
use super::{Check, Checker, error};
use crate::subst::free_vars;
use crate::syntax::{Const, Head, Lit, Name, Path, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

pub(super) fn prelude(name: &str) -> Type {
    Type::con("Prelude", name)
}

pub(super) fn list(t: Type) -> Type {
    Type::app(prelude("List"), t)
}

pub(super) fn seq(t: Type) -> Type {
    Type::app(prelude("Seq"), t)
}

pub(super) fn option(t: Type) -> Type {
    Type::app(prelude("Option"), t)
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

include!("expr/inference.rs");
include!("expr/applications_records.rs");

pub(super) fn mono(t: Type) -> Scheme {
    Scheme {
        vars: Vec::new(),
        body: t,
        outlives: Vec::new(),
    }
}

impl Checker<'_> {
    /// (FuncRef-In): a term that names no local is a value of `FuncRef f`
    /// when it is one of `f`; as the checker, this one takes a λ, a
    /// top-level value, a constructor or an operation. A term of type
    /// `FuncRef f` already passes as it is.
    fn check_func_ref(&mut self, e: &Term, f: &Type, full: &Type) -> Check<()> {
        if matches!(e, Term::Lam(..) | Term::Fix(..)) && func_ref(e) {
            return self.check(e, f);
        }
        let t = self.infer(e)?;
        if matches!(self.resolve(&t), Type::App(h, _) if *h == prelude("FuncRef")) {
            return self.fit(e, &t, full);
        }
        if func_ref(e) {
            return self.fit(e, &t, f);
        }
        if matches!(self.resolve(&t), Type::Fun(..) | Type::Forall(_)) {
            return error(
                "FuncRef-In",
                "a `FuncRef` is a λ or a top-level value that names no local",
            );
        }
        self.fit(e, &t, full)
    }
}

/// Whether `e` is a function the checker takes as a `FuncRef`: a λ, a
/// top-level value, a constructor or an operation, with no free variable.
fn func_ref(e: &Term) -> bool {
    let form = match e {
        Term::Lam(..) | Term::Fix(..) | Term::Global(_) => true,
        Term::Apply(Head::Con(_) | Head::Const(_), args) => args.is_empty(),
        _ => false,
    };
    form && free_vars(e).is_empty()
}

fn lit(l: &Lit) -> Type {
    prelude(match l {
        Lit::Int(_) => "Int",
        Lit::Int8(_) => "Int8",
        Lit::Int16(_) => "Int16",
        Lit::Int64(_) => "Int64",
        Lit::UInt8(_) => "UInt8",
        Lit::UInt16(_) => "UInt16",
        Lit::UInt32(_) => "UInt32",
        Lit::UInt64(_) => "UInt64",
        Lit::Float(_) => "Float",
        Lit::Float32(_) => "Float32",
        Lit::BigInt(_) => "BigInt",
        Lit::String(_) => "String",
        Lit::Bool(_) => "Bool",
        Lit::Unit => "Unit",
    })
}
