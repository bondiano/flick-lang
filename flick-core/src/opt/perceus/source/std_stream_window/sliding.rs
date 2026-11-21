//! `Stream.Transform` `sliding` / `slidingSize` helper.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

fn sliding_win_step(b: &ExprBuilder) -> Expr {
    b.block(
        [
            b.let_(
                "w",
                b.app2(b.global("List.Core", "take"), b.loc("ws"), b.var("cur")),
            ),
            b.assign(
                "wins",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("wins"),
                    b.list([b.loc("w")]),
                ),
            ),
            b.let_("cl2", b.app(b.global("List.Core", "length"), b.var("cur"))),
            b.let_(
                "adv",
                b.if_(
                    b.bin(BinOp::Lt, b.loc("ss"), b.loc("cl2")),
                    b.loc("ss"),
                    b.loc("cl2"),
                ),
            ),
            b.assign(
                "cur",
                b.app2(b.global("List.Core", "drop"), b.loc("adv"), b.var("cur")),
            ),
            b.rset(b.loc("skip"), b.sub_i(b.loc("ss"), b.loc("adv"))),
        ],
        b.unit(),
    )
}

fn sliding_on_chunk(b: &ExprBuilder) -> Expr {
    let apply_skip = b.block(
        [
            b.let_("cl", b.app(b.global("List.Core", "length"), b.var("cur"))),
            b.let_(
                "d",
                b.if_(
                    b.bin(BinOp::Lt, b.loc("sk"), b.loc("cl")),
                    b.loc("sk"),
                    b.loc("cl"),
                ),
            ),
            b.assign(
                "cur",
                b.app2(b.global("List.Core", "drop"), b.loc("d"), b.var("cur")),
            ),
            b.rset(b.loc("skip"), b.sub_i(b.loc("sk"), b.loc("d"))),
        ],
        b.unit(),
    );
    let has_win = b.bin(
        BinOp::Ge,
        b.app(b.global("List.Core", "length"), b.var("cur")),
        b.loc("ws"),
    );
    let emit_wins = b.block(
        [
            b.rset(b.loc("emitted"), b.bool_(true)),
            b.var_decl("wins", b.list(vec![])),
            Stmt::Expr(b.while_(has_win, sliding_win_step(b))),
            b.rset(b.loc("buf"), b.var("cur")),
        ],
        b.break_("ssl", b.ok(b.var("wins"))),
    );
    let need_more = b.bin(
        BinOp::Lt,
        b.app(b.global("List.Core", "length"), b.var("cur")),
        b.loc("ws"),
    );
    let stmts = [
        b.var_decl(
            "cur",
            b.app2(
                b.global("List.Core", "append"),
                b.rget(b.loc("buf")),
                b.loc("xs"),
            ),
        ),
        b.let_("sk", b.rget(b.loc("skip"))),
        Stmt::Expr(b.if_(
            b.bin(BinOp::Gt, b.loc("sk"), b.int(0)),
            apply_skip,
            b.unit(),
        )),
    ];
    b.block(
        stmts,
        b.if_(
            need_more,
            b.block([b.rset(b.loc("buf"), b.var("cur"))], b.unit()),
            emit_wins,
        ),
    )
}

fn sliding_on_end(b: &ExprBuilder) -> Expr {
    let skip_leftover = b.bin(
        BinOp::Or,
        b.bin(BinOp::Eq, b.loc("ll"), b.int(0)),
        b.bin(
            BinOp::And,
            b.rget(b.loc("emitted")),
            b.bin(BinOp::Le, b.loc("ll"), b.sub_i(b.loc("ws"), b.loc("ss"))),
        ),
    );
    let finish_now = b.match_(
        b.loc("er"),
        [
            arm(b.ok_pat("d"), b.break_("ssl", b.err(b.loc("d")))),
            arm(
                b.err_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        ],
    );
    let flush_tail = b.block(
        [
            b.rset(b.loc("done_r"), b.some(b.loc("er"))),
            b.rset(b.loc("buf"), b.list(vec![])),
        ],
        b.break_("ssl", b.ok(b.list([b.loc("left")]))),
    );
    let stmts = [
        b.let_("left", b.rget(b.loc("buf"))),
        b.let_("ll", b.app(b.global("List.Core", "length"), b.loc("left"))),
    ];
    b.block(stmts, b.if_(skip_leftover, finish_now, flush_tail))
}

pub(super) fn st_sliding_size_helper(b: &ExprBuilder) -> Expr {
    let pull_task = b.task(b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), b.ok(b.loc("xs"))),
            arm(b.err_pat("d"), b.err(b.ok(b.loc("d")))),
        ],
    ));
    let caught = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        b.lam(&["c"], b.err(b.err(b.loc("c")))),
        pull_task,
    );
    let pull_loop = b.loop_ctrl(
        "ssl",
        b.match_(
            caught,
            [
                arm(b.ok_pat("xs"), sliding_on_chunk(b)),
                arm(b.err_pat("er"), sliding_on_end(b)),
            ],
        ),
    );
    let replay_end = b.match_(
        b.loc("end_res"),
        [
            arm(b.ok_pat("d"), b.err(b.loc("d"))),
            arm(
                b.err_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("done_r")),
            [
                arm(b.some_pat("end_res"), replay_end),
                arm(Pat::Wild, pull_loop),
            ],
        ),
    );
    let ws = b.if_(
        b.bin(BinOp::Le, b.loc("sz"), b.int(0)),
        b.int(1),
        b.loc("sz"),
    );
    let ss = b.if_(
        b.bin(BinOp::Le, b.loc("st"), b.int(0)),
        b.int(1),
        b.loc("st"),
    );
    let init = [
        b.let_("ws", ws),
        b.let_("ss", ss),
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("buf", b.rmake(b.list(vec![]))),
        b.let_("emitted", b.rmake(b.bool_(false))),
        b.let_("skip", b.rmake(b.int(0))),
        b.let_("done_r", b.rmake(b.none())),
    ];
    b.lam(&["sz", "st", "self", "up", "own"], b.block(init, pull))
}
