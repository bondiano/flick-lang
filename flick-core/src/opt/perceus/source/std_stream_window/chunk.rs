//! `Stream.Transform` `rechunk` and `split` helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Stmt};

pub(super) fn st_rechunk_helper(b: &ExprBuilder) -> Expr {
    let emit_target = b.block(
        [
            b.let_(
                "out",
                b.app2(b.global("List.Core", "take"), b.loc("target"), b.loc("cur")),
            ),
            b.rset(
                b.loc("buf"),
                b.app2(b.global("List.Core", "drop"), b.loc("target"), b.loc("cur")),
            ),
        ],
        b.break_("srk", b.ok(b.loc("out"))),
    );
    let flush_end = b.if_(
        b.bin(BinOp::Gt, b.loc("cl"), b.int(0)),
        b.block(
            [b.rset(b.loc("buf"), b.list(vec![]))],
            b.break_("srk", b.ok(b.loc("cur"))),
        ),
        b.break_("srk", b.err(b.unit())),
    );
    let append_chunk = b.block(
        [b.rset(
            b.loc("buf"),
            b.app2(b.global("List.Core", "append"), b.loc("cur"), b.loc("xs")),
        )],
        b.unit(),
    );
    let pull_more = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), append_chunk),
            arm(
                b.err_pat("_d"),
                b.block([b.rset(b.loc("ended"), b.bool_(true))], b.unit()),
            ),
        ],
    );
    let loop_body = b.block(
        [
            b.let_("cur", b.rget(b.loc("buf"))),
            b.let_("cl", b.app(b.global("List.Core", "length"), b.loc("cur"))),
        ],
        b.if_(
            b.bin(BinOp::Ge, b.loc("cl"), b.loc("target")),
            emit_target,
            b.if_(b.rget(b.loc("ended")), flush_end, pull_more),
        ),
    );
    let target = b.if_(
        b.bin(BinOp::Le, b.loc("sz"), b.int(0)),
        b.int(1),
        b.loc("sz"),
    );
    let init = [
        b.let_("target", target),
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("buf", b.rmake(b.list(vec![]))),
        b.let_("ended", b.rmake(b.bool_(false))),
    ];
    b.lam(
        &["sz", "self", "up", "own"],
        b.block(init, b.lam(&["_u"], b.loop_ctrl("srk", loop_body))),
    )
}

pub(super) fn st_split_helper(b: &ExprBuilder) -> Expr {
    let flush_cur = b.block(
        [
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.list([b.var("cur")]),
                ),
            ),
            b.assign("cur", b.list(vec![])),
        ],
        b.unit(),
    );
    let on_sep = b.if_(
        b.bin(
            BinOp::Gt,
            b.app(b.global("List.Core", "length"), b.var("cur")),
            b.int(0),
        ),
        flush_cur,
        b.unit(),
    );
    let push_el = b.assign_e(
        "cur",
        b.app2(
            b.global("List.Core", "append"),
            b.var("cur"),
            b.list([b.loc("a")]),
        ),
    );
    let scan = b.each(
        "a",
        b.loc("xs"),
        b.if_(b.app(b.loc("p"), b.loc("a")), on_sep, push_el),
    );
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_chunk = b.block(
        [
            b.var_decl("cur", b.rget(b.loc("acc"))),
            b.var_decl("out", b.list(vec![])),
            Stmt::Expr(scan),
            b.rset(b.loc("acc"), b.var("cur")),
        ],
        b.if_(has_out, b.break_("ssp", b.ok(b.var("out"))), b.unit()),
    );
    let has_rem = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("rem")),
        b.int(0),
    );
    let on_end = b.block(
        [
            b.rset(b.loc("done"), b.bool_(true)),
            b.let_("rem", b.rget(b.loc("acc"))),
            b.rset(b.loc("acc"), b.list(vec![])),
        ],
        b.if_(
            has_rem,
            b.break_("ssp", b.ok(b.list([b.loc("rem")]))),
            b.break_("ssp", b.err(b.loc("d"))),
        ),
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [arm(b.ok_pat("xs"), on_chunk), arm(b.err_pat("d"), on_end)],
    );
    let pull = b.lam(
        &["_u"],
        b.if_(
            b.rget(b.loc("done")),
            b.err(b.unit()),
            b.loop_ctrl("ssp", loop_body),
        ),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("acc", b.rmake(b.list(vec![]))),
        b.let_("done", b.rmake(b.bool_(false))),
    ];
    b.lam(&["p", "self", "up", "own"], b.block(init, pull))
}
