//! `Stream.Combine` `zipLatestAll` helper.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, ExprKind, Pat, Stmt};

fn list_set_at(b: &ExprBuilder, idx: Expr, val: Expr, xs: Expr) -> Expr {
    let step = b.block(
        [
            b.let_("ck", b.var("k")),
            b.assign("k", b.add_i(b.loc("ck"), b.int(1))),
        ],
        b.yield_(b.if_(b.bin(BinOp::Eq, b.loc("ck"), idx), val, b.loc("item"))),
    );
    b.block(
        [b.var_decl("k", b.int(0))],
        b.gen_strict(b.each("item", xs, step)),
    )
}

fn unwrap_all_some(b: &ExprBuilder, xs: Expr) -> Expr {
    let step = b.match_(
        b.loc("o"),
        [
            arm(b.some_pat("v"), b.yield_(b.loc("v"))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    b.gen_strict(b.each("o", xs, step))
}

fn latest_channel_step(b: &ExprBuilder) -> Expr {
    let wait_ready = b.block(
        [Stmt::Expr(
            b.app(b.global("Latch", "await_"), b.loc("ready")),
        )],
        b.none(),
    );
    let emit_snap = b.block(
        [
            b.let_("snap", unwrap_all_some(b, b.loc("next_l"))),
            b.let_("_op", b.app(b.global("Latch", "open"), b.loc("ready"))),
        ],
        b.some(b.list([b.loc("snap")])),
    );
    let next_cnt = b.if_(
        b.loc("was"),
        b.rget(b.loc("seen")),
        b.add_i(b.rget(b.loc("seen")), b.int(1)),
    );
    let on_elem = b.lam(
        &["a"],
        b.block(
            [
                b.let_("was", b.rget(b.loc("has_i"))),
                b.rset(b.loc("has_i"), b.bool_(true)),
                b.let_(
                    "next_l",
                    list_set_at(b, b.loc("ci"), b.some(b.loc("a")), b.rget(b.loc("latest"))),
                ),
                b.rset(b.loc("latest"), b.loc("next_l")),
                b.let_("cnt", next_cnt),
                b.rset(b.loc("seen"), b.loc("cnt")),
            ],
            b.if_(
                b.bin(BinOp::Lt, b.loc("cnt"), b.loc("n")),
                wait_ready,
                emit_snap,
            ),
        ),
    );
    let filter_opt = b.lam(
        &["o"],
        b.match_(
            b.loc("o"),
            [
                arm(b.some_pat("v"), b.ok(b.loc("v"))),
                arm(Pat::Wild, b.err(b.unit())),
            ],
        ),
    );
    let on_done = b.lam(
        &["_d"],
        b.if_(
            b.not(b.rget(b.loc("has_i"))),
            b.block(
                [b.let_("_o", b.app(b.global("Latch", "open"), b.loc("empty_l")))],
                b.unit(),
            ),
            b.unit(),
        ),
    );
    let flat_s = b.app(rt(b, "Helper.ch_flatten_array"), b.loc("s"));
    let stmts = [
        b.let_("ci", b.var("i")),
        b.assign("i", b.add_i(b.loc("ci"), b.int(1))),
        b.let_("has_i", b.rmake(b.bool_(false))),
        b.let_("mapped", b.app2(rt(b, "Helper.ch_map"), on_elem, flat_s)),
        b.let_(
            "kept",
            b.app2(rt(b, "Helper.ch_filter_map"), filter_opt, b.loc("mapped")),
        ),
        b.let_(
            "ch",
            b.app2(rt(b, "Helper.ch_map_done"), on_done, b.loc("kept")),
        ),
    ];
    b.block(stmts, b.yield_(b.loc("ch")))
}

pub(super) fn st_zip_latest_all_helper(b: &ExprBuilder) -> Expr {
    let init_latest = b.gen_strict(b.each("_s", b.loc("streams"), b.yield_(b.none())));
    let merge_opts = b.at(ExprKind::Record(vec![
        ("bufferSize".into(), b.some(b.int(1))),
        ("concurrency".into(), b.none()),
        ("switch".into(), b.bool_(false)),
    ]));
    let merged = b.app2(
        rt(b, "Helper.ch_merge_all"),
        merge_opts,
        b.app(rt(b, "Helper.ch_from_list"), b.loc("chs")),
    );
    let fin = b.app2(
        rt(b, "Helper.ch_interrupt_when"),
        b.task(b.app(b.global("Latch", "await_"), b.loc("empty_l"))),
        b.loc("merged"),
    );
    let stmts = [
        b.let_(
            "n",
            b.app(b.global("List.Core", "length"), b.loc("streams")),
        ),
        b.let_("latest", b.rmake(init_latest)),
        b.let_("seen", b.rmake(b.int(0))),
        b.let_("ready", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_("empty_l", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.var_decl("i", b.int(0)),
        b.let_(
            "chs",
            b.gen_strict(b.each("s", b.loc("streams"), latest_channel_step(b))),
        ),
        b.let_("merged", merged),
        b.let_("fin", fin),
    ];
    b.lam(
        &["streams", "up", "own"],
        b.block(stmts, b.app2(b.loc("fin"), b.loc("up"), b.loc("own"))),
    )
}
