//! `Pool.invalidate` and `Pool.reserve` templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::slot::remove_and_close_slot_expr;
use super::{pool_pat, slot_pat, st_pat, wake_waiters_stmt};

fn find_slot_by_item(b: &ExprBuilder) -> Expr {
    let matches = b.bin(BinOp::Eq, b.loc("sv"), b.loc("item"));
    let pred = b.lam(
        &["sl"],
        b.match_(b.rget(b.loc("sl")), [arm(slot_pat(b), matches)]),
    );
    b.app2(b.global("List.Core", "find"), pred, b.loc("slots"))
}

pub(super) fn pool_invalidate_expr(b: &ExprBuilder) -> Expr {
    let marked = b.tuple([
        b.loc("sv"),
        b.loc("so"),
        b.loc("su"),
        b.loc("sr"),
        b.bool_(true),
        b.loc("sc"),
        b.loc("su_ms"),
    ]);
    let maybe_close = b.if_(
        b.bin(BinOp::Eq, b.loc("su"), b.int(0)),
        remove_and_close_slot_expr(b, b.loc("sl"), b.loc("so")),
        b.unit(),
    );
    let on_slot = b.block([b.rset(b.loc("sl"), marked)], maybe_close);
    let on_found = b.match_(b.rget(b.loc("sl")), [arm(slot_pat(b), on_slot)]);
    let on_st = b.match_(
        find_slot_by_item(b),
        [arm(b.some_pat("sl"), on_found), arm(Pat::Wild, b.unit())],
    );
    let on_pool = b.match_(b.rget(b.loc("st")), [arm(st_pat(b), on_st)]);
    b.lam(
        &["item", "pool"],
        b.match_(b.loc("pool"), [arm(pool_pat(b), on_pool)]),
    )
}

pub(super) fn pool_reserve_expr(b: &ExprBuilder) -> Expr {
    let wake_body = b.block(
        [
            b.rset(
                b.loc("st"),
                b.tuple([b.loc("slots"), b.list(vec![]), b.loc("closed")]),
            ),
            wake_waiters_stmt(b, b.loc("waits")),
        ],
        b.unit(),
    );
    let wake_all = b.match_(b.rget(b.loc("st")), [arm(st_pat(b), wake_body)]);
    let unreserved = b.tuple([
        b.loc("sv"),
        b.loc("so"),
        b.loc("su"),
        b.bool_(false),
        b.loc("si"),
        b.loc("sc"),
        b.loc("su_ms"),
    ]);
    let unreserve = b.match_(
        b.rget(b.loc("sl")),
        [arm(
            slot_pat(b),
            b.block([b.rset(b.loc("sl"), unreserved)], wake_all),
        )],
    );
    let reserved = b.tuple([
        b.loc("sv"),
        b.loc("so"),
        b.loc("su"),
        b.bool_(true),
        b.loc("si"),
        b.loc("sc"),
        b.loc("su_ms"),
    ]);
    let add_fin = b.app(b.global("Scope", "add_finalizer"), b.task(unreserve));
    let on_slot = b.block(
        [b.rset(b.loc("sl"), reserved), Stmt::Expr(add_fin)],
        b.unit(),
    );
    let on_found = b.match_(b.rget(b.loc("sl")), [arm(slot_pat(b), on_slot)]);
    let on_st = b.match_(
        find_slot_by_item(b),
        [arm(b.some_pat("sl"), on_found), arm(Pat::Wild, b.unit())],
    );
    let on_pool = b.match_(b.rget(b.loc("st")), [arm(st_pat(b), on_st)]);
    b.lam(
        &["item", "pool"],
        b.match_(b.loc("pool"), [arm(pool_pat(b), on_pool)]),
    )
}
