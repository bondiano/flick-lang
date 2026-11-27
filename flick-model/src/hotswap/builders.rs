//! Term and type builders shared by the hot-swap session generator.

use std::collections::BTreeMap;
use std::rc::Rc;

use flick_core::numeric::{Arith, Num};

use crate::syntax::{Const, Lit, Name, Path, Prim, Term};
use crate::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

pub(super) struct Cursor<'a> {
    choices: &'a [u32],
    at: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(choices: &'a [u32]) -> Self {
        Self { choices, at: 0 }
    }

    pub(super) fn pick(&mut self, n: usize) -> usize {
        let c = self.choices.get(self.at).copied().unwrap_or(0);
        self.at += 1;
        usize::try_from(c).unwrap_or(0) % n.max(1)
    }
}

pub(super) const HOT_MOD: &str = "Hot";
pub(super) const FIXTURE_MOD: &str = "Fixture";

pub(super) fn hot(name: &str) -> Path {
    Path::new(HOT_MOD, name)
}

pub(super) fn prelude_ty(name: &str) -> Type {
    Type::con("Prelude", name)
}

pub(super) fn int_ty() -> Type {
    prelude_ty("Int")
}

pub(super) fn bool_ty() -> Type {
    prelude_ty("Bool")
}

pub(super) fn string_ty() -> Type {
    prelude_ty("String")
}

pub(super) fn unit_ty() -> Type {
    prelude_ty("Unit")
}

pub(super) fn pair_ty(a: Type, b: Type) -> Type {
    Type::Tuple(Rc::from([a, b]))
}

pub(super) fn owner_ty(region: &str) -> Type {
    Type::app(
        Type::con("Fiber", "Owner"),
        Type::Var(Var::Named(region.into())),
    )
}

pub(super) fn mono(body: Type) -> Scheme {
    Scheme::new(Vec::new(), body)
}

pub(super) fn poly_id_scheme() -> Scheme {
    Scheme::new(
        vec![("a".into(), Kind::Type)],
        Type::fun(
            Type::Var(Var::Named("a".into())),
            Stage::Pure,
            Type::Var(Var::Named("a".into())),
        ),
    )
}

pub(super) fn add_int(a: Term, b: Term) -> Term {
    Term::app(
        Term::app(
            Term::konst(Const::Prim(Prim::Arith(Arith::Add, Num::Int))),
            a,
        ),
        b,
    )
}

pub(super) fn add_const_fn(delta: i32) -> Term {
    Term::lam(
        "x",
        add_int(Term::Var("x".into()), Term::Lit(Lit::Int(delta))),
    )
}

pub(super) fn diverging_fn(new_ty: &Type) -> Term {
    let loop_scheme = Scheme::new(
        Vec::new(),
        Type::fun(unit_ty(), Stage::Pure, new_ty.clone()),
    );
    Term::lam(
        "_old",
        Term::LetRec(
            "spin".into(),
            Some(Rc::new(loop_scheme)),
            Rc::new(Term::lam(
                "u",
                Term::app(Term::Var("spin".into()), Term::Var("u".into())),
            )),
            Rc::new(Term::app(Term::Var("spin".into()), Term::unit())),
        ),
    )
}

pub(super) fn dying_fn(msg: &str) -> Term {
    Term::lam("_old", Term::Die(msg.into()))
}

pub(super) fn unit_thunk_ty(caps: Caps) -> Type {
    let arrow = Type::fun(
        unit_ty(),
        Stage::Runtime(Row::none(), Row::none()),
        unit_ty(),
    );
    if caps.is_empty() {
        arrow
    } else {
        Type::Captured(Rc::new(arrow), caps)
    }
}

pub(super) fn thunk_record_ty(caps: Caps) -> Type {
    let mut fields = BTreeMap::new();
    fields.insert("run".into(), unit_thunk_ty(caps));
    Type::Record(Fields { fields, tail: None })
}

pub(super) fn thunk_record_val() -> Term {
    Term::lam(
        "_o",
        Term::Record(Rc::from([(
            Name::from("run"),
            Term::lam("_u", Term::unit()),
        )])),
    )
}
