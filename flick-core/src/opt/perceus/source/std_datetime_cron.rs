//! Audited `DateTime` and `Cron` foreign adapters (`packages/std/src/{DateTime,Cron}.fl`).

use flick_ir::memory::{CronIntrinsic, DateTimeIntrinsic};
use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::std_codec::{REFUSE_FALLIBLE, leaf_codec};
use super::std_schema_rt::{meta_con, rt, str_lit};
use super::{Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Span};

impl Lower<'_> {
    pub(super) fn std_datetime_cron_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();

        if from == "flick:DateTime"
            && bind == "zoneMakeLocal"
            && value.mode == ForeignMode::Sync
            && stages == 1
        {
            return Some(self.intrinsic(
                Intrinsic::DateTime(DateTimeIntrinsic::ZoneMakeLocal),
                false,
                vec![],
                block,
                span,
            ));
        }
        if value.mode != ForeignMode::Pure {
            return None;
        }
        if from == "flick:Cron" {
            let op = cron_intrinsic(bind, stages)?;
            return Some(self.intrinsic(Intrinsic::Cron(op), false, vec![], block, span));
        }
        if from != "flick:DateTime" {
            return None;
        }
        if let Some(op) = datetime_intrinsic(bind, stages) {
            return Some(self.intrinsic(Intrinsic::DateTime(op), false, vec![], block, span));
        }
        let b = ExprBuilder::new(span, "$dt");
        let expr = datetime_template(&b, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn cron_intrinsic(bind: &str, stages: usize) -> Option<CronIntrinsic> {
    use CronIntrinsic as C;
    let op = match bind {
        "flickMake" => C::Make,
        "flickParse" => C::Parse,
        "flickFormat" => C::Format,
        "flickMatch" => C::Match,
        "flickNext" => C::Next,
        "flickPrev" => C::Prev,
        "flickSequence" => C::Sequence,
        "Equivalence" => C::Equals,
        _ => return None,
    };
    (op.arity() == stages).then_some(op)
}

fn datetime_intrinsic(bind: &str, stages: usize) -> Option<DateTimeIntrinsic> {
    use DateTimeIntrinsic as D;
    let op = match bind {
        "flickMake" => D::Make,
        "flickFromParts" => D::FromParts,
        "flickFromEpochSeconds" => D::FromEpochSeconds,
        "flickMakeZoned" => D::MakeZoned,
        "flickMakeZonedFromString" => D::MakeZonedFromString,
        "flickUnsafeFromEpochMillis" => D::UnsafeFromEpochMillis,
        "isUtc" => D::IsUtc,
        "isZoned" => D::IsZoned,
        "toUtc" => D::ToUtc,
        "flickSetZone" => D::SetZone,
        "flickSetZoneOffset" => D::SetZoneOffset,
        "flickSetZoneNamed" => D::SetZoneNamed,
        "zoneMakeOffset" => D::ZoneMakeOffset,
        "flickZoneMakeNamed" => D::ZoneMakeNamed,
        "flickZoneFromString" => D::ZoneFromString,
        "zoneToString" => D::ZoneToString,
        "toEpochMillis" => D::ToEpochMillis,
        "toEpochSeconds" => D::ToEpochSeconds,
        "toParts" => D::ToParts,
        "toPartsUtc" => D::ToPartsUtc,
        "flickSetParts" => D::SetParts,
        "flickSetPartsUtc" => D::SetPartsUtc,
        "removeTime" => D::RemoveTime,
        "flickZonedOffset" => D::ZonedOffset,
        "flickZonedOffsetIso" => D::ZonedOffsetIso,
        "flickAdd" => D::Add,
        "flickSubtract" => D::Subtract,
        "addDuration" => D::AddDuration,
        "subtractDuration" => D::SubtractDuration,
        "flickStartOf" => D::StartOf,
        "flickEndOf" => D::EndOf,
        "flickNearest" => D::Nearest,
        "flickDistance" => D::Distance,
        "flickCompare" => D::Compare,
        "flickFormat" => D::Format,
        "flickFormatLocal" => D::FormatLocal,
        "flickFormatUtc" => D::FormatUtc,
        "flickFormatIntl" => D::FormatIntl,
        "formatIso" => D::FormatIso,
        "formatIsoDate" => D::FormatIsoDate,
        "formatIsoDateUtc" => D::FormatIsoDateUtc,
        "formatIsoOffset" => D::FormatIsoOffset,
        "flickFormatIsoZoned" => D::FormatIsoZoned,
        _ => return None,
    };
    (op.arity() == stages).then_some(op)
}

#[expect(clippy::too_many_lines, reason = "central DateTime template dispatch")]
fn datetime_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let cmp = |x: Expr, y: Expr| b.app2(rt(b, "DateTime.Compare"), x, y);
    let rf = || meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]);
    match (bind, stages) {
        ("match", 2) => Some(b.lam(
            &["opts", "self"],
            b.if_(
                b.app(rt(b, "DateTime.IsUtc"), b.loc("self")),
                b.app(b.field(b.loc("opts"), "onUtc"), b.loc("self")),
                b.app(b.field(b.loc("opts"), "onZoned"), b.loc("self")),
            ),
        )),
        ("mapEpochMillis", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "DateTime.WithEpochMillis"),
                b.app(
                    b.loc("f"),
                    b.app(rt(b, "DateTime.ToEpochMillis"), b.loc("self")),
                ),
                b.loc("self"),
            ),
        )),
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
        ("flickJsonUtc", 0) => Some(leaf_codec(b, "datetime_utc", rf())),
        ("flickJsonUtcMillis", 0) => Some(leaf_codec(b, "datetime_utc_millis", rf())),
        ("flickJsonZoned", 0) => Some(leaf_codec(b, "datetime_zoned", rf())),
        ("flickJsonZone", 0) => Some(leaf_codec(b, "datetime_zone", rf())),
        ("flickJsonZoneNamed", 0) => Some(leaf_codec(b, "datetime_zone_named", rf())),
        ("flickJsonZoneOffset", 0) => Some(leaf_codec(
            b,
            "datetime_zone_offset",
            meta_con(b, "$Stage", vec![meta_con(b, "$Number", vec![])]),
        )),
        _ => None,
    }
}
