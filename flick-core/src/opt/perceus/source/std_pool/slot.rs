//! Slot lifecycle, refill, release, and pool construction templates for `Pool`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::{now_ms, slot_pat, st_pat, wake_waiters_stmt};

pub(super) fn acquire_slot_expr(b: &ExprBuilder, init_in_use: i32) -> Expr {
    let run_acq = b.task(b.app2(b.loc("env_run"), b.loc("env"), b.loc("acq")));
    let run_in_owner = b.task(b.app2(b.loc("owner_run"), b.loc("ho"), run_acq));
    let on_ok = b.block(
        [b.let_("t0", now_ms(b))],
        b.rmake(b.tuple([
            b.loc("hv"),
            b.loc("ho"),
            b.int(init_in_use),
            b.bool_(false),
            b.bool_(false),
            b.loc("t0"),
            b.loc("t0"),
        ])),
    );
    let close_and_rethrow = b.block(
        [Stmt::Expr(b.app2(
            b.loc("owner_close"),
            b.loc("ho"),
            b.some(b.loc("hc")),
        ))],
        b.app(b.global("Failure", "rethrow"), b.loc("hc")),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("hex")),
        [
            arm(b.some_pat("hc"), close_and_rethrow),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("pool acquire")),
            ),
        ],
    );
    let tail = b.match_(
        b.app(b.global("Exit", "value"), b.loc("hex")),
        [arm(b.some_pat("hv"), on_ok), arm(Pat::Wild, on_fail)],
    );
    b.block(
        [
            b.let_("ho", b.app(b.loc("owner_new"), b.unit())),
            b.let_("hex", b.app(b.global("Effect.Core", "exit"), run_in_owner)),
        ],
        tail,
    )
}

pub(super) fn refill_to_min_expr(b: &ExprBuilder) -> Expr {
    let close_new = b.match_(
        b.rget(b.loc("nsl")),
        [arm(
            slot_pat(b),
            b.app2(b.loc("owner_close"), b.loc("so"), b.none()),
        )],
    );
    let append_slots = b.app2(
        b.global("List.Core", "append"),
        b.loc("slots2"),
        b.list([b.loc("nsl")]),
    );
    let store_new = b.block(
        [
            b.rset(
                b.loc("st"),
                b.tuple([append_slots, b.list(vec![]), b.bool_(false)]),
            ),
            wake_waiters_stmt(b, b.loc("waits2")),
        ],
        b.unit(),
    );
    let commit_slot = b.match_(
        b.rget(b.loc("st")),
        [arm(
            b.tup_pat(&["slots2", "waits2", "closed2"]),
            b.if_(b.loc("closed2"), close_new, store_new),
        )],
    );
    let acquire_one = b.block(
        [
            b.let_("nsl", acquire_slot_expr(b, 0)),
            Stmt::Expr(commit_slot),
        ],
        b.unit(),
    );
    let at_min = b.bin(
        BinOp::Ge,
        b.app(b.global("List.Core", "length"), b.loc("slots")),
        b.loc("mn"),
    );
    let done = b.bin(BinOp::Or, b.loc("closed"), at_min);
    let step = b.match_(
        b.rget(b.loc("st")),
        [arm(
            st_pat(b),
            b.if_(done, b.break_("prf", b.unit()), acquire_one),
        )],
    );
    b.loop_ctrl("prf", step)
}

pub(super) fn remove_and_close_slot_expr(
    b: &ExprBuilder,
    target_slot: Expr,
    target_owner: Expr,
) -> Expr {
    let keep_pred = b.lam(
        &["x"],
        b.not(b.app2(b.global("Ref", "same"), b.loc("x"), target_slot)),
    );
    let filtered = b.app2(b.global("List.Core", "filter"), keep_pred, b.loc("slots"));
    let detach_body = b.block(
        [
            b.let_("slots2", filtered),
            b.rset(
                b.loc("st"),
                b.tuple([b.loc("slots2"), b.list(vec![]), b.loc("closed")]),
            ),
            wake_waiters_stmt(b, b.loc("waits")),
        ],
        b.unit(),
    );
    let detach = b.match_(b.rget(b.loc("st")), [arm(st_pat(b), detach_body)]);
    b.block(
        [
            Stmt::Expr(detach),
            Stmt::Expr(b.app2(b.loc("owner_close"), target_owner, b.none())),
        ],
        refill_to_min_expr(b),
    )
}

pub(super) fn defer_release_expr(b: &ExprBuilder, slot_ref: Expr) -> Expr {
    let is_fatal = b.bin(
        BinOp::Or,
        b.app(b.global("Cause", "has_dies"), b.loc("c")),
        b.app(b.global("Cause", "has_interrupts"), b.loc("c")),
    );
    let broken = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("exit")),
        [
            arm(b.some_pat("c"), is_fatal),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
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
    let wake_waiters = b.match_(b.rget(b.loc("st")), [arm(st_pat(b), wake_body)]);
    let should_evict = b.bin(
        BinOp::And,
        b.loc("ninv"),
        b.bin(BinOp::Eq, b.loc("nu"), b.int(0)),
    );
    let updated_slot = b.tuple([
        b.loc("sv"),
        b.loc("so"),
        b.loc("nu"),
        b.loc("sr"),
        b.loc("ninv"),
        b.loc("sc"),
        now_ms(b),
    ]);
    let after_update = b.if_(
        should_evict,
        remove_and_close_slot_expr(b, slot_ref.clone(), b.loc("so")),
        wake_waiters,
    );
    let on_slot = b.block(
        [
            b.let_("nu", b.sub_i(b.loc("su"), b.int(1))),
            b.let_("ninv", b.bin(BinOp::Or, b.loc("si"), b.loc("broken"))),
            b.rset(slot_ref.clone(), updated_slot),
        ],
        after_update,
    );
    let body = b.match_(b.rget(slot_ref), [arm(slot_pat(b), on_slot)]);
    b.lam(&["exit"], b.block([b.let_("broken", broken)], body))
}

pub(super) fn make_pool_expr(b: &ExprBuilder, mn: Expr, mx: Expr, ttl: Expr, byc: Expr) -> Expr {
    let close_slot = b.match_(
        b.rget(b.loc("sl")),
        [arm(
            slot_pat(b),
            b.app2(b.loc("owner_close"), b.loc("so"), b.none()),
        )],
    );
    let close_all_body = b.block(
        [
            b.rset(
                b.loc("st"),
                b.tuple([b.list(vec![]), b.list(vec![]), b.bool_(true)]),
            ),
            wake_waiters_stmt(b, b.loc("waits")),
            b.let_("_cl", b.for_ctrl("pcl", "sl", b.loc("slots"), close_slot)),
        ],
        b.unit(),
    );
    let close_all = b.match_(b.rget(b.loc("st")), [arm(st_pat(b), close_all_body)]);
    let conc = b.match_(
        b.field(b.loc("opts"), "concurrency"),
        [arm(b.some_pat("n"), b.loc("n")), arm(Pat::Wild, b.int(1))],
    );
    let init_st = b.rmake(b.tuple([b.list(vec![]), b.list(vec![]), b.bool_(false)]));
    let pool_tuple = b.tuple([
        b.loc("acq"),
        b.loc("mn"),
        b.loc("mx"),
        b.loc("conc"),
        b.loc("ttl"),
        b.loc("byc"),
        b.loc("env"),
        b.loc("st"),
    ]);
    b.lam(
        &["opts"],
        b.block(
            [
                b.let_("acq", b.field(b.loc("opts"), "acquire")),
                b.let_("mn", mn),
                b.let_("mx", mx),
                b.let_("conc", conc),
                b.let_("ttl", ttl),
                b.let_("byc", byc),
                b.let_("env", b.app(b.loc("env_capture"), b.unit())),
                b.let_("st", init_st),
                Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(close_all))),
                Stmt::Expr(refill_to_min_expr(b)),
            ],
            pool_tuple,
        ),
    )
}
