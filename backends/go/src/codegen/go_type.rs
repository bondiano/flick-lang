//! Typed Go representations beyond erased `flickrt.Value`: native Go scalars,
//! allocation-free concrete `Option` layouts, closed record value structs, and
//! by-value closed ADT structs.

use std::collections::HashMap;

use flick_backend_support::Known;
use flick_ir::{IrType, Module, TypeDecl};
use flick_pretty::{Document, DocumentArena};

use super::adt::AdtShape;
use super::arith::Native;
use super::layout::Code;
pub(super) use super::opt_repr::{OptCon, OptPayload, OptRepr};
use super::record::RecordShape;

/// Metadata for a module's `TypeDecl` needed for layout and tag computation.
#[derive(Debug, Clone, Copy)]
pub(super) struct TypeDeclInfo<'a> {
    pub(super) decl: &'a TypeDecl,
    pub(super) own: bool,
    pub(super) package: Option<&'a str>,
}

/// Type declarations indexed by `(module, type_name)`.
pub(super) type TypeDecls<'a> = HashMap<(String, String), TypeDeclInfo<'a>>;

pub(super) fn type_decls<'a>(own: &[&'a Module], builtins: &[&'a Module]) -> TypeDecls<'a> {
    let mut out = HashMap::new();
    for (is_own, slice) in [(true, own), (false, builtins)] {
        for m in slice {
            for decl in &m.types {
                out.entry((m.name.clone(), decl.name.clone()))
                    .or_insert(TypeDeclInfo {
                        decl,
                        own: is_own,
                        package: m.package.as_deref(),
                    });
            }
        }
    }
    out
}

/// A Go value representation more specific than erased `flickrt.Value`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum GoType {
    /// Native Go scalar (`int32`, `float64`, `bool`, `string`, fixed-width): already valid
    /// as a `flickrt.Value` without explicit conversion.
    Scalar(Native),
    /// Allocation-free concrete `Option`: must be converted to `*flickrt.Data`
    /// before entering a `flickrt.Value` slot.
    Opt(OptRepr),
    /// Closed record shape struct carried by value: implements `flickrt.RecordView`
    /// with value receivers, so already valid as a `flickrt.Value` without conversion.
    Record(RecordShape),
    /// Closed nonrecursive ADT (including concrete `Result`) carried by value:
    /// must be converted via `Box_A_*` before entering a `flickrt.Value` slot.
    Adt(AdtShape),
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
    /// An erased `flickrt.Value` (`Opt` and `Adt` are emitted or converted as erased values).
    Value,
    /// Any natural typed Go representation.
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

impl GoType {
    pub(super) fn as_scalar(&self) -> Option<Native> {
        match self {
            GoType::Scalar(n) => Some(*n),
            GoType::Opt(_) | GoType::Record(_) | GoType::Adt(_) => None,
        }
    }

    pub(super) fn needs_boxing(&self) -> bool {
        matches!(self, GoType::Opt(_) | GoType::Adt(_))
    }

    pub(super) fn of(lang: &Known, decls: &TypeDecls, ty: Option<&IrType>) -> Option<GoType> {
        if let Some(native) = Native::of(lang, ty) {
            return Some(GoType::Scalar(native));
        }
        if let Some(repr) = OptRepr::of(lang, decls, ty) {
            return Some(GoType::Opt(repr));
        }
        if let Some(shape) = RecordShape::of(lang, decls, ty) {
            return Some(GoType::Record(shape));
        }
        AdtShape::of(lang, decls, ty).map(GoType::Adt)
    }

    pub(super) fn go_type(&self) -> String {
        match self {
            GoType::Scalar(n) => n.go().to_owned(),
            GoType::Opt(r) => r.go_type(),
            GoType::Record(s) => s.go_type(),
            GoType::Adt(s) => s.go_type(),
        }
    }

    pub(super) fn unbox_doc<'a>(&self, a: &'a DocumentArena<'a>, v: Document<'a>) -> Document<'a> {
        match self {
            GoType::Scalar(n) => n.unbox(a, v),
            GoType::Opt(r) => r.unbox_doc(a, v),
            GoType::Record(s) => s.unbox_doc(a, v),
            GoType::Adt(s) => s.unbox_doc(a, v),
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
            GoType::Adt(s) => s.box_doc(a, v),
        }
    }
}
