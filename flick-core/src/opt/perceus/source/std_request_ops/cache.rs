//! `RequestResolver.with_cache` deduplication and LRU/FIFO cache combinator.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_request::{entry_pat, res_pat};
use super::super::{Expr, Pat, Stmt};
use super::clone_resolver;

fn drop_req(b: &ExprBuilder, xs: Expr, rq: Expr) -> Expr {
    let pred = b.lam(
        &["p"],
        b.match_(
            b.loc("p"),
            [arm(
                b.tup_pat(&["pk", "_pv"]),
                b.not(b.bin(BinOp::Eq, b.loc("pk"), rq)),
            )],
        ),
    );
    b.app2(b.global("List.Core", "filter"), pred, xs)
}

fn evict_expr(b: &ExprBuilder) -> Expr {
    let over = b.sub_i(
        b.app(b.global("List.Core", "length"), b.loc("cs")),
        b.loc("cap"),
    );
    let trimmed = b.app2(b.global("List.Core", "drop"), b.loc("over"), b.loc("cs"));
    let do_drop = b.block([b.rset(b.loc("cref"), trimmed)], b.unit());
    let body = b.if_(b.bin(BinOp::Gt, b.loc("over"), b.int(0)), do_drop, b.unit());
    b.no_preempt(b.block(
        [b.let_("cs", b.rget(b.loc("cref"))), b.let_("over", over)],
        body,
    ))
}

fn cache_hit_completed(b: &ExprBuilder) -> Expr {
    let promoted = b.app2(
        b.global("List.Core", "append"),
        drop_req(b, b.loc("cs"), b.loc("e_req")),
        b.list([b.loc("pair")]),
    );
    let bump_lru = b.if_(
        b.loc("is_lru"),
        b.block([b.rset(b.loc("cref"), promoted)], b.unit()),
        b.unit(),
    );
    b.block([Stmt::Expr(bump_lru)], b.some(b.loc("ex")))
}

fn mark_orig_uninterruptible(b: &ExprBuilder) -> Expr {
    let set_unint = b.block(
        [b.rset(
            b.loc("ost"),
            b.tuple([b.loc("oc"), b.bool_(true), b.loc("ob")]),
        )],
        b.unit(),
    );
    let update_st = b.match_(
        b.rget(b.loc("ost")),
        [arm(b.tup_pat(&["oc", "_ou", "ob"]), set_unint)],
    );
    let on_orig = b.match_(
        b.loc("orig"),
        [arm(b.tup_pat(&["_1", "_2", "_3", "ost", "_5"]), update_st)],
    );
    b.match_(
        b.loc("ce"),
        [arm(b.some_pat("orig"), on_orig), arm(Pat::Wild, b.unit())],
    )
}

fn cache_hit_inflight(b: &ExprBuilder) -> Expr {
    let waiters = b.app2(
        b.global("List.Core", "append"),
        b.loc("cw"),
        b.list([b.loc("e")]),
    );
    let updated_cell = b.tuple([b.loc("ce"), b.none(), waiters]);
    b.block(
        [
            Stmt::Expr(mark_orig_uninterruptible(b)),
            b.rset(b.loc("cell"), updated_cell),
        ],
        b.none(),
    )
}

fn cache_hit_arm(b: &ExprBuilder) -> Expr {
    let on_cex = b.match_(
        b.loc("cex"),
        [
            arm(b.some_pat("ex"), cache_hit_completed(b)),
            arm(Pat::Wild, cache_hit_inflight(b)),
        ],
    );
    let on_cell = b.match_(
        b.rget(b.loc("cell")),
        [arm(b.tup_pat(&["ce", "cex", "cw"]), on_cex)],
    );
    b.match_(b.loc("pair"), [arm(b.tup_pat(&["_pk", "cell"]), on_cell)])
}

fn cache_miss_complete_cb(b: &ExprBuilder) -> Expr {
    let cleared = b.tuple([b.none(), b.none(), b.list(vec![])]);
    let on_interrupt = b.block(
        [
            b.rset(
                b.loc("cref"),
                drop_req(b, b.rget(b.loc("cref")), b.loc("e_req")),
            ),
            b.rset(b.loc("cell"), cleared),
        ],
        b.unit(),
    );
    let stored = b.tuple([b.none(), b.some(b.loc("ex")), b.list(vec![])]);
    let on_result = b.block([b.rset(b.loc("cell"), stored)], b.unit());
    let update_cell = b.if_(
        b.app(b.global("Exit", "has_interrupts"), b.loc("ex")),
        on_interrupt,
        on_result,
    );
    let take_waiters = b.no_preempt(b.match_(
        b.rget(b.loc("cell")),
        [arm(
            b.tup_pat(&["_ce", "_cx", "cw"]),
            b.block([Stmt::Expr(update_cell)], b.loc("cw")),
        )],
    ));
    let notify_waiters = b.for_ctrl(
        "rcw",
        "pe",
        b.loc("ws"),
        b.app2(b.global("Request", "complete"), b.loc("ex"), b.loc("pe")),
    );
    b.lam(
        &["ex"],
        b.block(
            [
                b.let_("ws", take_waiters),
                Stmt::Expr(b.app(b.loc("prev_cmp"), b.loc("ex"))),
            ],
            notify_waiters,
        ),
    )
}

fn cache_miss_arm(b: &ExprBuilder) -> Expr {
    let new_cell = b.rmake(b.tuple([b.some(b.loc("e")), b.none(), b.list(vec![])]));
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.loc("cs"),
        b.list([b.tuple([b.loc("e_req"), b.loc("cell")])]),
    );
    b.block(
        [
            b.let_("cell", new_cell),
            b.rset(b.loc("cref"), appended),
            b.let_("prev_cmp", b.rget(b.loc("e_cmp"))),
            b.rset(b.loc("e_cmp"), cache_miss_complete_cb(b)),
        ],
        b.ok(b.bool_(true)),
    )
}

fn pre_check_expr(b: &ExprBuilder) -> Expr {
    let find_pred = b.lam(
        &["p"],
        b.match_(
            b.loc("p"),
            [arm(
                b.tup_pat(&["pk", "_pv"]),
                b.bin(BinOp::Eq, b.loc("pk"), b.loc("e_req")),
            )],
        ),
    );
    let hit_let = b.let_(
        "hit",
        b.app2(b.global("List.Core", "find"), find_pred, b.loc("cs")),
    );
    let dispatch_hit = b.match_(
        b.loc("hit"),
        [
            arm(b.some_pat("pair"), cache_hit_arm(b)),
            arm(Pat::Wild, cache_miss_arm(b)),
        ],
    );
    let lookup =
        b.no_preempt(b.block([b.let_("cs", b.rget(b.loc("cref"))), hit_let], dispatch_hit));
    let complete_cached = b.block(
        [Stmt::Expr(b.app2(
            b.global("Request", "complete"),
            b.loc("ex"),
            b.loc("e"),
        ))],
        b.bool_(false),
    );
    let decide = b.match_(
        b.loc("act"),
        [
            arm(b.ok_pat("_ad"), b.bool_(true)),
            arm(b.some_pat("ex"), complete_cached),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(entry_pat(b), b.block([b.let_("act", lookup)], decide))],
        ),
    )
}

pub(super) fn with_cache_template(b: &ExprBuilder) -> Expr {
    // Cache Ref: `List (req, ccell : Ref (entry_opt : Option Entry, exit_opt : Option Exit, waiters : List Entry))`
    let cached_run = b.lam(
        &["es", "k"],
        b.app2(
            b.global("Effect.Errors", "ensuring"),
            b.task(evict_expr(b)),
            b.task(b.app2(b.loc("r_run"), b.loc("es"), b.loc("k"))),
        ),
    );
    let is_lru = b.match_(
        b.field(b.loc("opts"), "strategy"),
        [
            arm(b.some_pat("s"), b.bin(BinOp::Eq, b.loc("s"), b.str_("lru"))),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let body = b.block(
        [
            b.let_("cap", b.field(b.loc("opts"), "capacity")),
            b.let_("is_lru", is_lru),
            b.let_("cref", b.rmake(b.list(vec![]))),
        ],
        clone_resolver(
            b,
            None,
            None,
            None,
            Some(b.some(pre_check_expr(b))),
            None,
            Some(cached_run),
        ),
    );
    b.lam(
        &["opts", "self_r"],
        b.match_(b.loc("self_r"), [arm(res_pat(b), body)]),
    )
}
