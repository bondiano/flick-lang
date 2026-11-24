//! Audited `Metric` base constructors, registry lookup, and series hooks (Stage 4D).

mod hooks;
mod registry;
mod summary;

use flick_ir::{Arith, BinOp, Foreign, ForeignMode, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_metric_ops::{effect_track_template, metric_ops_template};
use super::std_observe_refs::{obs_read, upsert_pair_list};
use super::{Code, Expr, ExprKind, Local, Lower, LowerError, Pat, Span};

pub(super) use registry::{make_metric_expr, merge_attrs_expr, sort_pairs_expr};

pub(super) const KEY_METRIC_ATTRS: &str = "effect/Metric/CurrentMetricAttributes";
pub(super) const KEY_RUNTIME_METRICS: &str = "effect/Metric/RuntimeEnabled";

impl Lower<'_> {
    pub(super) fn std_metric_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = value.binding.module_export()?;
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$mt");
        let expr = match from {
            "flick:Metric" => metric_base_template(&b, bind, value.mode, stages)
                .or_else(|| metric_ops_template(&b, bind, value.mode, stages))?,
            "flick:Effect" if value.mode == ForeignMode::Effect => {
                effect_track_template(&b, bind, stages)?
            }
            _ => return None,
        };
        Some(self.lower_with_manual_scope(&expr, "$mt", block, span))
    }
}

pub(super) fn fadd(b: &ExprBuilder, l: Expr, r: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Add, Num::Float), l, r)
}

pub(super) fn fsub(b: &ExprBuilder, l: Expr, r: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Sub, Num::Float), l, r)
}

pub(super) fn fmul(b: &ExprBuilder, l: Expr, r: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Mul, Num::Float), l, r)
}

pub(super) fn current_metric_attrs(b: &ExprBuilder) -> Expr {
    obs_read(b, KEY_METRIC_ATTRS, b.list(vec![]))
}

fn summary_template(b: &ExprBuilder) -> Expr {
    let stamped = || {
        b.tuple([
            b.loc("x"),
            b.app(b.global("Clock", "current_time_millis"), b.unit()),
        ])
    };
    let wrapped = b.tuple([
        b.loc("bg"),
        b.lam(&["ex", "x"], b.app2(b.loc("bu"), b.loc("ex"), stamped())),
        b.lam(&["ex", "x"], b.app2(b.loc("bm"), b.loc("ex"), stamped())),
    ]);
    let base_call = b.app2(
        b.global("Metric", "summary_with_timestamp"),
        b.loc("id"),
        b.loc("o"),
    );
    b.lam(
        &["id", "o"],
        b.block(
            [b.let_("base", base_call)],
            b.match_(
                b.loc("base"),
                [arm(b.tup_pat(&["bg", "bu", "bm"]), wrapped)],
            ),
        ),
    )
}

fn timer_template(b: &ExprBuilder) -> Expr {
    let default_exp = b.app(
        b.global("Metric", "exponential_boundaries"),
        b.at(ExprKind::Record(vec![
            ("start".into(), b.float(0.5)),
            ("factor".into(), b.float(2.0)),
            ("count".into(), b.int(35)),
        ])),
    );
    let bnds = b.match_(
        b.field(b.loc("o"), "boundaries"),
        [
            arm(b.some_pat("bs"), b.loc("bs")),
            arm(Pat::Wild, default_exp),
        ],
    );
    let attrs = upsert_pair_list(
        b,
        b.field(b.loc("o"), "attributes"),
        b.str_("time_unit"),
        b.str_("milliseconds"),
    );
    let hist = make_metric_expr(
        b,
        b.loc("id"),
        "Histogram",
        b.field(b.loc("o"), "description"),
        b.loc("attrs"),
        hooks::histogram_hooks(b, b.loc("bnds")),
    );
    let to_ms = b.lam(
        &["d"],
        b.match_(
            b.app(b.global("Duration", "to_millis"), b.loc("d")),
            [
                arm(b.some_pat("ms"), b.loc("ms")),
                arm(Pat::Wild, b.global("Float", "infinity")),
            ],
        ),
    );
    b.lam(
        &["id", "o"],
        b.block(
            [
                b.let_("bnds", bnds),
                b.let_("attrs", attrs),
                b.let_("h", hist),
            ],
            b.app2(b.global("Metric", "map_input"), to_ms, b.loc("h")),
        ),
    )
}

fn metric_base_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode != ForeignMode::Pure || stages != 2 {
        return None;
    }
    match bind {
        "flickCounter" => Some(b.lam(
            &["id", "o"],
            make_metric_expr(
                b,
                b.loc("id"),
                "Counter",
                b.field(b.loc("o"), "description"),
                b.field(b.loc("o"), "attributes"),
                hooks::counter_hooks(b, b.field(b.loc("o"), "incremental")),
            ),
        )),
        "flickGauge" => Some(b.lam(
            &["id", "o"],
            make_metric_expr(
                b,
                b.loc("id"),
                "Gauge",
                b.field(b.loc("o"), "description"),
                b.field(b.loc("o"), "attributes"),
                hooks::gauge_hooks(b),
            ),
        )),
        "flickFrequency" => Some(b.lam(
            &["id", "o"],
            make_metric_expr(
                b,
                b.loc("id"),
                "Frequency",
                b.field(b.loc("o"), "description"),
                b.field(b.loc("o"), "attributes"),
                hooks::frequency_hooks(b, b.field(b.loc("o"), "preregisteredWords")),
            ),
        )),
        "flickHistogram" => Some(b.lam(
            &["id", "o"],
            make_metric_expr(
                b,
                b.loc("id"),
                "Histogram",
                b.field(b.loc("o"), "description"),
                b.field(b.loc("o"), "attributes"),
                hooks::histogram_hooks(b, b.field(b.loc("o"), "boundaries")),
            ),
        )),
        "flickSummaryWithTimestamp" => Some(b.lam(
            &["id", "o"],
            make_metric_expr(
                b,
                b.loc("id"),
                "Summary",
                b.field(b.loc("o"), "description"),
                b.field(b.loc("o"), "attributes"),
                summary::summary_hooks(
                    b,
                    b.field(b.loc("o"), "maxAge"),
                    b.field(b.loc("o"), "maxSize"),
                    b.field(b.loc("o"), "quantiles"),
                ),
            ),
        )),
        "flickSummary" => Some(summary_template(b)),
        "flickTimer" => Some(timer_template(b)),
        _ => None,
    }
}
