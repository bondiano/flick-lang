//! Typed Go representations beyond erased `flickrt.Value`: native Go scalars
//! (`int32`, `float64`, `bool`, `string`) and allocation-free concrete `Option`
//! layouts (`flickrt.OptBool`, `flickrt.OptRef`, `flickrt.Opt[T]`).
//!
//! Unlike scalars, a typed `Option` has a distinct Go type from its erased
//! `*flickrt.Data` representation and MUST be converted via `box_doc` before
//! crossing into any `flickrt.Value` slot.

use std::collections::HashMap;

use flick_backend_support::Known;
use flick_ir::{IrType, Lang, Module, Rep, TypeDecl};
use flick_pretty::{Document, DocumentArena, docvec};

use super::arith::Native;
use super::layout::{Code, call};
use super::record::RecordShape;

/// Type declarations indexed by `(module, type_name)`.
pub(super) type TypeDecls<'a> = HashMap<(String, String), &'a TypeDecl>;

pub(super) fn type_decls<'a>(modules: &[&'a Module]) -> TypeDecls<'a> {
    let mut out = HashMap::new();
    for m in modules {
        for decl in &m.types {
            out.insert((m.name.clone(), decl.name.clone()), decl);
        }
    }
    out
}

/// Which `Option` constructor a [`Constructor`] is, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OptCon {
    Some,
    None,
}

/// The payload slot of a presence-flag `flickrt.Opt[T]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
}

/// The allocation-free Go layout of a concrete `Option T`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum OptRepr {
    /// 1-byte sentinel `flickrt.OptBool` (`0 = None`, `1 = Some(false)`, `2 = Some(true)`).
    BoolSentinel,
    /// `flickrt.OptRef` (`struct{ val flickrt.Value }`, `nil` sentinel for `None`).
    /// Used only when the concrete non-`Option` payload is guaranteed non-nil.
    RefSentinel,
    /// Presence-flag struct `flickrt.Opt[T]` (`{ Val T; Ok bool }`).
    Flag(OptPayload),
}

/// A Go value representation more specific than erased `flickrt.Value`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum GoType {
    /// Native Go scalar (`int32`, `float64`, `bool`, `string`): already valid
    /// as a `flickrt.Value` without explicit conversion.
    Scalar(Native),
    /// Allocation-free concrete `Option`: must be converted to `*flickrt.Data`
    /// before entering a `flickrt.Value` slot.
    Opt(OptRepr),
    /// Closed record shape struct carried by value: implements `flickrt.RecordView`
    /// with value receivers, so already valid as a `flickrt.Value` without conversion.
    Record(RecordShape),
}

/// A compiled Go expression together with its actual Go type (`None` means
/// erased `flickrt.Value`).
#[derive(Debug, Clone)]
pub(super) struct Typed<'a> {
    pub(super) code: Code<'a>,
    pub(super) ty: Option<GoType>,
}

/// What Go representation a caller wants from expression emission.
#[derive(Debug, Clone, Copy)]
pub(super) enum Want<'w> {
    /// An erased `flickrt.Value` (native scalars are already valid values;
    /// typed `Option` is emitted or converted as `*flickrt.Data`).
    Value,
    /// Any natural typed Go representation (preserving scalars and typed `Option`).
    Any,
    /// Exactly this typed Go representation.
    Exact(&'w GoType),
}

impl<'w> Want<'w> {
    pub(super) fn of(ty: Option<&'w GoType>) -> Self {
        match ty {
            Some(gt) => Want::Exact(gt),
            None => Want::Value,
        }
    }
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
    /// substitution `σ`, `of(σ(T)) == Some(r)`. Polymorphic `Option a` (and
    /// `Option (Option a)`) return `None` so erased polymorphic code keeps the
    /// tagged `*flickrt.Data` form.
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
            // Only constrained higher-rank values are wrapped in `*flickrt.Poly`
            // at runtime; an unconstrained `Forall` holds its body directly (which
            // may be `nil`, e.g. `forall a. Unknown`) and can agree with its
            // instantiated body under higher-rank instantiation.
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
                // `Errors` agrees with error ADTs/`Never` and `ServiceImpl` agrees
                // with records; keep them erased so equivalent types never get
                // conflicting typed `Option` layouts.
                if lang.is(Lang::Errors, module, name) || lang.is(Lang::ServiceImpl, module, name) {
                    return None;
                }
                // An `opaque type alias` unifies transparently with its target
                // inside its defining module; stay erased to preserve substitution
                // invariance and handle recursive aliases without unfolding.
                if let Some(decl) = decls.get(&(module.clone(), name.clone()))
                    && decl.alias.is_some()
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
                if let Some(decl) = decls.get(&(module.clone(), name.clone()))
                    && !decl.constructors.is_empty()
                    && decl.constructors.iter().all(|c| c.rep != Rep::Unboxed)
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

    /// Converts a typed `Option` expression into the erased `*flickrt.Data` representation.
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

    /// Converts an erased `*flickrt.Data` `Option` into this typed Go layout.
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

    /// Whether Go's built-in `==` matches Flick's `SameValueZero` structural
    /// equality on this typed `Option` representation.
    pub(super) fn is_go_comparable(&self) -> bool {
        match self {
            OptRepr::BoolSentinel | OptRepr::Flag(OptPayload::Int | OptPayload::String) => true,
            OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Float | OptPayload::Value) => false,
            OptRepr::Flag(OptPayload::Nested(inner)) => inner.is_go_comparable(),
        }
    }

    /// Structural equality (`==`) on two values of this typed `Option` layout.
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
}

impl GoType {
    pub(super) fn as_scalar(&self) -> Option<Native> {
        match self {
            GoType::Scalar(n) => Some(*n),
            GoType::Opt(_) | GoType::Record(_) => None,
        }
    }

    pub(super) fn of(lang: &Known, decls: &TypeDecls, ty: Option<&IrType>) -> Option<GoType> {
        if let Some(native) = Native::of(lang, ty) {
            return Some(GoType::Scalar(native));
        }
        if let Some(repr) = OptRepr::of(lang, decls, ty) {
            return Some(GoType::Opt(repr));
        }
        RecordShape::of(lang, decls, ty).map(GoType::Record)
    }

    pub(super) fn go_type(&self) -> String {
        match self {
            GoType::Scalar(n) => n.go().to_owned(),
            GoType::Opt(r) => r.go_type(),
            GoType::Record(s) => s.go_type(),
        }
    }

    pub(super) fn unbox_doc<'a>(&self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        match self {
            GoType::Scalar(n) => n.unbox(a, v),
            GoType::Opt(r) => r.unbox_doc(a, v),
            GoType::Record(s) => s.unbox_doc(a, v),
        }
    }

    pub(super) fn box_if_needed<'a>(
        &self,
        a: &'a DocumentArena<'a>,
        v: Document<'a>,
    ) -> Document<'a> {
        match self {
            GoType::Scalar(_) | GoType::Record(_) => v,
            GoType::Opt(r) => r.box_doc(a, v),
        }
    }
}
