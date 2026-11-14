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

include!("expr/inference.rs");
include!("expr/applications_records.rs");

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
