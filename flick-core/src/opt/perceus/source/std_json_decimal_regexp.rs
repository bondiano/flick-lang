//! Audited `Json`, `JsonPointer`, `JsonPatch`, `Differ`, `Inspectable`,
//! `Formatter`, `Version`, `BigDecimal`, and `RegExp` foreign adapters.

use flick_ir::memory::{BigDecimalIntrinsic, JsonIntrinsic, RegExpIntrinsic};
use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_json_decimal_regexp_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();

        // `JsonPointer.parse_uri_fragment` uses `nullable` result adaptation (`Option (List String)`).
        if from == "flick:JsonPointer"
            && bind == "parseUriFragment"
            && value.mode == ForeignMode::Pure
            && stages == 1
            && value.catch.is_none()
            && value.adapter.words.is_empty()
            && value.adapter.optional.is_empty()
            && value.adapter.owned.is_empty()
            && value.adapter.result.nullable
        {
            return Some(self.intrinsic(
                Intrinsic::Json(JsonIntrinsic::ParseUriFragment),
                false,
                vec![],
                block,
                span,
            ));
        }

        if !has_standard_shape(value) {
            return None;
        }

        // `Version.current : Unit ->{} String` uses `ForeignMode::Sync` with `arity 0`.
        if from == "flick:Version"
            && bind == "getCurrentVersion"
            && value.mode == ForeignMode::Sync
            && stages == 1
        {
            return Some(self.intrinsic(
                Intrinsic::Json(JsonIntrinsic::VersionCurrent),
                false,
                vec![],
                block,
                span,
            ));
        }

        if value.mode != ForeignMode::Pure {
            return None;
        }

        if let Some(kind) = direct_intrinsic(from, bind, stages) {
            return Some(self.intrinsic(kind, false, vec![], block, span));
        }

        let b = ExprBuilder::new(span, "$jdr");
        let expr = pure_template(&b, from, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn direct_intrinsic(from: &str, bind: &str, stages: usize) -> Option<Intrinsic> {
    let kind = match (from, bind) {
        ("flick:rt", "jsonParse") => Intrinsic::Json(JsonIntrinsic::Parse),
        ("flick:rt", "jsonStringify") | ("flick:Formatter", "format") => {
            Intrinsic::Json(JsonIntrinsic::Stringify)
        }
        ("flick:Formatter", "flickFormatJson") => Intrinsic::Json(JsonIntrinsic::FormatJson),
        ("flick:Inspectable", "flickToStringUnknown") => {
            Intrinsic::Json(JsonIntrinsic::InspectToString)
        }
        ("flick:JsonPointer", "escapeToken") => Intrinsic::Json(JsonIntrinsic::EscapeToken),
        ("flick:JsonPointer", "unescapeToken") => Intrinsic::Json(JsonIntrinsic::UnescapeToken),
        ("flick:JsonPointer", "formatUriFragment") => {
            Intrinsic::Json(JsonIntrinsic::FormatUriFragment)
        }
        ("flick:JsonPatch", "flickGet") => Intrinsic::Json(JsonIntrinsic::PatchGet),
        ("flick:JsonPatch", "flickApply") => Intrinsic::Json(JsonIntrinsic::PatchApply),
        ("flick:rt", "decimalFromString") => Intrinsic::BigDecimal(BigDecimalIntrinsic::FromString),
        ("flick:rt", "decimalFromBigInt") => Intrinsic::BigDecimal(BigDecimalIntrinsic::FromBigInt),
        ("flick:rt", "decimalFromInteger") => {
            Intrinsic::BigDecimal(BigDecimalIntrinsic::FromInteger)
        }
        ("flick:rt", "decimalFromNumber") => Intrinsic::BigDecimal(BigDecimalIntrinsic::FromNumber),
        ("flick:rt", "decimalToString") => Intrinsic::BigDecimal(BigDecimalIntrinsic::ToString),
        ("flick:rt", "decimalNormalString") => {
            Intrinsic::BigDecimal(BigDecimalIntrinsic::ToStringNormal)
        }
        ("flick:rt", "decimalScale") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Scale),
        ("flick:rt", "decimalAdd") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Add),
        ("flick:rt", "decimalSubtract") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Subtract),
        ("flick:rt", "decimalMultiply") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Multiply),
        ("flick:rt", "decimalDivideToScale") => {
            Intrinsic::BigDecimal(BigDecimalIntrinsic::DivideToScale)
        }
        ("flick:rt", "decimalCompare") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Compare),
        ("flick:BigDecimal", "normalize") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Normalize),
        ("flick:BigDecimal", "flickScale") => Intrinsic::BigDecimal(BigDecimalIntrinsic::SetScale),
        ("flick:BigDecimal", "sumAll") => Intrinsic::BigDecimal(BigDecimalIntrinsic::SumAll),
        ("flick:BigDecimal", "multiplyAll") => {
            Intrinsic::BigDecimal(BigDecimalIntrinsic::MultiplyAll)
        }
        ("flick:BigDecimal", "divide") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Divide),
        ("flick:BigDecimal", "remainder") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Remainder),
        ("flick:BigDecimal", "flickSign") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Sign),
        ("flick:BigDecimal", "abs") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Abs),
        ("flick:BigDecimal", "negate") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Negate),
        ("flick:BigDecimal", "toExponential") => {
            Intrinsic::BigDecimal(BigDecimalIntrinsic::ToExponential)
        }
        ("flick:BigDecimal", "isInteger") => Intrinsic::BigDecimal(BigDecimalIntrinsic::IsInteger),
        ("flick:BigDecimal", "isZero") => Intrinsic::BigDecimal(BigDecimalIntrinsic::IsZero),
        ("flick:BigDecimal", "isNegative") => {
            Intrinsic::BigDecimal(BigDecimalIntrinsic::IsNegative)
        }
        ("flick:BigDecimal", "isPositive") => {
            Intrinsic::BigDecimal(BigDecimalIntrinsic::IsPositive)
        }
        ("flick:BigDecimal", "flickRound") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Round),
        ("flick:BigDecimal", "flickTruncate") => {
            Intrinsic::BigDecimal(BigDecimalIntrinsic::Truncate)
        }
        ("flick:BigDecimal", "flickCeil") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Ceil),
        ("flick:BigDecimal", "flickFloor") => Intrinsic::BigDecimal(BigDecimalIntrinsic::Floor),
        ("flick:RegExp", "flickMake") => Intrinsic::RegExp(RegExpIntrinsic::Make),
        ("flick:RegExp", "escape") => Intrinsic::RegExp(RegExpIntrinsic::Escape),
        ("flick:RegExp", "flickMatch") => Intrinsic::RegExp(RegExpIntrinsic::MatchFirst),
        ("flick:RegExp", "flickMatchAll") => Intrinsic::RegExp(RegExpIntrinsic::MatchAll),
        ("flick:RegExp", "flickReplaceAll") => Intrinsic::RegExp(RegExpIntrinsic::ReplaceAll),
        ("flick:String", "search") => Intrinsic::RegExp(RegExpIntrinsic::Search),
        ("flick:String", "replace") => Intrinsic::RegExp(RegExpIntrinsic::Replace),
        _ => return None,
    };
    (kind.arity() == stages).then_some(kind)
}

fn pure_template(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    if from == "flick:Inspectable" && bind == "toJson" && stages == 1 {
        return Some(b.lam(&["x"], b.loc("x")));
    }
    if from == "flick:Differ" {
        return differ_template(b, bind, stages);
    }
    if from == "flick:BigDecimal" {
        return decimal_order_template(b, bind, stages);
    }
    None
}

fn differ_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let dcon = || ExprBuilder::con("Differ", "Differ", "$Differ", 4);
    let unwrap = |d: Expr, body: Expr| {
        b.match_(
            d,
            vec![arm(
                Pat::Con(
                    dcon(),
                    vec![
                        Pat::Bind(b.name("de")),
                        Pat::Bind(b.name("dd")),
                        Pat::Bind(b.name("dc")),
                        Pat::Bind(b.name("dp")),
                    ],
                ),
                body,
            )],
        )
    };
    match (bind, stages) {
        ("flickEmpty", 1) => Some(b.lam(&["d"], unwrap(b.loc("d"), b.loc("de")))),
        ("flickDiff", 3) => Some(b.lam(
            &["old", "new", "d"],
            unwrap(b.loc("d"), b.app2(b.loc("dd"), b.loc("old"), b.loc("new"))),
        )),
        ("flickCombine", 3) => Some(b.lam(
            &["first", "second", "d"],
            unwrap(
                b.loc("d"),
                b.app2(b.loc("dc"), b.loc("first"), b.loc("second")),
            ),
        )),
        ("flickPatch", 3) => Some(b.lam(
            &["old", "p", "d"],
            unwrap(b.loc("d"), b.app2(b.loc("dp"), b.loc("old"), b.loc("p"))),
        )),
        _ => None,
    }
}

fn decimal_order_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let cmp = |x: Expr, y: Expr| b.app2(b.global("BigDecimal", "compare_decimal"), x, y);
    match (bind, stages) {
        ("min", 2) => Some(b.lam(
            &["that", "self"],
            b.if_(
                b.bin(BinOp::Eq, cmp(b.loc("self"), b.loc("that")), b.ord("GT")),
                b.loc("that"),
                b.loc("self"),
            ),
        )),
        ("max", 2) => Some(b.lam(
            &["that", "self"],
            b.if_(
                b.bin(BinOp::Eq, cmp(b.loc("self"), b.loc("that")), b.ord("LT")),
                b.loc("that"),
                b.loc("self"),
            ),
        )),
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
            b.if_(
                b.bin(
                    BinOp::Eq,
                    cmp(b.loc("self"), b.field(b.loc("bounds"), "minimum")),
                    b.ord("LT"),
                ),
                b.field(b.loc("bounds"), "minimum"),
                b.if_(
                    b.bin(
                        BinOp::Eq,
                        cmp(b.loc("self"), b.field(b.loc("bounds"), "maximum")),
                        b.ord("GT"),
                    ),
                    b.field(b.loc("bounds"), "maximum"),
                    b.loc("self"),
                ),
            ),
        )),
        _ => None,
    }
}
