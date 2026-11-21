//! Batch `Queue` consumption IR templates (`take_all`, `take_between`, `collect`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat};
use super::shared::{
    empty_status_check, park_take_reg, q_drain_offers_stmt, q_state_pat, q_wake_woken_offers,
};

pub(super) fn take_all_template(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let next_state = b.tuple([
        b.loc("cap"),
        b.loc("strat"),
        b.var("v_items"),
        b.loc("status"),
        b.var("v_offers"),
        b.loc("takes"),
        b.loc("done_d"),
    ]);
    let take_non_empty = b.block(
        [
            b.var_decl("v_items", b.list(vec![])),
            b.var_decl("v_offers", b.loc("offers")),
            b.var_decl("woken_offers", b.list(vec![])),
            q_drain_offers_stmt(b),
            b.rset(b.loc("q"), next_state),
            b.let_("_w", q_wake_woken_offers(b)),
        ],
        b.ok(b.some(b.app2(b.global("NonEmpty", "cons"), b.loc("h"), b.loc("tl")))),
    );
    let on_empty = empty_status_check(b, b.ok(b.none()), park_take_reg(b));
    let try_step = b.no_preempt(b.match_(
        b.rget(b.loc("q")),
        [arm(
            q_state_pat(b),
            b.match_(
                b.loc("items"),
                [arm(cons_pat, take_non_empty), arm(Pat::Wild, on_empty)],
            ),
        )],
    ));
    let step = b.match_(
        try_step,
        [
            arm(b.ok_pat("res"), b.break_("qta", b.loc("res"))),
            arm(b.err_pat("td"), b.dawait(b.loc("td"))),
        ],
    );
    b.lam(&["q"], b.loop_ctrl("qta", step))
}

fn take_between_take_avail(b: &ExprBuilder) -> Expr {
    let take_cnt = b.if_(
        b.bin(BinOp::Lt, b.loc("avail"), b.loc("need")),
        b.loc("avail"),
        b.loc("need"),
    );
    let next_state = b.tuple([
        b.loc("cap"),
        b.loc("strat"),
        b.var("v_items"),
        b.loc("status"),
        b.var("v_offers"),
        b.loc("takes"),
        b.loc("done_d"),
    ]);
    let enough = b.bin(
        BinOp::Ge,
        b.app(b.global("List.Core", "length"), b.var("acc")),
        b.loc("mn"),
    );
    b.block(
        [
            b.let_("take_cnt", take_cnt),
            b.let_(
                "chunk",
                b.app2(
                    b.global("List.Core", "take"),
                    b.loc("take_cnt"),
                    b.loc("items"),
                ),
            ),
            b.var_decl(
                "v_items",
                b.app2(
                    b.global("List.Core", "drop"),
                    b.loc("take_cnt"),
                    b.loc("items"),
                ),
            ),
            b.var_decl("v_offers", b.loc("offers")),
            b.var_decl("woken_offers", b.list(vec![])),
            q_drain_offers_stmt(b),
            b.rset(b.loc("q"), next_state),
            b.let_("_w", q_wake_woken_offers(b)),
            b.assign(
                "acc",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("acc"),
                    b.loc("chunk"),
                ),
            ),
        ],
        b.if_(enough, b.ok(b.some(b.some(b.var("acc")))), b.ok(b.none())),
    )
}

pub(super) fn take_between_template(b: &ExprBuilder) -> Expr {
    let done_with_acc = b.match_(
        b.var("acc"),
        [
            arm(
                Pat::List(vec![], None),
                empty_status_check(
                    b,
                    b.ok(b.some(b.none())),
                    b.ok(b.some(b.some(b.list(vec![])))),
                ),
            ),
            arm(Pat::Wild, b.ok(b.some(b.some(b.var("acc"))))),
        ],
    );
    let on_open_empty = b.if_(
        b.bin(BinOp::Ge, b.loc("got"), b.loc("mn")),
        b.ok(b.some(b.some(b.var("acc")))),
        park_take_reg(b),
    );
    let on_no_avail = b.match_(
        b.loc("status"),
        [
            arm(b.some_pat("_oc"), done_with_acc),
            arm(Pat::Wild, on_open_empty),
        ],
    );
    let check_need = b.if_(
        b.bin(BinOp::Le, b.loc("need"), b.int(0)),
        b.ok(b.some(b.some(b.var("acc")))),
        b.if_(
            b.bin(BinOp::Gt, b.loc("avail"), b.int(0)),
            take_between_take_avail(b),
            on_no_avail,
        ),
    );
    let try_step = b.no_preempt(b.match_(
        b.rget(b.loc("q")),
        [arm(
            q_state_pat(b),
            b.block(
                [
                    b.let_("got", b.app(b.global("List.Core", "length"), b.var("acc"))),
                    b.let_("need", b.sub_i(b.loc("mx"), b.loc("got"))),
                    b.let_(
                        "avail",
                        b.app(b.global("List.Core", "length"), b.loc("items")),
                    ),
                ],
                check_need,
            ),
        )],
    ));
    let on_opt_done = b.match_(
        b.loc("opt_done"),
        [
            arm(b.some_pat("res"), b.break_("qtb", b.loc("res"))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let step = b.match_(
        try_step,
        [
            arm(b.ok_pat("opt_done"), on_opt_done),
            arm(b.err_pat("td"), b.dawait(b.loc("td"))),
        ],
    );
    let body = b.block(
        [b.var_decl("acc", b.list(vec![]))],
        b.if_(
            b.bin(BinOp::Lt, b.loc("mx"), b.loc("mn")),
            b.some(b.list(vec![])),
            b.loop_ctrl("qtb", step),
        ),
    );
    b.lam(&["mn", "mx", "q"], body)
}

pub(super) fn collect_template(b: &ExprBuilder) -> Expr {
    let push_x = b.block(
        [b.assign(
            "acc",
            b.app2(
                b.global("List.Core", "append"),
                b.var("acc"),
                b.list([b.loc("x")]),
            ),
        )],
        b.unit(),
    );
    let step = b.match_(
        b.app(b.global("Queue", "take"), b.loc("q")),
        [
            arm(b.some_pat("x"), push_x),
            arm(Pat::Wild, b.break_("qcol", b.var("acc"))),
        ],
    );
    b.lam(
        &["q"],
        b.block(
            [b.var_decl("acc", b.list(vec![]))],
            b.loop_ctrl("qcol", step),
        ),
    )
}
