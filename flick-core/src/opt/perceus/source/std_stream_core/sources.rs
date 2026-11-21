//! `Stream.Core` source construction helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_channel_core::DEFAULT_CHUNK_SIZE;
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Stmt};

pub(super) fn st_from_lists_helper(b: &ExprBuilder) -> Expr {
    let non_empty = b.lam(
        &["xs"],
        b.bin(
            BinOp::Gt,
            b.app(b.global("List.Core", "length"), b.loc("xs")),
            b.int(0),
        ),
    );
    let filtered = b.app2(b.global("List.Core", "filter"), non_empty, b.loc("xss"));
    b.lam(&["xss"], b.app(rt(b, "Helper.ch_from_list"), filtered))
}

pub(super) fn st_from_effect_repeat_helper(b: &ExprBuilder) -> Expr {
    let pull = b.lam(&["_u"], b.ok(b.list([b.await_(b.loc("t"))])));
    b.lam(&["t", "_up", "_own"], pull)
}

pub(super) fn st_from_effect_schedule_helper(b: &ExprBuilder) -> Expr {
    let mark_done = b.if_(
        b.loc("dn"),
        b.block([b.rset(b.loc("done"), b.bool_(true))], b.unit()),
        b.unit(),
    );
    let step_arm = arm(
        pair_pat(b, "_out", "dn"),
        b.block([Stmt::Expr(mark_done)], b.ok(b.list([b.loc("v")]))),
    );
    let step_match = b.match_(
        b.app2(rt(b, "Helper.ch_sched_step"), b.loc("step"), b.loc("v")),
        [step_arm],
    );
    let next_val = b.block([b.let_("v", b.await_(b.loc("t")))], step_match);
    let pull = b.lam(
        &["_u"],
        b.if_(b.rget(b.loc("done")), b.err(b.unit()), next_val),
    );
    let init = [
        b.let_("step", b.app(b.loc("sched"), b.unit())),
        b.let_("done", b.rmake(b.bool_(false))),
    ];
    b.lam(&["sched", "t", "_up", "_own"], b.block(init, pull))
}

pub(super) fn st_from_list_effect_helper(b: &ExprBuilder) -> Expr {
    let chunked = b.app2(
        rt(b, "Helper.ch_from_list_chunked"),
        b.int(DEFAULT_CHUNK_SIZE),
        b.await_(b.loc("t")),
    );
    b.lam(&["t"], b.app(rt(b, "Helper.ch_unwrap"), b.task(chunked)))
}

pub(super) fn st_from_list_effect_repeat_helper(b: &ExprBuilder) -> Expr {
    let non_empty = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("xs")),
        b.int(0),
    );
    let step = b.block(
        [b.let_("xs", b.await_(b.loc("t")))],
        b.if_(non_empty, b.break_("sler", b.ok(b.loc("xs"))), b.unit()),
    );
    let pull = b.lam(&["_u"], b.loop_ctrl("sler", step));
    b.lam(&["t", "_up", "_own"], pull)
}

pub(super) fn st_tick_helper(b: &ExprBuilder) -> Expr {
    let first_emit = b.block(
        [b.rset(b.loc("first"), b.bool_(false))],
        b.ok(b.list([b.unit()])),
    );
    let sleep = b.if_(
        b.app(rt(b, "Duration.IsPositive"), b.loc("dur")),
        b.app(rt(b, "Async.EffectSleep"), b.loc("dur")),
        b.unit(),
    );
    let next_emit = b.block([Stmt::Expr(sleep)], b.ok(b.list([b.unit()])));
    let pull = b.lam(
        &["_u"],
        b.if_(b.rget(b.loc("first")), first_emit, next_emit),
    );
    let init = [b.let_("first", b.rmake(b.bool_(true)))];
    b.lam(&["dur", "_up", "_own"], b.block(init, pull))
}

pub(super) fn st_from_schedule_helper(b: &ExprBuilder) -> Expr {
    let mapped = b.app2(
        rt(b, "Helper.ch_map"),
        b.lam(&["v"], b.list([b.loc("v")])),
        b.app(rt(b, "Helper.ch_from_schedule"), b.loc("sched")),
    );
    let body = b.app2(
        rt(b, "Helper.ch_map_done"),
        b.lam(&["_d"], b.unit()),
        mapped,
    );
    b.lam(&["sched"], body)
}
