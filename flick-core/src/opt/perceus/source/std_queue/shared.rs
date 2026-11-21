//! Shared `Queue` state representation and wakeup/drain IR helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

// Representation: `Ref (cap, strat, items, status, offers, takes, done_def)`
// - `cap`: `-1` for unbounded, `>= 0` for bounded
// - `strat`: `0` Suspend, `1` Dropping, `2` Sliding
// - `status`: `None` open | `Some None` ended | `Some (Some cause)` failed/interrupted
// - `offers`: `List (Int, a, Deferred none Bool)` (id, item, waiter)
// - `takes`: `List (Deferred none Unit)` (broadcast wakeups)
// - `done_def`: `Deferred none (Option Cause)`
pub(super) fn q_state_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&[
        "cap", "strat", "items", "status", "offers", "takes", "done_d",
    ])
}

pub(super) fn q_make_expr(b: &ExprBuilder, cap: Expr, strat: Expr) -> Expr {
    b.rmake(b.tuple([
        cap,
        strat,
        b.list(vec![]),
        b.none(),
        b.list(vec![]),
        b.list(vec![]),
        b.dmake(),
    ]))
}

pub(super) fn q_wake_takes(b: &ExprBuilder, takes_expr: Expr) -> Stmt {
    b.let_(
        "_wt",
        b.for_ctrl(
            "qt",
            "td",
            takes_expr,
            b.block([b.dsucceed(b.unit(), b.loc("td"))], b.unit()),
        ),
    )
}

pub(super) fn q_wake_offers_false(b: &ExprBuilder, offers_expr: Expr) -> Stmt {
    let wake_one = b.match_(
        b.loc("ow"),
        [arm(
            b.tup_pat(&["_oid", "_ox", "od"]),
            b.block([b.dsucceed(b.bool_(false), b.loc("od"))], b.unit()),
        )],
    );
    b.let_("_wo", b.for_ctrl("qo", "ow", offers_expr, wake_one))
}

pub(super) fn q_wake_woken_offers(b: &ExprBuilder) -> Expr {
    b.for_ctrl(
        "qwo",
        "od",
        b.var("woken_offers"),
        b.block([b.dsucceed(b.bool_(true), b.loc("od"))], b.unit()),
    )
}

pub(super) fn q_drain_offers_stmt(b: &ExprBuilder) -> Stmt {
    // Assumes mutable vars `v_items`, `v_offers`, `woken_offers` exist and `cap` is in scope.
    let at_cap = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("cap"), b.int(0)),
        b.bin(
            BinOp::Ge,
            b.app(b.global("List.Core", "length"), b.var("v_items")),
            b.loc("cap"),
        ),
    );
    let pop_pat = Pat::List(
        vec![b.tup_pat(&["_oid", "ox", "od"])],
        Some(Box::new(Pat::Bind(b.name("orest")))),
    );
    let pop_offer = b.block(
        [
            b.assign(
                "v_items",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("v_items"),
                    b.list([b.loc("ox")]),
                ),
            ),
            b.assign("v_offers", b.loc("orest")),
            b.assign(
                "woken_offers",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("woken_offers"),
                    b.list([b.loc("od")]),
                ),
            ),
        ],
        b.unit(),
    );
    let pop_match = b.match_(
        b.var("v_offers"),
        [
            arm(Pat::List(vec![], None), b.break_("qdo", b.unit())),
            arm(pop_pat, pop_offer),
        ],
    );
    b.let_(
        "_qdo",
        b.loop_ctrl("qdo", b.if_(at_cap, b.break_("qdo", b.unit()), pop_match)),
    )
}

pub(super) fn q_finish_expr(b: &ExprBuilder, clear_items: bool, opt_cause: Expr) -> Expr {
    let next_items = if clear_items {
        b.list(vec![])
    } else {
        b.loc("items")
    };
    let next_state = b.tuple([
        b.loc("cap"),
        b.loc("strat"),
        next_items,
        b.some(b.loc("oc")),
        b.list(vec![]),
        b.list(vec![]),
        b.loc("done_d"),
    ]);
    let close_now = b.block(
        [
            b.let_("oc", opt_cause),
            b.rset(b.loc("q"), next_state),
            q_wake_offers_false(b, b.loc("offers")),
            q_wake_takes(b, b.loc("takes")),
            b.dsucceed(b.loc("oc"), b.loc("done_d")),
        ],
        b.bool_(true),
    );
    let body = b.match_(
        b.loc("status"),
        [
            arm(b.some_pat("_s"), b.bool_(false)),
            arm(Pat::Wild, close_now),
        ],
    );
    b.lam(
        &["q"],
        b.no_preempt(b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), body)])),
    )
}

pub(super) fn park_take_reg(b: &ExprBuilder) -> Expr {
    let next_takes = b.app2(
        b.global("List.Core", "append"),
        b.loc("takes"),
        b.list([b.loc("td")]),
    );
    let next_state = b.tuple([
        b.loc("cap"),
        b.loc("strat"),
        b.loc("items"),
        b.loc("status"),
        b.loc("offers"),
        next_takes,
        b.loc("done_d"),
    ]);
    b.block(
        [b.let_("td", b.dmake()), b.rset(b.loc("q"), next_state)],
        b.err(b.loc("td")),
    )
}

pub(super) fn empty_status_check(b: &ExprBuilder, on_ended: Expr, on_open: Expr) -> Expr {
    let on_some = b.match_(
        b.loc("oc"),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(Pat::Wild, on_ended),
        ],
    );
    b.match_(
        b.loc("status"),
        [arm(b.some_pat("oc"), on_some), arm(Pat::Wild, on_open)],
    )
}
