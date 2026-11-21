//! Binary `Channel.merge` IR helper.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt, str_lit};
use super::super::{Expr, Pat, Stmt};

fn merge_on_done(b: &ExprBuilder, side: &str) -> Expr {
    let both_done = b.bin(
        BinOp::And,
        b.bin(BinOp::Eq, b.loc("strat"), str_lit(b, "both")),
        b.bin(BinOp::Eq, b.loc("n"), b.int(2)),
    );
    let side_or_both = b.bin(
        BinOp::Or,
        b.bin(BinOp::Eq, b.loc("strat"), str_lit(b, side)),
        both_done,
    );
    let halt = b.bin(
        BinOp::Or,
        b.bin(BinOp::Eq, b.loc("strat"), str_lit(b, "either")),
        side_or_both,
    );
    let finish_q = b.block(
        [
            b.rset(b.loc("dref"), b.some(b.loc("d"))),
            b.let_("_e", b.app(b.global("Queue", "end"), b.loc("q"))),
        ],
        b.unit(),
    );
    b.block(
        [
            b.let_("n", b.add_i(b.rget(b.loc("ended")), b.int(1))),
            b.rset(b.loc("ended"), b.loc("n")),
            b.let_("halt", halt),
        ],
        b.if_(b.loc("halt"), finish_q, b.unit()),
    )
}

fn merge_run_side(b: &ExprBuilder, side: &str, ch_name: &str) -> Expr {
    let on_cause = b.lam(
        &["c"],
        b.block(
            [
                Stmt::Expr(b.app(b.loc("sclose"), b.some(b.loc("c")))),
                b.let_(
                    "_f",
                    b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("q")),
                ),
            ],
            b.unit(),
        ),
    );
    let offer_v = b.block(
        [b.let_(
            "_o",
            b.app2(b.global("Queue", "offer"), b.loc("v"), b.loc("q")),
        )],
        b.unit(),
    );
    let end_side = b.block(
        [
            Stmt::Expr(b.app(b.loc("sclose"), b.none())),
            Stmt::Expr(merge_on_done(b, side)),
        ],
        b.break_("ms", b.unit()),
    );
    let pull_loop = b.loop_ctrl(
        "ms",
        b.match_(
            b.app(b.loc("p"), b.unit()),
            [arm(b.ok_pat("v"), offer_v), arm(b.err_pat("d"), end_side)],
        ),
    );
    let run_pull = b.task(b.block(
        [b.let_("p", b.app2(b.loc(ch_name), b.loc("up"), b.loc("sown")))],
        pull_loop,
    ));
    let body = b.match_(
        b.app(rt(b, "Helper.ch_fork_scope"), b.loc("forked")),
        [arm(
            pair_pat(b, "sown", "sclose"),
            b.app2(b.global("Effect.Errors", "catch_cause"), on_cause, run_pull),
        )],
    );
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        b.task(body),
    )
}

fn merge_pull(b: &ExprBuilder) -> Expr {
    let on_end = b.match_(
        b.rget(b.loc("dref")),
        [
            arm(b.some_pat("d"), b.err(b.loc("d"))),
            arm(Pat::Wild, b.err(b.unit())),
        ],
    );
    b.lam(
        &["_u"],
        b.match_(
            b.app(b.global("Queue", "take"), b.loc("q")),
            [
                arm(b.some_pat("v"), b.ok(b.loc("v"))),
                arm(Pat::Wild, on_end),
            ],
        ),
    )
}

pub(super) fn ch_merge_helper(b: &ExprBuilder) -> Expr {
    // (strategy: String, right: Channel, left: Channel) -> Channel
    let shutdown_fin = b.task(b.block(
        [b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc("q")))],
        b.unit(),
    ));
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(b.global("Scope", "add_finalizer"), shutdown_fin)),
    );
    let build = b.lam(
        &["up", "_own", "forked"],
        b.block(
            [
                b.let_("q", b.app(b.global("Queue", "bounded"), b.int(1))),
                b.let_("ended", b.rmake(b.int(0))),
                b.let_("dref", b.rmake(b.none())),
                Stmt::Expr(reg_fin),
                b.let_("_fl", merge_run_side(b, "left", "left")),
                b.let_("_fr", merge_run_side(b, "right", "right")),
            ],
            merge_pull(b),
        ),
    );
    b.lam(
        &["strat", "right", "left"],
        b.app(rt(b, "Helper.ch_bracket"), build),
    )
}
