//! Audited `Channel.Transform` pure IR adapters (Stage 4E).

mod filter_ops;
mod map_ops;
mod pipe_ops;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::rt;
use super::{Expr, Pat, Span};

pub(super) fn channel_transform_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    match (bind, stages) {
        ("map" | "mapEffect", 2) => Some(rt(b, "Helper.ch_map")),
        ("tap", 2) => Some(rt(b, "Helper.ch_tap")),
        ("mapDone" | "mapDoneEffect", 2) => Some(rt(b, "Helper.ch_map_done")),
        ("mapInput", 2) => Some(rt(b, "Helper.ch_map_input")),
        ("mapInputError", 2) => Some(rt(b, "Helper.ch_map_input_error")),
        ("flatMap", 2) => Some(rt(b, "Helper.ch_flat_map")),
        ("switchMap", 2) => Some(rt(b, "Helper.ch_switch_map")),
        ("flatten", 1) => Some(b.app(rt(b, "Helper.ch_flat_map"), b.lam(&["x"], b.loc("x")))),
        ("flattenArray", 1) => Some(rt(b, "Helper.ch_flatten_array")),
        ("concat", 2) => Some(b.lam(
            &["next", "self"],
            b.app2(
                rt(b, "Helper.ch_concat_with"),
                b.lam(&["_d"], b.loc("next")),
                b.loc("self"),
            ),
        )),
        ("concatWith", 2) => Some(rt(b, "Helper.ch_concat_with")),
        ("orElseIfEmpty", 2) => Some(rt(b, "Helper.ch_or_else_if_empty")),
        ("drain", 1) => Some(rt(b, "Helper.ch_drain")),
        ("forever", 1) => Some(rt(b, "Helper.ch_forever")),
        ("filter" | "filterEffect", 2) => Some(rt(b, "Helper.ch_filter")),
        ("filterMap" | "filterMapEffect", 2) => Some(rt(b, "Helper.ch_filter_map")),
        ("filterArray" | "filterArrayEffect", 2) => Some(rt(b, "Helper.ch_filter_array")),
        ("filterMapArray" | "filterMapArrayEffect", 2) => Some(rt(b, "Helper.ch_filter_map_array")),
        ("mapAccum", 3) => Some(rt(b, "Helper.ch_map_accum")),
        ("scan" | "scanEffect", 3) => Some(rt(b, "Helper.ch_scan")),
        ("pipeTo", 2) => Some(rt(b, "Helper.ch_pipe_to")),
        ("pipeToOrFail", 2) => Some(rt(b, "Helper.ch_pipe_to_or_fail")),
        _ => None,
    }
}

pub(super) fn channel_transform_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$cht");
    match name {
        "ch_map" => Some(map_ops::ch_map_helper(&b)),
        "ch_tap" => Some(map_ops::ch_tap_helper(&b)),
        "ch_map_done" => Some(map_ops::ch_map_done_helper(&b)),
        "ch_map_input" => Some(map_ops::ch_map_input_helper(&b)),
        "ch_map_input_error" => Some(map_ops::ch_map_input_error_helper(&b)),
        "ch_flat_map" => Some(map_ops::ch_flat_map_helper(&b)),
        "ch_switch_map" => Some(map_ops::ch_switch_map_helper(&b)),
        "ch_flatten_array" => Some(map_ops::ch_flatten_array_helper(&b)),
        "ch_concat_with" => Some(pipe_ops::ch_concat_with_helper(&b)),
        "ch_or_else_if_empty" => Some(pipe_ops::ch_or_else_if_empty_helper(&b)),
        "ch_drain" => Some(pipe_ops::ch_drain_helper(&b)),
        "ch_forever" => Some(pipe_ops::ch_forever_helper(&b)),
        "ch_filter" => Some(filter_ops::ch_filter_helper(&b)),
        "ch_filter_map" => Some(filter_ops::ch_filter_map_helper(&b)),
        "ch_filter_array" => Some(filter_ops::ch_filter_array_helper(&b)),
        "ch_filter_map_array" => Some(filter_ops::ch_filter_map_array_helper(&b)),
        "ch_map_accum" => Some(filter_ops::ch_map_accum_helper(&b)),
        "ch_scan" => Some(filter_ops::ch_scan_helper(&b)),
        "ch_pipe_to" => Some(pipe_ops::ch_pipe_to_helper(&b)),
        "ch_pipe_to_or_fail" => Some(pipe_ops::ch_pipe_to_or_fail_helper(&b)),
        _ => None,
    }
}

fn close_active_owner(b: &ExprBuilder, opt_c: Expr) -> Expr {
    let close_co = b.block(
        [b.rset(b.loc("aown"), b.none())],
        b.app2(rt(b, "Layer.OwnerClose"), b.loc("co"), opt_c),
    );
    b.app(
        b.global("Effect.Concurrency", "uninterruptible_mask"),
        b.lam(
            &["_r"],
            b.match_(
                b.rget(b.loc("aown")),
                [arm(b.some_pat("co"), close_co), arm(Pat::Wild, b.unit())],
            ),
        ),
    )
}
