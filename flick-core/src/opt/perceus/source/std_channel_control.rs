//! Audited `Channel.Control` pure IR adapters (recovery, lifecycle, merge, schedules, services, spans).

mod errors;
mod lifecycle;
mod merge;
mod merge_all;
mod schedule;
mod services;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{rt, str_lit};
use super::{Expr, Pat, Span};

pub(super) fn channel_control_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("catch", 2) => Some(rt(b, "Helper.ch_catch")),
        ("flickCatchCause", 2) => Some(rt(b, "Helper.ch_catch_cause")),
        ("tapError", 2) => Some(rt(b, "Helper.ch_tap_error")),
        ("flickTapCause", 2) => Some(rt(b, "Helper.ch_tap_cause")),
        ("mapError", 2) => Some(map_error_tpl(b)),
        ("ignore", 1) => Some(b.app(
            rt(b, "Helper.ch_catch"),
            b.lam(&["_e", "_up", "_own"], b.lam(&["_u"], b.err(b.unit()))),
        )),
        ("ignoreCause", 1) => Some(b.app(
            rt(b, "Helper.ch_catch_cause"),
            b.lam(&["_c", "_up", "_own"], b.lam(&["_u"], b.err(b.unit()))),
        )),
        ("ensuring", 2) => Some(b.lam(
            &["fin", "self"],
            b.app2(
                rt(b, "Helper.ch_on_exit"),
                b.lam(&["_ex"], b.await_(b.loc("fin"))),
                b.loc("self"),
            ),
        )),
        ("flickOnError", 2) => Some(on_error_tpl(b)),
        ("flickOnExit", 2) => Some(rt(b, "Helper.ch_on_exit")),
        ("onStart", 2) => Some(b.lam(
            &["t", "self"],
            b.app(
                rt(b, "Helper.ch_unwrap"),
                b.task(b.block([b.let_("_x", b.await_(b.loc("t")))], b.loc("self"))),
            ),
        )),
        ("onFirst", 2) => Some(rt(b, "Helper.ch_on_first")),
        ("onEnd", 2) => Some(rt(b, "Helper.ch_on_end")),
        ("interruptWhen", 2) => Some(rt(b, "Helper.ch_interrupt_when")),
        ("haltWhen", 2) => Some(rt(b, "Helper.ch_halt_when")),
        ("mergeEffect", 2) => Some(merge_effect_tpl(b)),
        ("flickMerge", 3) => Some(rt(b, "Helper.ch_merge")),
        ("flickMergeAll", 2) => Some(rt(b, "Helper.ch_merge_all")),
        ("repeat", 2) => Some(rt(b, "Helper.ch_repeat")),
        ("retry", 2) => Some(rt(b, "Helper.ch_retry")),
        ("schedule", 2) => Some(rt(b, "Helper.ch_schedule")),
        ("provide", 2) => Some(rt(b, "Helper.ch_provide")),
        ("provideService", 3) => Some(rt(b, "Helper.ch_provide_service")),
        ("provideServiceEffect", 3) => Some(rt(b, "Helper.ch_provide_service_eff")),
        ("updateService", 3) => Some(rt(b, "Helper.ch_update_service")),
        ("flickWithSpan", 3) => Some(rt(b, "Helper.ch_with_span")),
        _ => None,
    }
}

pub(super) fn channel_control_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$chctl");
    match name {
        "ch_catch_cause" => Some(errors::ch_catch_cause_helper(&b)),
        "ch_catch" => Some(errors::ch_catch_helper(&b)),
        "ch_tap_error" => Some(errors::ch_tap_error_helper(&b)),
        "ch_tap_cause" => Some(errors::ch_tap_cause_helper(&b)),
        "ch_on_exit" => Some(lifecycle::ch_on_exit_helper(&b)),
        "ch_on_first" => Some(lifecycle::ch_on_first_helper(&b)),
        "ch_on_end" => Some(lifecycle::ch_on_end_helper(&b)),
        "ch_interrupt_when" => Some(lifecycle::ch_interrupt_when_helper(&b)),
        "ch_halt_when" => Some(lifecycle::ch_halt_when_helper(&b)),
        "ch_merge" => Some(merge::ch_merge_helper(&b)),
        "ch_merge_all" => Some(merge_all::ch_merge_all_helper(&b)),
        "ch_repeat" => Some(schedule::ch_repeat_helper(&b)),
        "ch_retry" => Some(schedule::ch_retry_helper(&b)),
        "ch_schedule" => Some(schedule::ch_schedule_helper(&b)),
        "ch_provide_ctx" => Some(services::ch_provide_ctx_helper(&b)),
        "ch_provide" => Some(services::ch_provide_helper(&b)),
        "ch_provide_service" => Some(services::ch_provide_service_helper(&b)),
        "ch_provide_service_eff" => Some(services::ch_provide_service_eff_helper(&b)),
        "ch_update_service" => Some(services::ch_update_service_helper(&b)),
        "ch_with_span" => Some(services::ch_with_span_helper(&b)),
        _ => None,
    }
}

fn map_error_tpl(b: &ExprBuilder) -> Expr {
    let fail_mapped = b.app(b.global("Fail", "fail"), b.app(b.loc("f"), b.loc("e")));
    let handler = b.lam(&["e", "_up", "_own"], b.lam(&["_u"], fail_mapped));
    b.lam(
        &["f", "self"],
        b.app2(rt(b, "Helper.ch_catch"), handler, b.loc("self")),
    )
}

fn on_error_tpl(b: &ExprBuilder) -> Expr {
    let handler = b.lam(
        &["ex"],
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("ex")),
            [
                arm(b.some_pat("c"), b.app(b.loc("f"), b.loc("c"))),
                arm(Pat::Wild, b.unit()),
            ],
        ),
    );
    b.lam(
        &["f", "self"],
        b.app2(rt(b, "Helper.ch_on_exit"), handler, b.loc("self")),
    )
}

fn merge_effect_tpl(b: &ExprBuilder) -> Expr {
    let right_ch = b.lam(
        &["_up", "_own"],
        b.lam(
            &["_u"],
            b.block([b.let_("_x", b.await_(b.loc("t")))], b.err(b.unit())),
        ),
    );
    b.lam(
        &["t", "self"],
        b.app3(
            rt(b, "Helper.ch_merge"),
            str_lit(b, "left"),
            right_ch,
            b.loc("self"),
        ),
    )
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
