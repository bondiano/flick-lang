//! Spanned terms of `PureTotalV2` and their strict extraction from checked IR.
//!
//! Every `Int` comparison or `==`/`!=` must have its operand type positively
//! confirmed as `Int` (or `Bool` / compiler-emitted nullary ADT `_tag` for
//! `Same`/`NotSame`); any ambiguity or unsupported type/operator is refused
//! with `FLICK-T082`.

mod binary;
mod extract;
mod infer;

pub use extract::extract;
pub(super) use extract::extract_numeric_int_method;

use std::rc::Rc;

use crate::logic::{Ctor, Def, Ty};
use crate::source::Span;

/// Built-in type and class identities (`(module, name)`), supplied by the
/// caller from `types::lang` so `logic` stays independent of `types::lang`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LangTypes {
    pub int: (String, String),
    pub bool_: (String, String),
    pub float: (String, String),
    pub bigint: (String, String),
    pub string: (String, String),
    pub unit: (String, String),
    pub eq_class: (String, String),
    pub ord_class: (String, String),
    pub numeric_class: (String, String),
}

impl LangTypes {
    /// The primitive an IR type names; any type other than a named
    /// constructor is `Other`.
    pub fn classify_ty(&self, ty: &flick_ir::IrType) -> PrimHint {
        match ty {
            flick_ir::IrType::Named { module, name, .. } => self.classify(module, name),
            _ => PrimHint::Other,
        }
    }

    pub fn classify(&self, module: &str, name: &str) -> PrimHint {
        let pair = (module, name);
        if pair == (self.int.0.as_str(), self.int.1.as_str()) {
            PrimHint::Int
        } else if pair == (self.bool_.0.as_str(), self.bool_.1.as_str()) {
            PrimHint::Bool
        } else if pair == (self.float.0.as_str(), self.float.1.as_str()) {
            PrimHint::Float
        } else if pair == (self.bigint.0.as_str(), self.bigint.1.as_str()) {
            PrimHint::BigInt
        } else if pair == (self.string.0.as_str(), self.string.1.as_str()) {
            PrimHint::String
        } else if pair == (self.unit.0.as_str(), self.unit.1.as_str()) {
            PrimHint::Unit
        } else {
            PrimHint::Other
        }
    }

    pub fn is_numeric_int_instance(&self, module: &str, instance: &str) -> bool {
        module == self.numeric_class.0
            && instance == format!("{} {}", self.numeric_class.1, self.int.1)
    }
}

/// Primitive type classification used to confirm built-in `Int`/`Bool`
/// comparisons and equality during IR extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimHint {
    Int,
    Bool,
    Float,
    BigInt,
    String,
    Unit,
    Other,
}

/// Maps a `logic::Ty` to its `PrimHint` for seeding parameter environments.
pub fn prim_hint_of_ty(ty: &Ty, lang: &LangTypes) -> PrimHint {
    match ty {
        Ty::Int => PrimHint::Int,
        Ty::Bool => PrimHint::Bool,
        Ty::Data { module, name, .. } => lang.classify(module, name),
        Ty::Rigid(_) | Ty::Fun(..) | Ty::Refused(_) => PrimHint::Other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub kind: TermKind,
    pub span: Span,
}

impl Term {
    pub fn new(kind: TermKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Rewrites every span in this term tree to `span`, preventing foreign
    /// `FileId` spans from leaking into another module's diagnostics.
    #[must_use]
    pub fn remap_span(&self, span: Span) -> Self {
        let map = |t: &Term| Rc::new(t.remap_span(span));
        let kind = match &self.kind {
            TermKind::Var(v) => TermKind::Var(v.clone()),
            TermKind::IntLit(n) => TermKind::IntLit(*n),
            TermKind::BoolLit(b) => TermKind::BoolLit(*b),
            TermKind::UnitLit => TermKind::UnitLit,
            TermKind::IntBin(op, a, b) => TermKind::IntBin(*op, map(a), map(b)),
            TermKind::IntNeg(x) => TermKind::IntNeg(map(x)),
            TermKind::IntCmp(op, a, b) => TermKind::IntCmp(*op, map(a), map(b)),
            TermKind::BoolEq { eq, lhs, rhs } => TermKind::BoolEq {
                eq: *eq,
                lhs: map(lhs),
                rhs: map(rhs),
            },
            TermKind::TagEq { eq, lhs, rhs } => TermKind::TagEq {
                eq: *eq,
                lhs: map(lhs),
                rhs: map(rhs),
            },
            TermKind::And(a, b) => TermKind::And(map(a), map(b)),
            TermKind::Or(a, b) => TermKind::Or(map(a), map(b)),
            TermKind::Not(x) => TermKind::Not(map(x)),
            TermKind::If(c, t, f) => TermKind::If(map(c), map(t), map(f)),
            TermKind::Lam(p, b) => TermKind::Lam(p.clone(), map(b)),
            TermKind::App {
                func,
                arg,
                arg_first,
            } => TermKind::App {
                func: map(func),
                arg: map(arg),
                arg_first: *arg_first,
            },
            TermKind::Con(c) => TermKind::Con(c.clone()),
            TermKind::Match(s, arms) => TermKind::Match(
                map(s),
                arms.iter()
                    .map(|a| Arm {
                        pat: a.pat.clone(),
                        body: a.body.remap_span(span),
                    })
                    .collect(),
            ),
            TermKind::Let(x, v, b) => TermKind::Let(x.clone(), map(v), map(b)),
            TermKind::Def(d) => TermKind::Def(d.clone()),
        };
        Term::new(kind, span)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntBinOp {
    Add,
    Sub,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntCmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermKind {
    Var(Rc<str>),
    IntLit(i128),
    BoolLit(bool),
    UnitLit,
    IntBin(IntBinOp, Rc<Term>, Rc<Term>),
    IntNeg(Rc<Term>),
    IntCmp(IntCmpOp, Rc<Term>, Rc<Term>),
    BoolEq {
        eq: bool,
        lhs: Rc<Term>,
        rhs: Rc<Term>,
    },
    /// Constructor-tag equality for nullary tagged ADTs (`Identity::Tag`).
    TagEq {
        eq: bool,
        lhs: Rc<Term>,
        rhs: Rc<Term>,
    },
    And(Rc<Term>, Rc<Term>),
    Or(Rc<Term>, Rc<Term>),
    Not(Rc<Term>),
    If(Rc<Term>, Rc<Term>, Rc<Term>),
    Lam(Rc<str>, Rc<Term>),
    App {
        func: Rc<Term>,
        arg: Rc<Term>,
        arg_first: bool,
    },
    Con(Rc<Ctor>),
    Match(Rc<Term>, Rc<[Arm]>),
    Let(Rc<str>, Rc<Term>, Rc<Term>),
    Def(Rc<Def>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    pub pat: Pat,
    pub body: Term,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pat {
    Wild,
    Bind(Rc<str>),
    Int(i128),
    Bool(bool),
    Unit,
    Con(Rc<Ctor>, Vec<Pat>),
}
