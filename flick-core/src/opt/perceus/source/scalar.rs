//! Exact scalar host contracts; unknown globals and Number exports stay unsupported.
use super::Intrinsic;
use super::kept_for_call;
use crate::{
    ir::{BinOp, Expr, ExprKind, Param, Pat, Stage},
    source::Span,
};
use flick_ir::{Adapter, Binding, Copies, Foreign, ForeignMode, ResultAdapter, Shape};

pub(super) fn candidate(value: &Foreign) -> Option<Intrinsic> {
    let kind = match &value.binding {
        Binding::Global { name } if name == "Number" => Intrinsic::NumberToFloat,
        Binding::Global { name } if name == "BigInt" => Intrinsic::ToBigInt,
        Binding::Module { from, name } if from == "flick:Number" && name == "min" => {
            Intrinsic::FloatMin
        }
        Binding::Module { from, name } if from == "flick:Number" && name == "max" => {
            Intrinsic::FloatMax
        }
        _ => return None,
    };
    // Deep/opaque Number signatures may conservatively request an input copy.
    // This is ONLY candidate recognition: the caller must first prove the
    // immutable Int/BigInt/Float -> Float domain from checked declarations. Native
    // copyData is identity on those primitive representations, so that one
    // audited input-copy variant is discharged, not arbitrary FFI copying.
    let copy_input = matches!(kind, Intrinsic::NumberToFloat | Intrinsic::ToBigInt)
        && value.adapter.copies.args == [true];
    // Global BigInt's result is also immutable. Either copy bit is discharged
    // only after BOTH checked primitive-input and BigInt-output proofs.
    let copy_result = kind == Intrinsic::ToBigInt && value.adapter.copies.result;
    // Builtin Number inputs are Int/BigInt; checked Float is identity. Min/max take `that` at
    // host arity one and return a pure function taking `self` as source stage two.
    (value.mode == ForeignMode::Pure
        && value.dicts == 0
        && value.arity == 1
        && value.stages == vec![false; kind.arity()]
        && value.catch.is_none()
        && kept_for_call(&value.adapter)
            == Adapter {
                optional: vec![],
                owned: vec![],
                result: ResultAdapter {
                    nullable: false,
                    unit: false,
                },
                callbacks: vec![None],
                copies: Copies {
                    args: vec![copy_input],
                    result: copy_result,
                    callbacks: vec![None],
                },
                shapes: vec![Shape::Data],
                result_shape: Shape::Data,
                words: vec![],
                retention: None,
            })
    .then_some(kind)
}

/// A checked homogeneous Int domain needs no host call or new memory opcode.
/// Two pure source stages stay cold, and equality selects the second operand.
pub(super) fn int_selection(minimum: bool, span: Span) -> Expr {
    let at = |kind| Expr::new(kind, span);
    let local = |name: &str| at(ExprKind::Local(name.to_owned()));
    at(ExprKind::Lambda {
        params: ["$scalar.that", "$scalar.self"]
            .into_iter()
            .map(|name| Param {
                pat: Pat::Bind(name.to_owned()),
                stage: Stage::Pure,
                ty: None,
                binder_types: Vec::new(),
            })
            .collect(),
        body: Box::new(at(ExprKind::If(
            Box::new(at(ExprKind::Binary(
                if minimum { BinOp::Le } else { BinOp::Ge },
                Box::new(local("$scalar.self")),
                Box::new(local("$scalar.that")),
            ))),
            Box::new(local("$scalar.self")),
            Box::new(local("$scalar.that")),
        ))),
    })
}
