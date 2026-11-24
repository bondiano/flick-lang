//! Type classification and Go conversion code generation for direct Go
//! package bindings under `flick.go-ffi@0.1.0`.

use flick_backend_support::Known;
use flick_ir::{EnumLit, IrType, Lang, Rep, TypeRef};
use flick_pretty::{Document, DocumentArena, docvec};

use super::arith::Native;
use super::go_type::{GoType, OptPayload, OptRepr, TypeDecls};
use super::layout::call;

/// Defensive depth cap for unfolding aliases and nested types.
const MAX_DEPTH: usize = 32;

/// Built-in lang items explicitly outside the `flick.go-ffi@0.1.0` table.
const UNSUPPORTED_LANG: &[Lang] = &[
    Lang::Never,
    Lang::BigInt,
    Lang::Int8,
    Lang::Int16,
    Lang::Int64,
    Lang::UInt8,
    Lang::UInt16,
    Lang::UInt32,
    Lang::UInt64,
    Lang::Float32,
    Lang::Unknown,
    Lang::JsValue,
    Lang::Result,
];

/// A Flick type classified into its direct Go value-ABI representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GoAbiType {
    Unit,
    Bool,
    Int,
    Float,
    String,
    Bytes,
    List(Box<GoAbiType>),
    /// Go pointer `*T`: used ONLY where `nil` unambiguously represents `None`.
    OptionPtr(Box<GoAbiType>),
    /// Presence-flag representation (`(T, bool)` at top-level return,
    /// `struct{ Val T; Ok bool }` inside parameters, slices, or nested options).
    OptionFlag(Box<GoAbiType>),
    /// Opaque nominal handle or polymorphic variable (`flickrt.Value`).
    Opaque,
}

impl GoAbiType {
    /// Whether `*T` unambiguously distinguishes `None` (`nil`) from every valid
    /// `Some` payload without colliding with an inner sentinel.
    pub(super) fn ptr_unambiguous(&self) -> bool {
        matches!(
            self,
            Self::Unit
                | Self::Bool
                | Self::Int
                | Self::Float
                | Self::String
                | Self::Bytes
                | Self::List(_)
        )
    }

    /// Static Go type spelling of this ABI value.
    pub(super) fn go_type(&self) -> String {
        match self {
            Self::Unit => "struct{}".to_owned(),
            Self::Bool => "bool".to_owned(),
            Self::Int => "int32".to_owned(),
            Self::Float => "float64".to_owned(),
            Self::String => "string".to_owned(),
            Self::Bytes => "[]byte".to_owned(),
            Self::List(elem) => format!("[]{}", elem.go_type()),
            Self::OptionPtr(inner) => format!("*{}", inner.go_type()),
            Self::OptionFlag(inner) => format!("struct{{ Val {}; Ok bool }}", inner.go_type()),
            Self::Opaque => "flickrt.Value".to_owned(),
        }
    }

    /// Classifies `ty` into its `flick.go-ffi@0.1.0` representation, or `None`
    /// if `ty` lies outside the Go ABI table.
    pub(super) fn classify(
        lang: &Known,
        decls: &TypeDecls,
        ty: &IrType,
        depth: usize,
    ) -> Option<Self> {
        if depth >= MAX_DEPTH {
            return None;
        }
        match ty {
            IrType::Forall { body, .. } => Self::classify(lang, decls, body, depth + 1),
            IrType::Var(_) => Some(Self::Opaque),
            IrType::Named { module, name, args } => {
                Self::classify_named(lang, decls, module, name, args, depth)
            }
            IrType::Tuple(_)
            | IrType::Record { .. }
            | IrType::Fun { .. }
            | IrType::App { .. }
            | IrType::Exists { .. } => None,
        }
    }

    fn classify_named(
        lang: &Known,
        decls: &TypeDecls,
        module: &str,
        name: &str,
        args: &[TypeRef],
        depth: usize,
    ) -> Option<Self> {
        if lang.is(Lang::Unit, module, name) {
            return Some(Self::Unit);
        }
        if lang.is(Lang::Bool, module, name) {
            return Some(Self::Bool);
        }
        if lang.is(Lang::Int, module, name) {
            return Some(Self::Int);
        }
        if lang.is(Lang::Float, module, name) {
            return Some(Self::Float);
        }
        if lang.is(Lang::String, module, name) {
            return Some(Self::String);
        }
        if lang.is(Lang::Bytes, module, name) {
            return Some(Self::Bytes);
        }
        if lang.is(Lang::List, module, name) {
            let [elem] = args else { return None };
            return Some(Self::List(Box::new(Self::classify(
                lang,
                decls,
                elem,
                depth + 1,
            )?)));
        }
        if lang.is(Lang::Option, module, name) {
            let [inner] = args else { return None };
            let inner_abi = Self::classify(lang, decls, inner, depth + 1)?;
            return Some(if inner_abi.ptr_unambiguous() {
                Self::OptionPtr(Box::new(inner_abi))
            } else {
                Self::OptionFlag(Box::new(inner_abi))
            });
        }
        if UNSUPPORTED_LANG
            .iter()
            .any(|&item| lang.is(item, module, name))
        {
            return None;
        }
        let Some(decl) = decls.get(&(module.to_owned(), name.to_owned())) else {
            return Some(Self::Opaque);
        };
        if let Some(alias) = &decl.alias {
            let instantiated = subst_params(alias, &decl.params, args);
            return Self::classify(lang, decls, &instantiated, depth + 1);
        }
        if decl.error {
            return None;
        }
        if decl.constructors.is_empty() {
            return Some(Self::Opaque);
        }
        if let [ctor] = decl.constructors.as_slice()
            && ctor.rep == Rep::Unboxed
            && let [field] = ctor.fields.as_slice()
        {
            let instantiated = subst_params(field, &ctor.type_params, args);
            return Self::classify(lang, decls, &instantiated, depth + 1);
        }
        if decl
            .constructors
            .iter()
            .all(|c| matches!(c.rep, Rep::Literal(EnumLit::String(_))))
        {
            return Some(Self::String);
        }
        if decl
            .constructors
            .iter()
            .all(|c| matches!(c.rep, Rep::Literal(EnumLit::Int(_))))
        {
            return Some(Self::Int);
        }
        None
    }
}

fn subst_params(ty: &IrType, params: &[String], args: &[TypeRef]) -> IrType {
    if params.is_empty() || args.is_empty() {
        return ty.clone();
    }
    match ty {
        IrType::Var(v) => params
            .iter()
            .zip(args.iter())
            .find(|(p, _)| *p == v)
            .map_or_else(|| ty.clone(), |(_, arg)| (**arg).clone()),
        IrType::Named {
            module,
            name,
            args: inner,
        } => IrType::Named {
            module: module.clone(),
            name: name.clone(),
            args: inner
                .iter()
                .map(|a| TypeRef::new(subst_params(a, params, args)))
                .collect(),
        },
        IrType::Forall {
            vars,
            constraints,
            body,
        } => {
            let filtered: Vec<(String, TypeRef)> = params
                .iter()
                .zip(args.iter())
                .filter(|(p, _)| !vars.contains(p))
                .map(|(p, a)| (p.clone(), a.clone()))
                .collect();
            let (fp, fa): (Vec<_>, Vec<_>) = filtered.into_iter().unzip();
            IrType::Forall {
                vars: vars.clone(),
                constraints: constraints.clone(),
                body: TypeRef::new(subst_params(body, &fp, &fa)),
            }
        }
        _ => ty.clone(),
    }
}

/// Converts a Flick expression `doc` (of static Go type `flick_ty`, or erased
/// `flickrt.Value` when `None`) into the Go host value of type `abi.go_type()`.
pub(super) fn flick_to_go<'a>(
    a: &'a DocumentArena<'a>,
    abi: &GoAbiType,
    flick_ty: Option<&GoType>,
    doc: Document<'a>,
) -> Document<'a> {
    match abi {
        GoAbiType::Unit => a.text("struct{}{}"),
        GoAbiType::Bool => match flick_ty {
            Some(GoType::Scalar(Native::Bool)) => doc,
            _ => call(a, "flickrt.AsBool", [doc]),
        },
        GoAbiType::Int => match flick_ty {
            Some(GoType::Scalar(Native::Int)) => doc,
            _ => call(a, "flickrt.AsInt", [doc]),
        },
        GoAbiType::Float => match flick_ty {
            Some(GoType::Scalar(Native::Float)) => doc,
            _ => call(a, "flickrt.AsFloat", [doc]),
        },
        GoAbiType::String => match flick_ty {
            Some(GoType::Scalar(Native::String)) => doc,
            _ => call(a, "flickrt.AsString", [doc]),
        },
        GoAbiType::Bytes => call(a, "flickrt.BytesToGo", [doc]),
        GoAbiType::List(elem) => call(a, "flickrt.ListToGo", [doc, erased_to_go_fn(a, elem)]),
        GoAbiType::OptionPtr(inner) => match flick_ty {
            Some(GoType::Opt(OptRepr::BoolSentinel)) => call(a, "flickrt.OptBoolToPtr", [doc]),
            Some(GoType::Opt(OptRepr::Flag(
                OptPayload::Int | OptPayload::Float | OptPayload::String,
            ))) => call(a, "flickrt.OptToPtr", [doc]),
            Some(GoType::Opt(OptRepr::RefSentinel)) => {
                call(a, "flickrt.OptRefToPtr", [doc, erased_to_go_fn(a, inner)])
            }
            Some(GoType::Opt(OptRepr::Flag(OptPayload::Value))) => {
                call(a, "flickrt.OptValueToPtr", [doc, erased_to_go_fn(a, inner)])
            }
            _ => call(
                a,
                "flickrt.ErasedOptToPtr",
                [doc, erased_to_go_fn(a, inner)],
            ),
        },
        GoAbiType::OptionFlag(inner) => match flick_ty {
            Some(GoType::Opt(OptRepr::Flag(OptPayload::Nested(inner_repr)))) => {
                let inner_gt = GoType::Opt((**inner_repr).clone());
                call(
                    a,
                    "flickrt.OptFlagMap",
                    [doc, typed_to_go_fn(a, inner, Some(&inner_gt))],
                )
            }
            Some(GoType::Opt(OptRepr::Flag(OptPayload::Value))) => {
                call(a, "flickrt.OptFlagMap", [doc, erased_to_go_fn(a, inner)])
            }
            Some(GoType::Opt(OptRepr::RefSentinel)) => {
                call(a, "flickrt.OptRefToFlag", [doc, erased_to_go_fn(a, inner)])
            }
            Some(GoType::Opt(OptRepr::BoolSentinel)) => call(a, "flickrt.OptBoolToFlag", [doc]),
            Some(GoType::Opt(OptRepr::Flag(
                OptPayload::Int | OptPayload::Float | OptPayload::String,
            ))) => {
                let scalar_ty = a.alloc_str(&inner.go_type());
                let id_fn = docvec![a, "func(v ", scalar_ty, ") ", scalar_ty, " { return v }"];
                call(a, "flickrt.OptFlagMap", [doc, id_fn])
            }
            _ => call(
                a,
                "flickrt.ErasedOptToFlag",
                [doc, erased_to_go_fn(a, inner)],
            ),
        },
        GoAbiType::Opaque => match flick_ty {
            Some(GoType::Opt(repr)) => repr.box_doc(a, doc),
            _ => doc,
        },
    }
}

fn typed_to_go_fn<'a>(
    a: &'a DocumentArena<'a>,
    abi: &GoAbiType,
    flick_ty: Option<&GoType>,
) -> Document<'a> {
    let Some(gt) = flick_ty else {
        return erased_to_go_fn(a, abi);
    };
    let in_ty = a.alloc_str(&gt.go_type());
    let out_ty = a.alloc_str(&abi.go_type());
    let body = flick_to_go(a, abi, Some(gt), a.text("v"));
    docvec![a, "func(v ", in_ty, ") ", out_ty, " { return ", body, " }"]
}

/// A Go function expression of type `func(flickrt.Value) T` converting an erased
/// Flick value to `abi.go_type()`.
pub(super) fn erased_to_go_fn<'a>(a: &'a DocumentArena<'a>, abi: &GoAbiType) -> Document<'a> {
    match abi {
        GoAbiType::Unit => a.text("flickrt.AsUnit"),
        GoAbiType::Bool => a.text("flickrt.AsBool"),
        GoAbiType::Int => a.text("flickrt.AsInt"),
        GoAbiType::Float => a.text("flickrt.AsFloat"),
        GoAbiType::String => a.text("flickrt.AsString"),
        GoAbiType::Bytes => a.text("flickrt.BytesToGo"),
        GoAbiType::Opaque => a.text("flickrt.IdentityValue"),
        GoAbiType::List(_) | GoAbiType::OptionPtr(_) | GoAbiType::OptionFlag(_) => {
            let out_ty = a.alloc_str(&abi.go_type());
            let body = flick_to_go(a, abi, None, a.text("v"));
            docvec![
                a,
                "func(v flickrt.Value) ",
                out_ty,
                " { return ",
                body,
                " }"
            ]
        }
    }
}

/// Converts a Go value `doc` (of static type `abi.go_type()`) into a Flick value
/// of target Go type `want` (`None` means erased `flickrt.Value`).
pub(super) fn go_to_flick<'a>(
    a: &'a DocumentArena<'a>,
    abi: &GoAbiType,
    want: Option<&GoType>,
    doc: Document<'a>,
) -> Document<'a> {
    match abi {
        GoAbiType::Unit => a.text("flickrt.Unit"),
        GoAbiType::Bool
        | GoAbiType::Int
        | GoAbiType::Float
        | GoAbiType::String
        | GoAbiType::Opaque => doc,
        GoAbiType::Bytes => call(a, "flickrt.BytesFromGo", [doc]),
        GoAbiType::List(elem) => call(a, "flickrt.ListFromGo", [doc, go_to_erased_fn(a, elem)]),
        GoAbiType::OptionPtr(inner) => match want {
            Some(GoType::Opt(OptRepr::BoolSentinel)) => call(a, "flickrt.PtrToOptBool", [doc]),
            Some(GoType::Opt(OptRepr::Flag(
                OptPayload::Int | OptPayload::Float | OptPayload::String,
            ))) => call(a, "flickrt.PtrToOpt", [doc]),
            Some(GoType::Opt(OptRepr::RefSentinel)) => {
                call(a, "flickrt.PtrToOptRef", [doc, go_to_erased_fn(a, inner)])
            }
            Some(GoType::Opt(OptRepr::Flag(OptPayload::Value))) => {
                call(a, "flickrt.PtrToOptValue", [doc, go_to_erased_fn(a, inner)])
            }
            _ => call(
                a,
                "flickrt.PtrToErasedOpt",
                [doc, go_to_erased_fn(a, inner)],
            ),
        },
        GoAbiType::OptionFlag(inner) => {
            let inner_go = a.alloc_str(&inner.go_type());
            match want {
                Some(GoType::Opt(OptRepr::RefSentinel)) => {
                    let conv = go_to_erased_fn(a, inner);
                    call(
                        a,
                        docvec![a, "flickrt.StructToOptRef[", inner_go, "]"],
                        [doc, conv],
                    )
                }
                Some(GoType::Opt(OptRepr::Flag(OptPayload::Nested(inner_repr)))) => {
                    let inner_opt = a.alloc_str(&inner_repr.go_type());
                    let nested_type = GoType::Opt((**inner_repr).clone());
                    let conv = go_to_typed_fn(a, inner, Some(&nested_type));
                    call(
                        a,
                        docvec![a, "flickrt.StructToOptMap[", inner_go, ", ", inner_opt, "]"],
                        [doc, conv],
                    )
                }
                Some(GoType::Opt(OptRepr::Flag(OptPayload::Value))) => {
                    let conv = go_to_erased_fn(a, inner);
                    call(
                        a,
                        docvec![a, "flickrt.StructToOptMap[", inner_go, ", flickrt.Value]"],
                        [doc, conv],
                    )
                }
                _ => {
                    let conv = go_to_erased_fn(a, inner);
                    call(
                        a,
                        docvec![a, "flickrt.StructToErasedOpt[", inner_go, "]"],
                        [doc, conv],
                    )
                }
            }
        }
    }
}

/// Converts a Go `(r0, r1)` comma-ok return pair (where `r0` has ABI type `inner`
/// and `r1` is `bool`) into a Flick `Option` of target Go type `want`.
pub(super) fn comma_ok_to_flick<'a>(
    a: &'a DocumentArena<'a>,
    inner: &GoAbiType,
    want: Option<&GoType>,
    r0: Document<'a>,
    r1: Document<'a>,
) -> Document<'a> {
    match want {
        Some(GoType::Opt(OptRepr::BoolSentinel)) => call(a, "flickrt.FlagToOptBool", [r0, r1]),
        Some(GoType::Opt(OptRepr::Flag(
            OptPayload::Int | OptPayload::Float | OptPayload::String,
        ))) => call(a, "flickrt.FlagToOpt", [r0, r1]),
        Some(GoType::Opt(OptRepr::RefSentinel)) => call(
            a,
            "flickrt.FlagToOptRef",
            [r0, r1, go_to_erased_fn(a, inner)],
        ),
        Some(GoType::Opt(OptRepr::Flag(OptPayload::Value))) => call(
            a,
            "flickrt.FlagToOptMap",
            [r0, r1, go_to_erased_fn(a, inner)],
        ),
        Some(GoType::Opt(OptRepr::Flag(OptPayload::Nested(inner_repr)))) => {
            let inner_gt = GoType::Opt((**inner_repr).clone());
            call(
                a,
                "flickrt.FlagToOptMap",
                [r0, r1, go_to_typed_fn(a, inner, Some(&inner_gt))],
            )
        }
        _ => call(
            a,
            "flickrt.FlagToErasedOpt",
            [r0, r1, go_to_erased_fn(a, inner)],
        ),
    }
}

fn go_to_typed_fn<'a>(
    a: &'a DocumentArena<'a>,
    abi: &GoAbiType,
    want: Option<&GoType>,
) -> Document<'a> {
    let Some(gt) = want else {
        return go_to_erased_fn(a, abi);
    };
    let in_ty = a.alloc_str(&abi.go_type());
    let out_ty = a.alloc_str(&gt.go_type());
    let body = go_to_flick(a, abi, Some(gt), a.text("v"));
    docvec![a, "func(v ", in_ty, ") ", out_ty, " { return ", body, " }"]
}

/// A Go function expression of type `func(T) flickrt.Value` converting `abi.go_type()`
/// into an erased Flick value.
pub(super) fn go_to_erased_fn<'a>(a: &'a DocumentArena<'a>, abi: &GoAbiType) -> Document<'a> {
    match abi {
        GoAbiType::Unit => a.text("flickrt.FromUnit"),
        GoAbiType::Bool => a.text("flickrt.FromBool"),
        GoAbiType::Int => a.text("flickrt.FromInt"),
        GoAbiType::Float => a.text("flickrt.FromFloat"),
        GoAbiType::String => a.text("flickrt.FromString"),
        GoAbiType::Bytes => a.text("flickrt.BytesFromGo"),
        GoAbiType::Opaque => a.text("flickrt.IdentityValue"),
        GoAbiType::List(_) | GoAbiType::OptionPtr(_) | GoAbiType::OptionFlag(_) => {
            let in_ty = a.alloc_str(&abi.go_type());
            let body = go_to_flick(a, abi, None, a.text("v"));
            docvec![a, "func(v ", in_ty, ") flickrt.Value { return ", body, " }"]
        }
    }
}
