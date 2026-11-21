//! `Stream.Transform` `drop*` helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Stmt};

pub(super) fn st_drop_helper(b: &ExprBuilder) -> Expr {
    let emit_tail = b.break_(
        "sdr",
        b.ok(b.app2(
            b.global("List.Core", "drop"),
            b.sub_i(b.loc("n"), b.loc("d0")),
            b.loc("xs"),
        )),
    );
    let drop_more = b.block(
        [
            b.let_("l", b.app(b.global("List.Core", "length"), b.loc("xs"))),
            b.let_("d1", b.add_i(b.loc("d0"), b.loc("l"))),
            b.rset(b.loc("dr"), b.loc("d1")),
        ],
        b.if_(
            b.bin(BinOp::Gt, b.loc("d1"), b.loc("n")),
            emit_tail,
            b.unit(),
        ),
    );
    let on_chunk = b.block(
        [b.let_("d0", b.rget(b.loc("dr")))],
        b.if_(
            b.bin(BinOp::Ge, b.loc("d0"), b.loc("n")),
            b.break_("sdr", b.ok(b.loc("xs"))),
            drop_more,
        ),
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.break_("sdr", b.err(b.loc("d")))),
        ],
    );
    let open = b.lam(
        &["up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("dr", b.rmake(b.int(0))),
            ],
            b.lam(&["_u"], b.loop_ctrl("sdr", loop_body)),
        ),
    );
    b.lam(
        &["n", "self"],
        b.if_(b.bin(BinOp::Le, b.loc("n"), b.int(0)), b.loc("self"), open),
    )
}

pub(super) fn st_drop_right_helper(b: &ExprBuilder) -> Expr {
    let emit = b.block(
        [
            b.let_(
                "out",
                b.app2(b.global("List.Core", "take"), b.loc("take_n"), b.loc("c")),
            ),
            b.rset(
                b.loc("buf"),
                b.app2(b.global("List.Core", "drop"), b.loc("take_n"), b.loc("c")),
            ),
        ],
        b.break_("sdrr", b.ok(b.loc("out"))),
    );
    let keep = b.block([b.rset(b.loc("buf"), b.loc("c"))], b.unit());
    let on_chunk = b.block(
        [
            b.let_(
                "c",
                b.app2(
                    b.global("List.Core", "append"),
                    b.rget(b.loc("buf")),
                    b.loc("xs"),
                ),
            ),
            b.let_(
                "take_n",
                b.sub_i(
                    b.app(b.global("List.Core", "length"), b.loc("c")),
                    b.loc("n"),
                ),
            ),
        ],
        b.if_(b.bin(BinOp::Gt, b.loc("take_n"), b.int(0)), emit, keep),
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.break_("sdrr", b.err(b.loc("d")))),
        ],
    );
    let open = b.lam(
        &["up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("buf", b.rmake(b.list(vec![]))),
            ],
            b.lam(&["_u"], b.loop_ctrl("sdrr", loop_body)),
        ),
    );
    b.lam(
        &["n", "self"],
        b.if_(b.bin(BinOp::Le, b.loc("n"), b.int(0)), b.loc("self"), open),
    )
}

pub(super) fn st_drop_while_helper(b: &ExprBuilder) -> Expr {
    let stop_drop = b.block(
        [b.rset(b.loc("drp"), b.bool_(false))],
        b.break_(
            "sdw",
            b.ok(b.app2(b.global("List.Core", "drop"), b.var("i"), b.loc("xs"))),
        ),
    );
    let scan_step = b.if_(
        b.app(
            b.loc("p"),
            b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("xs")),
        ),
        b.assign_e("i", b.add_i(b.var("i"), b.int(1))),
        stop_drop,
    );
    let on_chunk = b.block(
        [
            b.var_decl("i", b.int(0)),
            b.let_("l", b.app(b.global("List.Core", "length"), b.loc("xs"))),
            Stmt::Expr(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("l")), scan_step)),
        ],
        b.unit(),
    );
    let pull_drop = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.break_("sdw", b.err(b.loc("d")))),
        ],
    );
    let loop_body = b.if_(
        b.not(b.rget(b.loc("drp"))),
        b.break_("sdw", b.app(b.loc("pull"), b.unit())),
        pull_drop,
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("drp", b.rmake(b.bool_(true))),
    ];
    b.lam(
        &["p", "self", "up", "own"],
        b.block(init, b.lam(&["_u"], b.loop_ctrl("sdw", loop_body))),
    )
}
