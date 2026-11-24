//! Audited `Int`, `Float`, `BigInt`, and `Flick.Debug` foreign adapters.

use std::f64::consts::PI;

use flick_ir::memory::{
    BigIntIntrinsic, DebugIntrinsic, FixedWidthIntrinsic, FloatIntrinsic, IntIntrinsic,
};
use flick_ir::{Arith, Foreign, ForeignMode, Num};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::{
    BinOp, Code, Env, Expr, Intrinsic, Lit, Local, Lower, LowerError, Name, Operation, Span, Stmt,
};

impl Lower<'_> {
    pub(super) fn core_scalar_foreign(
        &mut self,
        value: &Foreign,
        name: &Name,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = match &value.binding {
            flick_ir::Binding::Module { from, name } => (Some(from.as_str()), name.as_str()),
            flick_ir::Binding::Global { name } => (None, name.as_str()),
            flick_ir::Binding::Inline(_) => return None,
        };
        // Zero-argument Float constants (`Math.PI`, `Infinity`, `NaN`).
        if from.is_none() && value.arity == 0 && value.stages.is_empty() {
            let constant = match bind {
                "Math.PI" => Some(PI),
                "Infinity" => Some(f64::INFINITY),
                "NaN" => Some(f64::NAN),
                _ => None,
            };
            if let Some(c) = constant {
                return Some(block.emit(Operation::Literal(Lit::Float(c)), span));
            }
        }
        // `BigInt.to_string` binds global `"String"` specifically on module `BigInt`.
        if from.is_none()
            && bind == "String"
            && name.module == "BigInt"
            && name.name == "to_string"
            && value.stages.len() == 1
        {
            return Some(self.intrinsic(
                Intrinsic::BigInt(BigIntIntrinsic::ToString),
                false,
                vec![],
                block,
                span,
            ));
        }
        if let Some(kind) = scalar_intrinsic(from, bind, value.stages.len()) {
            return Some(self.intrinsic(kind, false, vec![], block, span));
        }
        let expr = scalar_template(from, bind, value.stages.len(), span)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn scalar_intrinsic(from: Option<&str>, bind: &str, stages: usize) -> Option<Intrinsic> {
    let kind = match (from, bind) {
        (Some("flick:rt"), "intParse") => Intrinsic::Int(IntIntrinsic::Parse),
        (Some("flick:rt"), "intFromFloat") => Intrinsic::Int(IntIntrinsic::FromFloat),
        (Some("flick:rt"), "intTruncate") => Intrinsic::Int(IntIntrinsic::Truncate),
        (Some("flick:rt"), "intAbs") => Intrinsic::Int(IntIntrinsic::Abs),
        (Some("flick:rt"), "intCheckedAdd") => Intrinsic::Int(IntIntrinsic::CheckedAdd),
        (Some("flick:rt"), "intCheckedSub") => Intrinsic::Int(IntIntrinsic::CheckedSub),
        (Some("flick:rt"), "intCheckedMul") => Intrinsic::Int(IntIntrinsic::CheckedMul),
        (Some("flick:rt"), "intCheckedDiv") => Intrinsic::Int(IntIntrinsic::CheckedDiv),
        (Some("flick:rt"), "intCheckedRem") => Intrinsic::Int(IntIntrinsic::CheckedRem),
        (Some("flick:rt"), "bitAnd") => Intrinsic::Int(IntIntrinsic::BitAnd),
        (Some("flick:rt"), "bitOr") => Intrinsic::Int(IntIntrinsic::BitOr),
        (Some("flick:rt"), "bitXor") => Intrinsic::Int(IntIntrinsic::BitXor),
        (Some("flick:rt"), "bitNot") => Intrinsic::Int(IntIntrinsic::BitNot),
        (Some("flick:rt"), "shiftLeft") => Intrinsic::Int(IntIntrinsic::ShiftLeft),
        (Some("flick:rt"), "shiftRight") => Intrinsic::Int(IntIntrinsic::ShiftRight),
        (Some("flick:rt"), "shiftRightUnsignedInt") => {
            Intrinsic::Int(IntIntrinsic::ShiftRightUnsigned)
        }
        (Some("flick:rt"), "showInt64") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ShowInt64),
        (Some("flick:rt"), "showUInt64") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ShowUInt64),
        (Some("flick:rt"), "toInt8") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToInt8),
        (Some("flick:rt"), "toInt16") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToInt16),
        (Some("flick:rt"), "toInt32") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToInt32),
        (Some("flick:rt"), "toInt64") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToInt64),
        (Some("flick:rt"), "toUInt8") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToUInt8),
        (Some("flick:rt"), "toUInt16") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToUInt16),
        (Some("flick:rt"), "toUInt32") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToUInt32),
        (Some("flick:rt"), "toUInt64") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToUInt64),
        (Some("flick:rt"), "toFloat32") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToFloat32),
        (Some("flick:rt"), "toFloat64") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToFloat64),
        (Some("flick:rt"), "toBigInt") => Intrinsic::FixedWidth(FixedWidthIntrinsic::ToBigInt),
        (Some("flick:rt"), "int64BitAnd") => {
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64BitAnd)
        }
        (Some("flick:rt"), "int64BitOr") => Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64BitOr),
        (Some("flick:rt"), "int64BitXor") => {
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64BitXor)
        }
        (Some("flick:rt"), "int64BitNot") => {
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64BitNot)
        }
        (Some("flick:rt"), "int64ShiftLeft") => {
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64ShiftLeft)
        }
        (Some("flick:rt"), "int64ShiftRight") => {
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64ShiftRight)
        }
        (Some("flick:rt"), "int64ShiftRightUnsigned") => {
            Intrinsic::FixedWidth(FixedWidthIntrinsic::Int64ShiftRightUnsigned)
        }
        (None, "Number.isNaN") => Intrinsic::Float(FloatIntrinsic::IsNaN),
        (None, "Number.isFinite") => Intrinsic::Float(FloatIntrinsic::IsFinite),
        (None, "Number.isInteger") => Intrinsic::Float(FloatIntrinsic::IsInteger),
        (None, "Number.isSafeInteger") => Intrinsic::Float(FloatIntrinsic::IsSafeInteger),
        (None, "Math.abs") => Intrinsic::Float(FloatIntrinsic::Abs),
        (None, "Math.floor") => Intrinsic::Float(FloatIntrinsic::Floor),
        (None, "Math.ceil") => Intrinsic::Float(FloatIntrinsic::Ceil),
        (None, "Math.round") => Intrinsic::Float(FloatIntrinsic::Round),
        (None, "Math.trunc") => Intrinsic::Float(FloatIntrinsic::Trunc),
        (None, "Math.sqrt") => Intrinsic::Float(FloatIntrinsic::Sqrt),
        (Some("flick:Number"), "parse") => Intrinsic::Float(FloatIntrinsic::Parse),
        (Some("flick:Number"), "remainder") => Intrinsic::Float(FloatIntrinsic::Remainder),
        (Some("flick:Number"), "nextPow2") => Intrinsic::Float(FloatIntrinsic::NextPow2),
        (Some("flick:Number"), "round") => Intrinsic::Float(FloatIntrinsic::RoundTo),
        (Some("flick:rt"), "numberSafeInteger") => Intrinsic::Float(FloatIntrinsic::SafeInteger),
        (Some("flick:rt"), "stringFromNumber") => Intrinsic::Float(FloatIntrinsic::ToString),
        (Some("flick:rt"), "stringToNumber") => Intrinsic::Float(FloatIntrinsic::StringToNumber),
        (Some("flick:rt"), "bigintFromNumber") => Intrinsic::BigInt(BigIntIntrinsic::FromNumber),
        (Some("flick:BigInt"), "fromNumber") => Intrinsic::BigInt(BigIntIntrinsic::FromSafeNumber),
        (Some("flick:BigInt"), "toNumber") => Intrinsic::BigInt(BigIntIntrinsic::ToSafeNumber),
        (Some("flick:rt"), "bigintParse") => Intrinsic::BigInt(BigIntIntrinsic::Parse),
        (Some("flick:BigInt"), "fromString") => Intrinsic::BigInt(BigIntIntrinsic::FromString),
        (Some("flick:BigInt"), "abs") => Intrinsic::BigInt(BigIntIntrinsic::Abs),
        (Some("flick:BigInt"), "sqrt") => Intrinsic::BigInt(BigIntIntrinsic::Sqrt),
        (Some("flick:rt"), "bigintDivide") => Intrinsic::BigInt(BigIntIntrinsic::Divide),
        (Some("flick:rt"), "bigintRemainder") => Intrinsic::BigInt(BigIntIntrinsic::Remainder),
        (Some("flick:BigInt"), "gcd") => Intrinsic::BigInt(BigIntIntrinsic::Gcd),
        (Some("flick:BigInt"), "lcm") => Intrinsic::BigInt(BigIntIntrinsic::Lcm),
        (Some("flick:rt"), "flickTodo") => Intrinsic::Debug(DebugIntrinsic::Todo),
        (Some("flick:rt"), "flickEcho") => Intrinsic::Debug(DebugIntrinsic::Echo),
        (Some("flick:rt"), "flickAssert") => Intrinsic::Debug(DebugIntrinsic::Assert),
        _ => return None,
    };
    (kind.arity() == stages).then_some(kind)
}

fn scalar_lit(b: &ExprBuilder, num: Num, one: bool) -> Expr {
    match (num, one) {
        (Num::Float, false) => b.float(0.0),
        (Num::Float, true) => b.float(1.0),
        (Num::Float32, false) => b.at(flick_ir::ExprKind::Lit(Lit::Float32(0.0))),
        (Num::Float32, true) => b.at(flick_ir::ExprKind::Lit(Lit::Float32(1.0))),
        (Num::BigInt, false) => b.bigint("0"),
        (Num::BigInt, true) => b.bigint("1"),
        (Num::Int, false) => b.int(0),
        (Num::Int, true) => b.int(1),
        (Num::Int8, false) => b.at(flick_ir::ExprKind::Lit(Lit::Int8(0))),
        (Num::Int8, true) => b.at(flick_ir::ExprKind::Lit(Lit::Int8(1))),
        (Num::Int16, false) => b.at(flick_ir::ExprKind::Lit(Lit::Int16(0))),
        (Num::Int16, true) => b.at(flick_ir::ExprKind::Lit(Lit::Int16(1))),
        (Num::Int64, false) => b.at(flick_ir::ExprKind::Lit(Lit::Int64(0))),
        (Num::Int64, true) => b.at(flick_ir::ExprKind::Lit(Lit::Int64(1))),
        (Num::UInt8, false) => b.at(flick_ir::ExprKind::Lit(Lit::UInt8(0))),
        (Num::UInt8, true) => b.at(flick_ir::ExprKind::Lit(Lit::UInt8(1))),
        (Num::UInt16, false) => b.at(flick_ir::ExprKind::Lit(Lit::UInt16(0))),
        (Num::UInt16, true) => b.at(flick_ir::ExprKind::Lit(Lit::UInt16(1))),
        (Num::UInt32, false) => b.at(flick_ir::ExprKind::Lit(Lit::UInt32(0))),
        (Num::UInt32, true) => b.at(flick_ir::ExprKind::Lit(Lit::UInt32(1))),
        (Num::UInt64, false) => b.at(flick_ir::ExprKind::Lit(Lit::UInt64(0))),
        (Num::UInt64, true) => b.at(flick_ir::ExprKind::Lit(Lit::UInt64(1))),
    }
}

fn scalar_template(from: Option<&str>, bind: &str, stages: usize, span: Span) -> Option<Expr> {
    let num = match from {
        Some("flick:Number") => Num::Float,
        Some("flick:BigInt") => Num::BigInt,
        _ => return None,
    };
    let b = ExprBuilder::new(span, "$num");
    scalar_arith(&b, num, bind, stages).or_else(|| scalar_order(&b, num, bind, stages))
}

fn scalar_arith(b: &ExprBuilder, num: Num, bind: &str, stages: usize) -> Option<Expr> {
    let zero = || scalar_lit(b, num, false);
    let one = || scalar_lit(b, num, true);
    match (bind, stages) {
        ("sum", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::Arith(Arith::Add, num), b.loc("self"), b.loc("that")),
        )),
        ("subtract", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::Arith(Arith::Sub, num), b.loc("self"), b.loc("that")),
        )),
        ("multiply", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::Arith(Arith::Mul, num), b.loc("self"), b.loc("that")),
        )),
        ("divide", 2) if num == Num::Float => Some(b.lam(
            &["that", "self"],
            b.if_(
                b.bin(BinOp::Eq, b.loc("that"), b.float(0.0)),
                b.none(),
                b.some(b.bin(
                    BinOp::Arith(Arith::Div, Num::Float),
                    b.loc("self"),
                    b.loc("that"),
                )),
            ),
        )),
        ("increment", 1) => Some(b.lam(
            &["n"],
            b.bin(BinOp::Arith(Arith::Add, num), b.loc("n"), one()),
        )),
        ("decrement", 1) => Some(b.lam(
            &["n"],
            b.bin(BinOp::Arith(Arith::Sub, num), b.loc("n"), one()),
        )),
        ("sumAll", 1) => Some(b.lam(
            &["xs"],
            b.block(
                vec![
                    b.var_decl("acc", zero()),
                    Stmt::Expr(b.each(
                        "x",
                        b.loc("xs"),
                        b.block(
                            vec![b.assign(
                                "acc",
                                b.bin(BinOp::Arith(Arith::Add, num), b.var("acc"), b.loc("x")),
                            )],
                            b.unit(),
                        ),
                    )),
                ],
                b.var("acc"),
            ),
        )),
        ("multiplyAll", 1) => Some(b.lam(
            &["xs"],
            b.block(
                vec![
                    b.var_decl("acc", one()),
                    Stmt::Expr(b.for_ctrl(
                        "$mul_all",
                        "x",
                        b.loc("xs"),
                        b.if_(
                            b.bin(BinOp::Eq, b.loc("x"), zero()),
                            b.block(
                                vec![b.assign("acc", zero())],
                                b.break_("$mul_all", b.unit()),
                            ),
                            b.block(
                                vec![b.assign(
                                    "acc",
                                    b.bin(BinOp::Arith(Arith::Mul, num), b.var("acc"), b.loc("x")),
                                )],
                                b.unit(),
                            ),
                        ),
                    )),
                ],
                b.var("acc"),
            ),
        )),
        _ => None,
    }
}

fn scalar_order(b: &ExprBuilder, num: Num, bind: &str, stages: usize) -> Option<Expr> {
    let cmp = |l: Expr, r: Expr| b.app2(b.global("Instances", "compare_prim"), l, r);
    match (bind, stages) {
        ("isLessThan", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::Eq, cmp(b.loc("self"), b.loc("that")), b.ord("LT")),
        )),
        ("isLessThanOrEqualTo", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::NotEq, cmp(b.loc("self"), b.loc("that")), b.ord("GT")),
        )),
        ("isGreaterThan", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::Eq, cmp(b.loc("self"), b.loc("that")), b.ord("GT")),
        )),
        ("isGreaterThanOrEqualTo", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::NotEq, cmp(b.loc("self"), b.loc("that")), b.ord("LT")),
        )),
        ("min", 2) if num == Num::BigInt => Some(b.lam(
            &["that", "self"],
            b.if_(
                b.bin(BinOp::NotEq, cmp(b.loc("self"), b.loc("that")), b.ord("GT")),
                b.loc("self"),
                b.loc("that"),
            ),
        )),
        ("max", 2) if num == Num::BigInt => Some(b.lam(
            &["that", "self"],
            b.if_(
                b.bin(BinOp::NotEq, cmp(b.loc("self"), b.loc("that")), b.ord("LT")),
                b.loc("self"),
                b.loc("that"),
            ),
        )),
        ("between", 2) => Some(b.lam(
            &["bounds", "self"],
            b.if_(
                b.bin(
                    BinOp::NotEq,
                    cmp(b.loc("self"), b.field(b.loc("bounds"), "minimum")),
                    b.ord("LT"),
                ),
                b.bin(
                    BinOp::NotEq,
                    cmp(b.loc("self"), b.field(b.loc("bounds"), "maximum")),
                    b.ord("GT"),
                ),
                b.bool_(false),
            ),
        )),
        ("clamp", 2) => Some(b.lam(
            &["bounds", "self"],
            b.block(
                vec![
                    b.let_("lo", b.field(b.loc("bounds"), "minimum")),
                    b.let_("hi", b.field(b.loc("bounds"), "maximum")),
                    b.let_(
                        "raised",
                        b.if_(
                            b.bin(BinOp::Eq, cmp(b.loc("self"), b.loc("lo")), b.ord("LT")),
                            b.loc("lo"),
                            b.loc("self"),
                        ),
                    ),
                ],
                b.if_(
                    b.bin(BinOp::Eq, cmp(b.loc("raised"), b.loc("hi")), b.ord("GT")),
                    b.loc("hi"),
                    b.loc("raised"),
                ),
            ),
        )),
        ("flickSign", 1) => Some(b.lam(&["n"], cmp(b.loc("n"), scalar_lit(b, num, false)))),
        _ => None,
    }
}
