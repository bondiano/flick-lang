//! Audited `Metric` operations, boundaries, snapshot/dump, runtime metrics,
//! and `Effect.Observe` `track*` adapters (Stage 4D).

mod ops;
mod track;

use flick_ir::ForeignMode;

use super::Expr;
use super::expr_builder::{ExprBuilder, arm};
use super::std_metric::{
    KEY_METRIC_ATTRS, KEY_RUNTIME_METRICS, current_metric_attrs, merge_attrs_expr,
};
use super::std_observe_refs::{obs_ref, obs_with};

pub(super) use track::effect_track_template;

fn runtime_metrics_layer(b: &ExprBuilder, enabled: bool) -> Expr {
    b.app(
        b.global("Layer", "from_build"),
        b.lam(
            &["_m"],
            b.app2(
                b.global("Context", "make"),
                obs_ref(b, KEY_RUNTIME_METRICS, b.bool_(false)),
                b.bool_(enabled),
            ),
        ),
    )
}

fn with_attributes_template(b: &ExprBuilder) -> Expr {
    let merged = || merge_attrs_expr(b, b.loc("ex"), b.loc("added"));
    let wrapped = b.tuple([
        b.lam(&["ex"], b.app(b.loc("gf"), merged())),
        b.lam(&["ex", "x"], b.app2(b.loc("uf"), merged(), b.loc("x"))),
        b.lam(&["ex", "x"], b.app2(b.loc("mf"), merged(), b.loc("x"))),
    ]);
    b.lam(
        &["added", "m"],
        b.match_(b.loc("m"), [arm(b.tup_pat(&["gf", "uf", "mf"]), wrapped)]),
    )
}

fn map_input_template(b: &ExprBuilder) -> Expr {
    let mapped = b.tuple([
        b.loc("gf"),
        b.lam(
            &["ex", "x"],
            b.app2(b.loc("uf"), b.loc("ex"), b.app(b.loc("f"), b.loc("x"))),
        ),
        b.lam(
            &["ex", "x"],
            b.app2(b.loc("mf"), b.loc("ex"), b.app(b.loc("f"), b.loc("x"))),
        ),
    ]);
    b.lam(
        &["f", "m"],
        b.match_(b.loc("m"), [arm(b.tup_pat(&["gf", "uf", "mf"]), mapped)]),
    )
}

fn with_constant_input_template(b: &ExprBuilder) -> Expr {
    let consted = b.tuple([
        b.loc("gf"),
        b.lam(&["ex", "_x"], b.app2(b.loc("uf"), b.loc("ex"), b.loc("c"))),
        b.lam(&["ex", "_x"], b.app2(b.loc("mf"), b.loc("ex"), b.loc("c"))),
    ]);
    b.lam(
        &["c", "m"],
        b.match_(b.loc("m"), [arm(b.tup_pat(&["gf", "uf", "mf"]), consted)]),
    )
}

pub(super) fn metric_ops_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    match (mode, bind, stages) {
        (ForeignMode::Effect, "flickValue", 1) => Some(b.lam(
            &["m"],
            b.match_(
                b.loc("m"),
                [arm(
                    b.tup_pat(&["gf", "_u", "_m"]),
                    b.app(b.loc("gf"), current_metric_attrs(b)),
                )],
            ),
        )),
        (ForeignMode::Effect, "update", 2) => Some(b.lam(
            &["inp", "m"],
            b.match_(
                b.loc("m"),
                [arm(
                    b.tup_pat(&["_g", "uf", "_m"]),
                    b.app2(b.loc("uf"), current_metric_attrs(b), b.loc("inp")),
                )],
            ),
        )),
        (ForeignMode::Effect, "modify", 2) => Some(b.lam(
            &["inp", "m"],
            b.match_(
                b.loc("m"),
                [arm(
                    b.tup_pat(&["_g", "_u", "mf"]),
                    b.app2(b.loc("mf"), current_metric_attrs(b), b.loc("inp")),
                )],
            ),
        )),
        (ForeignMode::Pure, "mapInput", 2) => Some(map_input_template(b)),
        (ForeignMode::Pure, "withConstantInput", 2) => Some(with_constant_input_template(b)),
        (ForeignMode::Pure, "withAttributes", 2) => Some(with_attributes_template(b)),
        (ForeignMode::Effect, "flickWithCurrentAttributes", 2) => Some(b.lam(
            &["attrs", "t"],
            obs_with(
                b,
                KEY_METRIC_ATTRS,
                b.list(vec![]),
                b.loc("attrs"),
                b.loc("t"),
            ),
        )),
        (ForeignMode::Effect, "flickSnapshot", 1) => Some(ops::snapshot_template(b)),
        (ForeignMode::Effect, "flickDump", 1) => Some(ops::dump_template(b)),
        (ForeignMode::Pure, "boundariesFromIterable", 1) => {
            Some(ops::boundaries_from_iterable_template(b))
        }
        (ForeignMode::Pure, "linearBoundaries", 1) => Some(ops::linear_boundaries_template(b)),
        (ForeignMode::Pure, "exponentialBoundaries", 1) => {
            Some(ops::exponential_boundaries_template(b))
        }
        (ForeignMode::Effect, "enableRuntimeMetrics", 1) => Some(b.lam(
            &["t"],
            obs_with(
                b,
                KEY_RUNTIME_METRICS,
                b.bool_(false),
                b.bool_(true),
                b.loc("t"),
            ),
        )),
        (ForeignMode::Effect, "disableRuntimeMetrics", 1) => Some(b.lam(
            &["t"],
            obs_with(
                b,
                KEY_RUNTIME_METRICS,
                b.bool_(false),
                b.bool_(false),
                b.loc("t"),
            ),
        )),
        (ForeignMode::Pure, "enableRuntimeMetricsLayer", 0) => Some(runtime_metrics_layer(b, true)),
        (ForeignMode::Pure, "disableRuntimeMetricsLayer", 0) => {
            Some(runtime_metrics_layer(b, false))
        }
        _ => None,
    }
}
