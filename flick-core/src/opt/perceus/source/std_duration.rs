//! Audited `Duration` foreign adapters.

use flick_ir::memory::DurationIntrinsic;
use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Operation, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_duration_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) || value.mode != ForeignMode::Pure {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();

        if stages == 0
            && let Some(kind) = zero_arg_intrinsic(from, bind)
        {
            return Some(block.emit(Operation::Intrinsic { kind, args: vec![] }, span));
        }
        if let Some(kind) = direct_pure_intrinsic(from, bind, stages) {
            return Some(self.intrinsic(kind, false, vec![], block, span));
        }
        if from == "flick:Duration" && matches!(bind, "match" | "flickMatchPair") {
            let expr = if bind == "match" && stages == 2 {
                duration_match_template(&ExprBuilder::new(span, "$dur"))
            } else if bind == "flickMatchPair" && stages == 3 {
                duration_match_pair_template(&ExprBuilder::new(span, "$dur"))
            } else {
                return None;
            };
            return Some(self.expr(&expr, &Env::new(), block).and_then(|function| {
                let argument = self.intrinsic(
                    Intrinsic::Duration(DurationIntrinsic::Deconstruct),
                    false,
                    vec![],
                    block,
                    span,
                )?;
                block.emit(Operation::Apply { function, argument }, span)
            }));
        }
        if from == "flick:Duration" {
            let expr = duration_order_template(&ExprBuilder::new(span, "$do"), bind, stages)?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        None
    }
}

fn zero_arg_intrinsic(from: &str, bind: &str) -> Option<Intrinsic> {
    match (from, bind) {
        ("flick:rt", "durationZero") => Some(Intrinsic::Duration(DurationIntrinsic::Zero)),
        ("flick:rt", "durationInfinity") => Some(Intrinsic::Duration(DurationIntrinsic::Infinity)),
        _ => None,
    }
}

fn direct_pure_intrinsic(from: &str, bind: &str, stages: usize) -> Option<Intrinsic> {
    let kind = match (from, bind) {
        ("flick:rt", "durationNanos") => Intrinsic::Duration(DurationIntrinsic::Nanos),
        ("flick:Duration", "flickMicros") => Intrinsic::Duration(DurationIntrinsic::Micros),
        ("flick:rt", "durationMillis") => Intrinsic::Duration(DurationIntrinsic::Millis),
        ("flick:rt", "durationSeconds") => Intrinsic::Duration(DurationIntrinsic::Seconds),
        ("flick:rt", "durationMinutes") => Intrinsic::Duration(DurationIntrinsic::Minutes),
        ("flick:Duration", "flickHours") => Intrinsic::Duration(DurationIntrinsic::Hours),
        ("flick:Duration", "flickDays") => Intrinsic::Duration(DurationIntrinsic::Days),
        ("flick:Duration", "flickWeeks") => Intrinsic::Duration(DurationIntrinsic::Weeks),
        ("flick:Duration", "flickFromInput") => Intrinsic::Duration(DurationIntrinsic::FromInput),
        ("flick:rt", "durationIsFinite") => Intrinsic::Duration(DurationIntrinsic::IsFinite),
        ("flick:Duration", "isZero") => Intrinsic::Duration(DurationIntrinsic::IsZero),
        ("flick:Duration", "isPositive") => Intrinsic::Duration(DurationIntrinsic::IsPositive),
        ("flick:rt", "durationToMillis") => Intrinsic::Duration(DurationIntrinsic::ToMillis),
        ("flick:Duration", "flickToSeconds") => Intrinsic::Duration(DurationIntrinsic::ToSeconds),
        ("flick:Duration", "flickToMinutes") => Intrinsic::Duration(DurationIntrinsic::ToMinutes),
        ("flick:Duration", "flickToHours") => Intrinsic::Duration(DurationIntrinsic::ToHours),
        ("flick:Duration", "flickToDays") => Intrinsic::Duration(DurationIntrinsic::ToDays),
        ("flick:Duration", "flickToWeeks") => Intrinsic::Duration(DurationIntrinsic::ToWeeks),
        ("flick:Duration", "toNanos") => Intrinsic::Duration(DurationIntrinsic::ToNanos),
        ("flick:Duration", "flickToHrTime") => Intrinsic::Duration(DurationIntrinsic::ToHrTime),
        ("flick:Duration", "parts") => Intrinsic::Duration(DurationIntrinsic::Parts),
        ("flick:rt", "durationAdd") => Intrinsic::Duration(DurationIntrinsic::Add),
        ("flick:Duration", "flickSubtract") => Intrinsic::Duration(DurationIntrinsic::Subtract),
        ("flick:Duration", "flickTimes") => Intrinsic::Duration(DurationIntrinsic::Times),
        ("flick:Duration", "flickDivide") => Intrinsic::Duration(DurationIntrinsic::Divide),
        ("flick:rt", "durationCompare") => Intrinsic::Duration(DurationIntrinsic::Compare),
        ("flick:rt", "durationFormat") => Intrinsic::Duration(DurationIntrinsic::Format),
        _ => return None,
    };
    (kind.arity() == stages).then_some(kind)
}

fn duration_order_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let cmp = |x: Expr, y: Expr| b.app2(b.global("Duration", "compare_duration"), x, y);
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

fn duration_match_template(b: &ExprBuilder) -> Expr {
    let m_pat = Pat::Con(
        ExprBuilder::con("Duration", "$Decon", "$Millis", 1),
        vec![Pat::Bind(b.name("ms"))],
    );
    let n_pat = Pat::Con(
        ExprBuilder::con("Duration", "$Decon", "$Nanos", 1),
        vec![Pat::Bind(b.name("ns"))],
    );
    b.lam(
        &["decon", "opts", "d"],
        b.match_(
            b.app(b.loc("decon"), b.loc("d")),
            vec![
                arm(
                    m_pat,
                    b.app(b.field(b.loc("opts"), "onMillis"), b.loc("ms")),
                ),
                arm(n_pat, b.app(b.field(b.loc("opts"), "onNanos"), b.loc("ns"))),
                arm(
                    Pat::Wild,
                    b.app(b.field(b.loc("opts"), "onInfinity"), b.unit()),
                ),
            ],
        ),
    )
}

fn duration_match_pair_template(b: &ExprBuilder) -> Expr {
    let m_pat = |n: &str| {
        Pat::Con(
            ExprBuilder::con("Duration", "$Decon", "$Millis", 1),
            vec![Pat::Bind(b.name(n))],
        )
    };
    let inf_pat = Pat::Con(
        ExprBuilder::con("Duration", "$Decon", "$Infinity", 0),
        vec![],
    );
    let to_ns = |d: Expr| {
        b.match_(
            b.app(b.global("Duration", "to_nanos"), d),
            vec![
                arm(b.some_pat("v"), b.loc("v")),
                arm(Pat::Wild, b.bigint("0")),
            ],
        )
    };
    b.lam(
        &["decon", "that", "opts", "self"],
        b.match_(
            b.tuple(vec![
                b.app(b.loc("decon"), b.loc("self")),
                b.app(b.loc("decon"), b.loc("that")),
            ]),
            vec![
                arm(
                    Pat::Tuple(vec![m_pat("a"), m_pat("b")]),
                    b.app2(b.field(b.loc("opts"), "onMillis"), b.loc("a"), b.loc("b")),
                ),
                arm(
                    Pat::Tuple(vec![inf_pat.clone(), Pat::Wild]),
                    b.app2(
                        b.field(b.loc("opts"), "onInfinity"),
                        b.loc("self"),
                        b.loc("that"),
                    ),
                ),
                arm(
                    Pat::Tuple(vec![Pat::Wild, inf_pat]),
                    b.app2(
                        b.field(b.loc("opts"), "onInfinity"),
                        b.loc("self"),
                        b.loc("that"),
                    ),
                ),
                arm(
                    Pat::Wild,
                    b.app2(
                        b.field(b.loc("opts"), "onNanos"),
                        to_ns(b.loc("self")),
                        to_ns(b.loc("that")),
                    ),
                ),
            ],
        ),
    )
}
