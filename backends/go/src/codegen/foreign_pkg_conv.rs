//! Go conversion code generation for direct Go package bindings under
//! `flick.go-ffi@0.1.0`.

use flick_pretty::{Document, DocumentArena, docvec};

use super::arith::Native;
pub(super) use super::foreign_pkg_abi::{GoAbiType, GoOpaqueType};
use super::go_type::{GoType, OptPayload, OptRepr};
use super::layout::call;

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
        GoAbiType::Opaque(opaque) => {
            let boxed = match flick_ty {
                Some(gt) => gt.box_if_needed(a, doc),
                None => doc,
            };
            if let Some(GoOpaqueType {
                resolved: Some(go_ty),
                ..
            }) = opaque
            {
                docvec![a, boxed, ".(", a.alloc_str(go_ty), ")"]
            } else {
                boxed
            }
        }
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
        GoAbiType::Opaque(Some(GoOpaqueType {
            resolved: Some(go_ty),
            ..
        })) => {
            let ty = a.alloc_str(go_ty);
            docvec![a, "func(v flickrt.Value) ", ty, " { return v.(", ty, ") }"]
        }
        GoAbiType::Opaque(_) => a.text("flickrt.IdentityValue"),
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
        | GoAbiType::Opaque(_) => doc,
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
        GoAbiType::Opaque(Some(GoOpaqueType {
            resolved: Some(go_ty),
            ..
        })) => {
            let ty = a.alloc_str(go_ty);
            docvec![a, "func(v ", ty, ") flickrt.Value { return v }"]
        }
        GoAbiType::Opaque(_) => a.text("flickrt.IdentityValue"),
        GoAbiType::List(_) | GoAbiType::OptionPtr(_) | GoAbiType::OptionFlag(_) => {
            let in_ty = a.alloc_str(&abi.go_type());
            let body = go_to_flick(a, abi, None, a.text("v"));
            docvec![a, "func(v ", in_ty, ") flickrt.Value { return ", body, " }"]
        }
    }
}
