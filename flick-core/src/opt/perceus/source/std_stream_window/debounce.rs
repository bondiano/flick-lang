//! `Stream.Transform` `debounce` helper.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat, Stmt};

fn take_last(b: &ExprBuilder, xs: Expr) -> Expr {
    let last_idx = b.sub_i(b.app(b.global("List.Core", "length"), xs.clone()), b.int(1));
    b.list([b.app2(rt(b, "Helper.list_at"), last_idx, xs)])
}

fn debounce_producer(b: &ExprBuilder) -> Expr {
    let next_emit = b.bin(
        BinOp::Arith(Arith::Add, Num::Float),
        b.app(rt(b, "Host.CurrentTimeMillis"), b.unit()),
        b.loc("ms"),
    );
    let on_arr = b.block(
        [
            b.rset(b.loc("last_arr"), b.some(b.loc("arr"))),
            b.rset(b.loc("emit_at"), next_emit),
            b.let_("_op", b.app(b.global("Latch", "open"), b.loc("el"))),
        ],
        b.unit(),
    );
    let pull_loop = b.loop_ctrl(
        "sdb",
        b.block(
            [Stmt::Expr(b.app(b.global("Latch", "await_"), b.loc("pl")))],
            b.match_(
                b.app(b.loc("pull"), b.unit()),
                [
                    arm(b.ok_pat("arr"), on_arr),
                    arm(b.err_pat("d"), b.break_("sdb", b.ok(b.loc("d")))),
                ],
            ),
        ),
    );
    let caught = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        b.lam(&["c"], b.err(b.loc("c"))),
        b.task(pull_loop),
    );
    let body = b.block(
        [
            b.let_("er", caught),
            b.rset(b.loc("end_res"), b.some(b.loc("er"))),
            b.rset(
                b.loc("emit_at"),
                b.app(rt(b, "Host.CurrentTimeMillis"), b.unit()),
            ),
            b.let_("_o1", b.app(b.global("Latch", "open"), b.loc("el"))),
            b.let_("_o2", b.app(b.global("Latch", "open"), b.loc("endl"))),
        ],
        b.unit(),
    );
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        b.task(body),
    )
}

fn debounce_after_end(b: &ExprBuilder) -> Expr {
    let flush_arr = b.block(
        [b.rset(b.loc("last_arr"), b.none())],
        b.ok(take_last(b, b.loc("arr"))),
    );
    let replay_err = b.match_(
        b.loc("er"),
        [
            arm(b.ok_pat("d"), b.err(b.loc("d"))),
            arm(
                b.err_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        ],
    );
    b.match_(
        b.rget(b.loc("last_arr")),
        [
            arm(b.some_pat("arr"), flush_arr),
            arm(Pat::Wild, replay_err),
        ],
    )
}

fn debounce_wait_emit(b: &ExprBuilder) -> Expr {
    let race_sleep = b.app2(
        rt(b, "Async.RaceFirst"),
        b.task(b.app(rt(b, "Async.EffectSleep"), b.loc("wd"))),
        b.task(b.app(b.global("Latch", "await_"), b.loc("endl"))),
    );
    let sleep_rem = b.if_(
        b.bin(BinOp::Gt, b.loc("rem"), b.float(0.0)),
        b.match_(
            b.app(rt(b, "Duration.Millis"), b.loc("rem")),
            [arm(b.ok_pat("wd"), race_sleep), arm(Pat::Wild, b.unit())],
        ),
        b.unit(),
    );
    let emit_arr = b.block(
        [
            b.let_("_c1", b.app(b.global("Latch", "close"), b.loc("el"))),
            b.let_("_c2", b.app(b.global("Latch", "close"), b.loc("pl"))),
            b.rset(b.loc("last_arr"), b.none()),
        ],
        b.break_("sdbs", b.ok(take_last(b, b.loc("arr")))),
    );
    let check_end = b.match_(
        b.rget(b.loc("end_res")),
        [
            arm(
                b.some_pat("er"),
                b.match_(
                    b.loc("er"),
                    [
                        arm(b.ok_pat("d"), b.break_("sdbs", b.err(b.loc("d")))),
                        arm(
                            b.err_pat("c"),
                            b.app(b.global("Failure", "rethrow"), b.loc("c")),
                        ),
                    ],
                ),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let try_emit = b.match_(
        b.rget(b.loc("last_arr")),
        [arm(b.some_pat("arr"), emit_arr), arm(Pat::Wild, check_end)],
    );
    let wait_step = b.block(
        [
            b.let_("now", b.app(rt(b, "Host.CurrentTimeMillis"), b.unit())),
            b.let_("at", b.rget(b.loc("emit_at"))),
            b.let_(
                "rem",
                b.bin(
                    BinOp::Arith(Arith::Sub, Num::Float),
                    b.loc("at"),
                    b.loc("now"),
                ),
            ),
            Stmt::Expr(sleep_rem),
        ],
        b.if_(
            b.bin(
                BinOp::Lt,
                b.app(rt(b, "Host.CurrentTimeMillis"), b.unit()),
                b.rget(b.loc("emit_at")),
            ),
            b.unit(),
            try_emit,
        ),
    );
    let stmts = [
        b.let_("_o1", b.app(b.global("Latch", "open"), b.loc("pl"))),
        Stmt::Expr(b.app(b.global("Latch", "await_"), b.loc("el"))),
    ];
    b.block(stmts, b.loop_ctrl("sdbs", wait_step))
}

pub(super) fn st_debounce_helper(b: &ExprBuilder) -> Expr {
    let ms = b.match_(
        b.app(rt(b, "Duration.ToMillis"), b.loc("dur")),
        [
            arm(b.some_pat("m"), b.loc("m")),
            arm(Pat::Wild, b.float(1e18)),
        ],
    );
    let stmts = [
        b.let_("ms", ms),
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("last_arr", b.rmake(b.none())),
        b.let_("end_res", b.rmake(b.none())),
        b.let_("emit_at", b.rmake(b.float(0.0))),
        b.let_("pl", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_("el", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_("endl", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_("_pf", debounce_producer(b)),
    ];
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("end_res")),
            [
                arm(b.some_pat("er"), debounce_after_end(b)),
                arm(Pat::Wild, debounce_wait_emit(b)),
            ],
        ),
    );
    let build = b.lam(&["up", "own", "forked"], b.block(stmts, pull));
    b.lam(&["dur", "self"], b.app(rt(b, "Helper.ch_bracket"), build))
}
