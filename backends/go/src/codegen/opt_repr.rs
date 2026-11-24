//! Allocation-free concrete `Option` Go layouts (`flickrt.OptBool`,
//! `flickrt.OptRef`, `flickrt.Opt[T]`).

use flick_backend_support::Known;
use flick_ir::{IrType, Lang, Rep};
use flick_pretty::{Document, DocumentArena, docvec};

use super::adt::align_up;
use super::arith::Native;
use super::go_type::{GoType, TypeDecls};
use super::layout::{Code, call};

/// Which `Option` constructor a [`Constructor`] is, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OptCon {
    Some,
    None,
}

/// The payload slot of a presence-flag `flickrt.Opt[T]`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum OptPayload {
    Int,
    Float,
    String,
    /// Concrete payload whose Go type is `flickrt.Value` and may hold `nil`
    /// (for example `Unknown`, `JsValue`, opaque foreign types, or aliases).
    Value,
    /// Nested concrete `Option`, distinguishing every level including `Some None`.
    Nested(Box<OptRepr>),
}

impl OptPayload {
    pub(super) fn go_type(&self) -> String {
        match self {
            OptPayload::Int => "int32".to_owned(),
            OptPayload::Float => "float64".to_owned(),
            OptPayload::String => "string".to_owned(),
            OptPayload::Value => "flickrt.Value".to_owned(),
            OptPayload::Nested(inner) => inner.go_type(),
        }
    }

    pub(super) fn size_align(&self) -> (usize, usize) {
        match self {
            OptPayload::Int => (4, 4),
            OptPayload::Float => (8, 8),
            OptPayload::String | OptPayload::Value => (16, 8),
            OptPayload::Nested(inner) => inner.size_align(),
        }
    }
}

/// The allocation-free Go layout of a concrete `Option T`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum OptRepr {
    /// 1-byte sentinel `flickrt.OptBool` (`0 = None`, `1 = Some(false)`, `2 = Some(true)`).
    BoolSentinel,
    /// `flickrt.OptRef` (`struct{ val flickrt.Value }`, `nil` sentinel for `None`).
    /// Used only when the concrete non-`Option` payload is guaranteed non-nil.
    RefSentinel,
    /// Presence-flag struct `flickrt.Opt[T]` (`{ Val T; Ok bool }`).
    Flag(OptPayload),
}

/// Built-in lang types whose Go `flickrt.Value` representation is guaranteed
/// non-nil for every valid value (never `nil` interface).
const NON_NIL_LANG: &[Lang] = &[
    Lang::Unit,
    Lang::Bytes,
    Lang::BigInt,
    Lang::List,
    Lang::Seq,
    Lang::Ref,
    // `Record.Field l a` projects to `{ l : a }` (`IrType::Record`) during
    // unification and is always a non-nil `flickrt.RecordView` at runtime.
    Lang::Field,
    Lang::Schema,
    Lang::Url,
    Lang::PureCodec,
    Lang::Eff,
    Lang::Lazy,
    Lang::Layer,
    Lang::Codec,
    Lang::Result,
    Lang::Exit,
    Lang::Failure,
    Lang::Json,
    Lang::SchemaError,
    Lang::AppError,
];

impl OptRepr {
    /// Computes the typed `Option` Go layout for `ty` when `ty` is a concrete
    /// `Option T` eligible for allocation-free representation.
    ///
    /// Substitution-invariant: if `of(T) == Some(r)`, then for every type
    /// substitution `σ`, `of(σ(T)) == Some(r)`.
    pub(super) fn of(lang: &Known, decls: &TypeDecls, ty: Option<&IrType>) -> Option<OptRepr> {
        let IrType::Named { module, name, args } = ty? else {
            return None;
        };
        if !lang.is(Lang::Option, module, name) || args.len() != 1 {
            return None;
        }
        Self::for_payload(lang, decls, &args[0])
    }

    fn for_payload(lang: &Known, decls: &TypeDecls, payload: &IrType) -> Option<OptRepr> {
        match payload {
            IrType::Var(_) | IrType::App { .. } | IrType::Exists { .. } => None,
            IrType::Forall { constraints, .. } => {
                (!constraints.is_empty()).then_some(OptRepr::RefSentinel)
            }
            IrType::Tuple(_) | IrType::Record { .. } | IrType::Fun { .. } => {
                Some(OptRepr::RefSentinel)
            }
            IrType::Named { module, name, args } => {
                if lang.is(Lang::Option, module, name) {
                    if args.len() != 1 {
                        return None;
                    }
                    let inner = Self::for_payload(lang, decls, &args[0])?;
                    return Some(OptRepr::Flag(OptPayload::Nested(Box::new(inner))));
                }
                if lang.is(Lang::Errors, module, name) || lang.is(Lang::ServiceImpl, module, name) {
                    return None;
                }
                if let Some(info) = decls.get(&(module.clone(), name.clone()))
                    && info.decl.alias.is_some()
                {
                    return None;
                }
                if args.is_empty() {
                    if lang.is(Lang::Int, module, name) {
                        return Some(OptRepr::Flag(OptPayload::Int));
                    }
                    if lang.is(Lang::Float, module, name) {
                        return Some(OptRepr::Flag(OptPayload::Float));
                    }
                    if lang.is(Lang::String, module, name) {
                        return Some(OptRepr::Flag(OptPayload::String));
                    }
                    if lang.is(Lang::Bool, module, name) {
                        return Some(OptRepr::BoolSentinel);
                    }
                    if lang.is(Lang::Unknown, module, name)
                        || lang.is(Lang::JsValue, module, name)
                        || lang.is(Lang::Never, module, name)
                    {
                        return Some(OptRepr::Flag(OptPayload::Value));
                    }
                }
                if NON_NIL_LANG.iter().any(|&l| lang.is(l, module, name)) {
                    return Some(OptRepr::RefSentinel);
                }
                if let Some(info) = decls.get(&(module.clone(), name.clone()))
                    && !info.decl.constructors.is_empty()
                    && info.decl.constructors.iter().all(|c| c.rep != Rep::Unboxed)
                {
                    return Some(OptRepr::RefSentinel);
                }
                Some(OptRepr::Flag(OptPayload::Value))
            }
        }
    }

    pub(super) fn go_type(&self) -> String {
        match self {
            OptRepr::BoolSentinel => "flickrt.OptBool".to_owned(),
            OptRepr::RefSentinel => "flickrt.OptRef".to_owned(),
            OptRepr::Flag(p) => format!("flickrt.Opt[{}]", p.go_type()),
        }
    }

    /// The Go type of the payload carried inside `Some`, or `None` for `flickrt.Value`.
    pub(super) fn payload_go_type(&self) -> Option<GoType> {
        match self {
            OptRepr::BoolSentinel => Some(GoType::Scalar(Native::Bool)),
            OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => None,
            OptRepr::Flag(OptPayload::Int) => Some(GoType::Scalar(Native::Int)),
            OptRepr::Flag(OptPayload::Float) => Some(GoType::Scalar(Native::Float)),
            OptRepr::Flag(OptPayload::String) => Some(GoType::Scalar(Native::String)),
            OptRepr::Flag(OptPayload::Nested(inner)) => Some(GoType::Opt((**inner).clone())),
        }
    }

    pub(super) fn none_code<'a>(&self, a: &'a DocumentArena<'a>) -> Code<'a> {
        match self {
            OptRepr::BoolSentinel => Code::leaf(a, "flickrt.OptBoolNone"),
            OptRepr::RefSentinel => Code::leaf(a, "flickrt.NoneRef"),
            OptRepr::Flag(p) => {
                let ty = a.alloc_str(&p.go_type());
                docvec![a, "flickrt.Opt[", ty, "]{}"].into()
            }
        }
    }

    pub(super) fn some_code<'a>(
        &self,
        a: &'a DocumentArena<'a>,
        payload: Document<'a>,
    ) -> Code<'a> {
        match self {
            OptRepr::BoolSentinel => call(a, "flickrt.SomeBool", [payload]).into(),
            OptRepr::RefSentinel => call(a, "flickrt.SomeRef", [payload]).into(),
            OptRepr::Flag(p) => {
                let ty = a.alloc_str(&p.go_type());
                docvec![a, "flickrt.Opt[", ty, "]{Val: ", payload, ", Ok: true}"].into()
            }
        }
    }

    pub(super) fn is_some_doc<'a>(
        &self,
        a: &'a DocumentArena<'a>,
        subj: Document<'a>,
    ) -> Document<'a> {
        match self {
            OptRepr::BoolSentinel | OptRepr::RefSentinel => docvec![a, subj, ".IsSome()"],
            OptRepr::Flag(_) => docvec![a, subj, ".Ok"],
        }
    }

    pub(super) fn is_none_doc<'a>(
        &self,
        a: &'a DocumentArena<'a>,
        subj: Document<'a>,
    ) -> Document<'a> {
        match self {
            OptRepr::BoolSentinel | OptRepr::RefSentinel => docvec![a, subj, ".IsNone()"],
            OptRepr::Flag(_) => docvec![a, "!", subj, ".Ok"],
        }
    }

    pub(super) fn unwrap_doc<'a>(
        &self,
        a: &'a DocumentArena<'a>,
        subj: Document<'a>,
    ) -> Document<'a> {
        match self {
            OptRepr::BoolSentinel | OptRepr::RefSentinel => docvec![a, subj, ".Unwrap()"],
            OptRepr::Flag(_) => docvec![a, subj, ".Val"],
        }
    }

    pub(super) fn box_doc<'a>(&self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        match self {
            OptRepr::BoolSentinel => call(a, "flickrt.BoxOptBool", [v]),
            OptRepr::RefSentinel => call(a, "flickrt.BoxOptRef", [v]),
            OptRepr::Flag(OptPayload::Int) => call(a, "flickrt.BoxOptInt", [v]),
            OptRepr::Flag(OptPayload::Float) => call(a, "flickrt.BoxOptFloat", [v]),
            OptRepr::Flag(OptPayload::String) => call(a, "flickrt.BoxOptString", [v]),
            OptRepr::Flag(OptPayload::Value) => call(a, "flickrt.BoxOptValue", [v]),
            OptRepr::Flag(OptPayload::Nested(inner)) => {
                call(a, "flickrt.BoxOpt", [v, inner.box_fn_doc(a)])
            }
        }
    }

    fn box_fn_doc<'a>(&self, a: &'a DocumentArena<'a>) -> Document<'a> {
        match self {
            OptRepr::BoolSentinel => a.text("flickrt.BoxOptBool"),
            OptRepr::RefSentinel => a.text("flickrt.BoxOptRef"),
            OptRepr::Flag(OptPayload::Int) => a.text("flickrt.BoxOptInt"),
            OptRepr::Flag(OptPayload::Float) => a.text("flickrt.BoxOptFloat"),
            OptRepr::Flag(OptPayload::String) => a.text("flickrt.BoxOptString"),
            OptRepr::Flag(OptPayload::Value) => a.text("flickrt.BoxOptValue"),
            OptRepr::Flag(OptPayload::Nested(_)) => {
                let ty = a.alloc_str(&self.go_type());
                docvec![
                    a,
                    "func(v ",
                    ty,
                    ") flickrt.Value { return ",
                    self.box_doc(a, a.text("v")),
                    " }"
                ]
            }
        }
    }

    pub(super) fn unbox_doc<'a>(&self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        match self {
            OptRepr::BoolSentinel => call(a, "flickrt.AsOptBool", [v]),
            OptRepr::RefSentinel => call(a, "flickrt.AsOptRef", [v]),
            OptRepr::Flag(OptPayload::Int) => call(a, "flickrt.AsOptInt", [v]),
            OptRepr::Flag(OptPayload::Float) => call(a, "flickrt.AsOptFloat", [v]),
            OptRepr::Flag(OptPayload::String) => call(a, "flickrt.AsOptString", [v]),
            OptRepr::Flag(OptPayload::Value) => call(a, "flickrt.AsOptValue", [v]),
            OptRepr::Flag(OptPayload::Nested(inner)) => {
                call(a, "flickrt.UnboxOpt", [v, inner.unbox_fn_doc(a)])
            }
        }
    }

    fn unbox_fn_doc<'a>(&self, a: &'a DocumentArena<'a>) -> Document<'a> {
        match self {
            OptRepr::BoolSentinel => a.text("flickrt.AsOptBool"),
            OptRepr::RefSentinel => a.text("flickrt.AsOptRef"),
            OptRepr::Flag(OptPayload::Int) => a.text("flickrt.AsOptInt"),
            OptRepr::Flag(OptPayload::Float) => a.text("flickrt.AsOptFloat"),
            OptRepr::Flag(OptPayload::String) => a.text("flickrt.AsOptString"),
            OptRepr::Flag(OptPayload::Value) => a.text("flickrt.AsOptValue"),
            OptRepr::Flag(OptPayload::Nested(_)) => {
                let ty = a.alloc_str(&self.go_type());
                docvec![
                    a,
                    "func(v flickrt.Value) ",
                    ty,
                    " { return ",
                    self.unbox_doc(a, a.text("v")),
                    " }"
                ]
            }
        }
    }

    pub(super) fn is_go_comparable(&self) -> bool {
        match self {
            OptRepr::BoolSentinel | OptRepr::Flag(OptPayload::Int | OptPayload::String) => true,
            OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Float | OptPayload::Value) => false,
            OptRepr::Flag(OptPayload::Nested(inner)) => inner.is_go_comparable(),
        }
    }

    pub(super) fn eq_doc<'a>(
        &self,
        a: &'a DocumentArena<'a>,
        lhs: Document<'a>,
        rhs: Document<'a>,
    ) -> Document<'a> {
        if self.is_go_comparable() {
            return docvec![a, "(", lhs, " == ", rhs, ")"];
        }
        match self {
            OptRepr::RefSentinel => call(a, "flickrt.EqOptRef", [lhs, rhs]),
            OptRepr::Flag(OptPayload::Float) => call(a, "flickrt.EqOptFloat", [lhs, rhs]),
            OptRepr::Flag(OptPayload::Value) => call(a, "flickrt.EqOptValue", [lhs, rhs]),
            OptRepr::Flag(OptPayload::Nested(inner)) => {
                call(a, "flickrt.EqOpt", [lhs, rhs, inner.eq_fn_doc(a)])
            }
            OptRepr::BoolSentinel | OptRepr::Flag(OptPayload::Int | OptPayload::String) => {
                unreachable!("handled by is_go_comparable")
            }
        }
    }

    fn eq_fn_doc<'a>(&self, a: &'a DocumentArena<'a>) -> Document<'a> {
        match self {
            OptRepr::RefSentinel => a.text("flickrt.EqOptRef"),
            OptRepr::Flag(OptPayload::Float) => a.text("flickrt.EqOptFloat"),
            OptRepr::Flag(OptPayload::Value) => a.text("flickrt.EqOptValue"),
            _ => {
                let ty = a.alloc_str(&self.go_type());
                docvec![
                    a,
                    "func(a, b ",
                    ty,
                    ") bool { return ",
                    self.eq_doc(a, a.text("a"), a.text("b")),
                    " }"
                ]
            }
        }
    }

    pub(super) fn contains_value(&self) -> bool {
        match self {
            OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => true,
            OptRepr::BoolSentinel
            | OptRepr::Flag(OptPayload::Int | OptPayload::Float | OptPayload::String) => false,
            OptRepr::Flag(OptPayload::Nested(inner)) => inner.contains_value(),
        }
    }

    pub(super) fn has_pointers(&self) -> bool {
        match self {
            OptRepr::RefSentinel | OptRepr::Flag(OptPayload::String | OptPayload::Value) => true,
            OptRepr::BoolSentinel | OptRepr::Flag(OptPayload::Int | OptPayload::Float) => false,
            OptRepr::Flag(OptPayload::Nested(inner)) => inner.has_pointers(),
        }
    }

    pub(super) fn size_align(&self) -> (usize, usize) {
        match self {
            OptRepr::BoolSentinel => (1, 1),
            OptRepr::RefSentinel => (16, 8),
            OptRepr::Flag(p) => {
                let (sz, al) = p.size_align();
                (align_up(sz + 1, al), al)
            }
        }
    }
}
