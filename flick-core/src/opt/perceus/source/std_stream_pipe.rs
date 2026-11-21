//! Audited `Stream.Pipe` pure IR adapters (sinks, channels, execution plans) (Stage 4E).

mod aggregate;
mod plan;
mod plan_inst;
mod sink;
mod tap;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{pair_pat, rt};
use super::{Expr, Pat, Span};

pub(super) fn stream_pipe_effect_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    match (bind, stages) {
        ("run", 2) => Some(rt(b, "Helper.st_run_sink")),
        ("peel", 2) => Some(rt(b, "Helper.st_peel")),
        _ => None,
    }
}

pub(super) fn stream_pipe_pure_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    match (bind, stages) {
        ("toChannel" | "fromChannel", 1) => Some(b.lam(&["x"], b.loc("x"))),
        ("pipeThroughChannel", 2) => Some(rt(b, "Helper.ch_pipe_to")),
        ("pipeThroughChannelOrFail", 2) => Some(rt(b, "Helper.ch_pipe_to_or_fail")),
        ("transduce", 2) => Some(rt(b, "Helper.st_transduce")),
        ("aggregate", 2) => Some(b.lam(
            &["sink", "self"],
            b.app3(
                rt(b, "Helper.st_aggregate_within"),
                b.loc("sink"),
                b.global("Schedule", "forever"),
                b.loc("self"),
            ),
        )),
        ("aggregateWithin", 3) => Some(rt(b, "Helper.st_aggregate_within")),
        ("pipeThrough", 2) => Some(pipe_through_template(b)),
        ("tapSink", 2) => Some(rt(b, "Helper.st_tap_sink")),
        ("flickStreamWithExecutionPlan", 4) => Some(rt(b, "Helper.st_with_execution_plan")),
        _ => None,
    }
}

fn pipe_through_template(b: &ExprBuilder) -> Expr {
    let empty = b.lam(&["_up", "_own"], rt(b, "Helper.ch_empty_pull"));
    let emit_leftover = b.match_(
        b.loc("lo"),
        [
            arm(
                b.some_pat("xs"),
                b.app(rt(b, "Helper.ch_from_list"), b.list([b.loc("xs")])),
            ),
            arm(Pat::Wild, empty),
        ],
    );
    let on_end = b.lam(
        &["end"],
        b.match_(b.loc("end"), [arm(pair_pat(b, "_v", "lo"), emit_leftover)]),
    );
    let piped = b.app2(
        rt(b, "Helper.ch_pipe_to_or_fail"),
        b.loc("sink"),
        b.loc("self"),
    );
    b.lam(
        &["sink", "self"],
        b.app2(rt(b, "Helper.ch_concat_with"), on_end, piped),
    )
}

pub(super) fn stream_pipe_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$stp");
    match name {
        "st_run_sink" => Some(sink::st_run_sink_helper(&b)),
        "st_transduce" => Some(sink::st_transduce_helper(&b)),
        "st_aggregate_within" => Some(aggregate::st_aggregate_within_helper(&b)),
        "st_tap_sink" => Some(tap::st_tap_sink_helper(&b)),
        "st_peel" => Some(sink::st_peel_helper(&b)),
        "st_retry_no_reset" => Some(tap::st_retry_no_reset_helper(&b)),
        "st_with_execution_plan" => Some(plan::st_with_execution_plan_helper(&b)),
        _ => None,
    }
}

fn run_sink_on_pull(b: &ExprBuilder, sink: Expr, up: Expr, own: Expr) -> Expr {
    let step = b.match_(
        b.app(b.loc("sp"), b.unit()),
        [
            arm(b.ok_pat("_o"), b.unit()),
            arm(b.err_pat("end"), b.break_("rsp", b.loc("end"))),
        ],
    );
    b.block(
        [b.let_("sp", b.app2(sink, up, own))],
        b.loop_ctrl("rsp", step),
    )
}
