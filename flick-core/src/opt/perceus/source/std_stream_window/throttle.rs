//! `Stream.Transform` `throttle` helper.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat, Stmt};

fn fop(b: &ExprBuilder, op: Arith, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(op, Num::Float), x, y)
}

fn fmin(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.if_(b.bin(BinOp::Lt, x.clone(), y.clone()), x, y)
}

fn throttle_enforce(b: &ExprBuilder) -> Expr {
    let accept = b.block(
        [
            b.rset(
                b.loc("toks"),
                fop(b, Arith::Sub, b.loc("avail"), b.loc("w")),
            ),
            b.rset(b.loc("ts"), b.loc("now")),
        ],
        b.break_("sth", b.ok(b.loc("arr"))),
    );
    b.if_(
        b.bin(BinOp::Le, b.loc("w"), b.loc("avail")),
        accept,
        b.unit(),
    )
}

fn throttle_shape(b: &ExprBuilder) -> Expr {
    let sleep_del = b.match_(
        b.app(rt(b, "Duration.Millis"), b.loc("del")),
        [
            arm(
                b.ok_pat("wd"),
                b.app(rt(b, "Async.EffectSleep"), b.loc("wd")),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let del_expr = fop(
        b,
        Arith::Mul,
        fop(
            b,
            Arith::Div,
            fop(b, Arith::Sub, b.float(0.0), b.loc("rem")),
            b.loc("units"),
        ),
        b.loc("dur_ms"),
    );
    let wait_neg = b.block(
        [b.let_("del", del_expr)],
        b.if_(
            b.bin(BinOp::Gt, b.loc("del"), b.float(0.0)),
            sleep_del,
            b.unit(),
        ),
    );
    let stmts = [
        b.let_("rem", fop(b, Arith::Sub, b.loc("avail"), b.loc("w"))),
        Stmt::Expr(b.if_(
            b.bin(BinOp::Lt, b.loc("rem"), b.float(0.0)),
            wait_neg,
            b.unit(),
        )),
        b.rset(b.loc("toks"), b.loc("rem")),
        b.rset(b.loc("ts"), b.loc("now")),
    ];
    b.block(stmts, b.break_("sth", b.ok(b.loc("arr"))))
}

pub(super) fn st_throttle_helper(b: &ExprBuilder) -> Expr {
    let shape_pat = Pat::Con(
        ExprBuilder::con("Stream.Transform", "ThrottleStrategy", "Shape", 0),
        vec![],
    );
    let dur_ms = b.match_(
        b.app(
            rt(b, "Duration.ToMillis"),
            b.field(b.loc("opts"), "duration"),
        ),
        [
            arm(b.some_pat("m"), b.loc("m")),
            arm(Pat::Wild, b.float(1e18)),
        ],
    );
    let burst = b.match_(
        b.field(b.loc("opts"), "burst"),
        [
            arm(b.some_pat("bv"), b.loc("bv")),
            arm(Pat::Wild, b.float(0.0)),
        ],
    );
    let enforce = b.match_(
        b.field(b.loc("opts"), "strategy"),
        [
            arm(
                b.some_pat("st"),
                b.match_(
                    b.loc("st"),
                    [
                        arm(shape_pat, b.bool_(false)),
                        arm(Pat::Wild, b.bool_(true)),
                    ],
                ),
            ),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let refilled = fop(
        b,
        Arith::Add,
        b.rget(b.loc("toks")),
        fop(
            b,
            Arith::Mul,
            fop(
                b,
                Arith::Div,
                fop(b, Arith::Sub, b.loc("now"), b.rget(b.loc("ts"))),
                b.loc("dur_ms"),
            ),
            b.loc("units"),
        ),
    );
    let on_arr = b.block(
        [
            b.let_("w", b.app(b.loc("cost"), b.loc("arr"))),
            b.let_("now", b.app(rt(b, "Host.CurrentTimeMillis"), b.unit())),
            b.let_("avail", fmin(b, refilled, b.loc("most"))),
        ],
        b.if_(b.loc("enforce"), throttle_enforce(b), throttle_shape(b)),
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("arr"), on_arr),
            arm(b.err_pat("d"), b.break_("sth", b.err(b.loc("d")))),
        ],
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("cost", b.field(b.loc("opts"), "cost")),
        b.let_("units", b.field(b.loc("opts"), "units")),
        b.let_("dur_ms", dur_ms),
        b.let_("burst", burst),
        b.let_("most", fop(b, Arith::Add, b.loc("units"), b.loc("burst"))),
        b.let_("enforce", enforce),
        b.let_("toks", b.rmake(b.loc("units"))),
        b.let_(
            "ts",
            b.rmake(b.app(rt(b, "Host.CurrentTimeMillis"), b.unit())),
        ),
    ];
    let pull = b.lam(&["_u"], b.loop_ctrl("sth", loop_body));
    b.lam(&["opts", "self", "up", "own"], b.block(init, pull))
}
