//! Single-element `Queue` consumption IR templates (`clear`, `take`, `poll`, `peek`).

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat};
use super::shared::{
    empty_status_check, park_take_reg, q_drain_offers_stmt, q_state_pat, q_wake_woken_offers,
};

fn drain_and_store_block(b: &ExprBuilder, init_items: Expr, tail: Expr) -> Expr {
    let next_state = b.tuple([
        b.loc("cap"),
        b.loc("strat"),
        b.var("v_items"),
        b.loc("status"),
        b.var("v_offers"),
        b.loc("takes"),
        b.loc("done_d"),
    ]);
    b.block(
        [
            b.var_decl("v_items", init_items),
            b.var_decl("v_offers", b.loc("offers")),
            b.var_decl("woken_offers", b.list(vec![])),
            q_drain_offers_stmt(b),
            b.rset(b.loc("q"), next_state),
            b.let_("_w", q_wake_woken_offers(b)),
        ],
        tail,
    )
}

pub(super) fn clear_template(b: &ExprBuilder) -> Expr {
    let on_some_status = b.match_(
        b.loc("oc"),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(Pat::Wild, b.list(vec![])),
        ],
    );
    let on_empty = b.match_(
        b.loc("status"),
        [
            arm(b.some_pat("oc"), on_some_status),
            arm(Pat::Wild, b.list(vec![])),
        ],
    );
    let on_non_empty = drain_and_store_block(b, b.list(vec![]), b.loc("items"));
    let body = b.match_(
        b.loc("items"),
        [
            arm(Pat::List(vec![], None), on_empty),
            arm(Pat::Wild, on_non_empty),
        ],
    );
    b.lam(
        &["q"],
        b.no_preempt(b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), body)])),
    )
}

pub(super) fn take_one_template(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let on_head = drain_and_store_block(b, b.loc("tl"), b.ok(b.some(b.loc("h"))));
    let on_empty = empty_status_check(b, b.ok(b.none()), park_take_reg(b));
    let try_step = b.no_preempt(b.match_(
        b.rget(b.loc("q")),
        [arm(
            q_state_pat(b),
            b.match_(
                b.loc("items"),
                [arm(cons_pat, on_head), arm(Pat::Wild, on_empty)],
            ),
        )],
    ));
    let step = b.match_(
        try_step,
        [
            arm(b.ok_pat("res"), b.break_("qt1", b.loc("res"))),
            arm(b.err_pat("td"), b.dawait(b.loc("td"))),
        ],
    );
    b.lam(&["q"], b.loop_ctrl("qt1", step))
}

pub(super) fn poll_template(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let on_head = drain_and_store_block(b, b.loc("tl"), b.some(b.loc("h")));
    let body = b.match_(
        b.loc("items"),
        [arm(cons_pat, on_head), arm(Pat::Wild, b.none())],
    );
    b.lam(
        &["q"],
        b.no_preempt(b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), body)])),
    )
}

pub(super) fn peek_template(b: &ExprBuilder) -> Expr {
    let head_pat = Pat::List(vec![Pat::Bind(b.name("h"))], Some(Box::new(Pat::Wild)));
    let on_empty = empty_status_check(b, b.ok(b.none()), park_take_reg(b));
    let try_step = b.no_preempt(b.match_(
        b.rget(b.loc("q")),
        [arm(
            q_state_pat(b),
            b.match_(
                b.loc("items"),
                [
                    arm(head_pat, b.ok(b.some(b.loc("h")))),
                    arm(Pat::Wild, on_empty),
                ],
            ),
        )],
    ));
    let step = b.match_(
        try_step,
        [
            arm(b.ok_pat("res"), b.break_("qpk", b.loc("res"))),
            arm(b.err_pat("td"), b.dawait(b.loc("td"))),
        ],
    );
    b.lam(&["q"], b.loop_ctrl("qpk", step))
}
