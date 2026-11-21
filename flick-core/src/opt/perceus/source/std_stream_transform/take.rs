//! `Stream.Transform` `take*` helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat, Stmt};

pub(super) fn st_take_helper(b: &ExprBuilder) -> Expr {
    let take_out = b.if_(
        b.bin(BinOp::Gt, b.loc("l"), b.loc("r")),
        b.app2(b.global("List.Core", "take"), b.loc("r"), b.loc("xs")),
        b.loc("xs"),
    );
    let on_chunk = b.block(
        [
            b.let_("l", b.app(b.global("List.Core", "length"), b.loc("xs"))),
            b.let_("out", take_out),
            b.rset(
                b.loc("rem"),
                b.sub_i(
                    b.loc("r"),
                    b.app(b.global("List.Core", "length"), b.loc("out")),
                ),
            ),
        ],
        b.ok(b.loc("out")),
    );
    let pull_next = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.err(b.loc("d"))),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.block(
            [b.let_("r", b.rget(b.loc("rem")))],
            b.if_(
                b.bin(BinOp::Le, b.loc("r"), b.int(0)),
                b.err(b.unit()),
                pull_next,
            ),
        ),
    );
    let open = b.lam(
        &["up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("rem", b.rmake(b.loc("n"))),
            ],
            pull,
        ),
    );
    let empty = b.lam(&["_up", "_own"], rt(b, "Helper.ch_empty_pull"));
    b.lam(
        &["n", "self"],
        b.if_(b.bin(BinOp::Le, b.loc("n"), b.int(0)), empty, open),
    )
}

pub(super) fn st_take_right_helper(b: &ExprBuilder) -> Expr {
    let trim = b.if_(
        b.bin(BinOp::Gt, b.loc("l"), b.loc("n")),
        b.app2(
            b.global("List.Core", "drop"),
            b.sub_i(b.loc("l"), b.loc("n")),
            b.loc("c"),
        ),
        b.loc("c"),
    );
    let on_chunk = b.block(
        [
            b.let_(
                "c",
                b.app2(b.global("List.Core", "append"), b.var("buf"), b.loc("xs")),
            ),
            b.let_("l", b.app(b.global("List.Core", "length"), b.loc("c"))),
            b.assign("buf", trim),
        ],
        b.unit(),
    );
    let drain_loop = b.loop_ctrl(
        "str",
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("xs"), on_chunk),
                arm(b.err_pat("_d"), b.break_("str", b.unit())),
            ],
        ),
    );
    let has_buf = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("buf")),
        b.int(0),
    );
    let collect = b.block(
        [
            b.var_decl("buf", b.list(vec![])),
            Stmt::Expr(drain_loop),
            b.rset(b.loc("done"), b.bool_(true)),
        ],
        b.if_(has_buf, b.ok(b.var("buf")), b.err(b.unit())),
    );
    let pull = b.lam(
        &["_u"],
        b.if_(b.rget(b.loc("done")), b.err(b.unit()), collect),
    );
    let open = b.lam(
        &["up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("done", b.rmake(b.bool_(false))),
            ],
            pull,
        ),
    );
    let empty = b.lam(&["_up", "_own"], rt(b, "Helper.ch_empty_pull"));
    b.lam(
        &["n", "self"],
        b.if_(b.bin(BinOp::Le, b.loc("n"), b.int(0)), empty, open),
    )
}

pub(super) fn st_take_until_helper(b: &ExprBuilder) -> Expr {
    let push_a = b.assign_e(
        "out",
        b.app2(
            b.global("List.Core", "append"),
            b.var("out"),
            b.list([b.loc("a")]),
        ),
    );
    let on_stop = b.block(
        [
            b.rset(b.loc("done"), b.bool_(true)),
            Stmt::Expr(b.if_(b.loc("excl"), b.unit(), push_a.clone())),
        ],
        b.break_("stu", b.unit()),
    );
    let scan = b.for_ctrl(
        "stu",
        "a",
        b.loc("xs"),
        b.if_(b.app(b.loc("stop"), b.loc("a")), on_stop, push_a),
    );
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_chunk = b.block(
        [b.var_decl("out", b.list(vec![])), Stmt::Expr(scan)],
        b.if_(has_out, b.ok(b.var("out")), b.err(b.unit())),
    );
    let pull_next = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.err(b.loc("d"))),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.if_(b.rget(b.loc("done")), b.err(b.unit()), pull_next),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("done", b.rmake(b.bool_(false))),
    ];
    b.lam(&["excl", "stop", "self", "up", "own"], b.block(init, pull))
}

pub(super) fn st_take_while_filter_helper(b: &ExprBuilder) -> Expr {
    let push_v = b.assign_e(
        "out",
        b.app2(
            b.global("List.Core", "append"),
            b.var("out"),
            b.list([b.loc("v")]),
        ),
    );
    let stop = b.block(
        [b.rset(b.loc("done"), b.bool_(true))],
        b.break_("stwf", b.unit()),
    );
    let pull_step = b.match_(
        b.app(b.loc("f"), b.loc("a")),
        [arm(b.ok_pat("v"), push_v), arm(Pat::Wild, stop)],
    );
    let scan = b.for_ctrl("stwf", "a", b.loc("xs"), pull_step);
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_chunk = b.block(
        [b.var_decl("out", b.list(vec![])), Stmt::Expr(scan)],
        b.if_(has_out, b.ok(b.var("out")), b.err(b.unit())),
    );
    let pull_next = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.err(b.loc("d"))),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.if_(b.rget(b.loc("done")), b.err(b.unit()), pull_next),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("done", b.rmake(b.bool_(false))),
    ];
    b.lam(&["f", "self", "up", "own"], b.block(init, pull))
}
