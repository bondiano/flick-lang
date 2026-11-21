//! `Pool.get`, free-slot search, and TTL expiration purge templates.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::slot::{acquire_slot_expr, defer_release_expr};
use super::{now_ms, pool_pat, slot_pat, st_pat};

fn find_free_slot(b: &ExprBuilder, slots: Expr) -> Expr {
    let available = b.bin(
        BinOp::And,
        b.not(b.loc("sr")),
        b.bin(BinOp::Lt, b.loc("su"), b.loc("conc")),
    );
    let is_free = b.bin(BinOp::And, b.not(b.loc("si")), available);
    let pred = b.lam(
        &["sl"],
        b.match_(b.rget(b.loc("sl")), [arm(slot_pat(b), is_free)]),
    );
    b.app2(b.global("List.Core", "find"), pred, slots)
}

fn purge_slot_expr(b: &ExprBuilder) -> Expr {
    let base = b.if_(b.loc("byc"), b.loc("sc"), b.loc("su_ms"));
    let age = b.bin(
        BinOp::Arith(Arith::Sub, Num::Float),
        b.loc("now"),
        b.loc("base"),
    );
    let idle_expired = b.bin(
        BinOp::And,
        b.bin(BinOp::Eq, b.loc("su"), b.int(0)),
        b.bin(BinOp::Ge, age, b.loc("tms")),
    );
    let can_evict = b.bin(
        BinOp::And,
        b.bin(BinOp::Gt, b.var("rem"), b.loc("mn")),
        idle_expired,
    );
    let push_dead = b.app2(
        b.global("List.Core", "append"),
        b.var("dead"),
        b.list([b.loc("so")]),
    );
    let on_evict = b.block(
        [
            b.assign("rem", b.sub_i(b.var("rem"), b.int(1))),
            b.assign("dead", push_dead),
        ],
        b.unit(),
    );
    let push_kept = b.app2(
        b.global("List.Core", "append"),
        b.var("kept"),
        b.list([b.loc("sl")]),
    );
    let on_keep = b.block([b.assign("kept", push_kept)], b.unit());
    let body = b.block([b.let_("base", base)], b.if_(can_evict, on_evict, on_keep));
    b.match_(b.rget(b.loc("sl")), [arm(slot_pat(b), body)])
}

fn purge_expired_stmt(b: &ExprBuilder) -> Stmt {
    let close_dead = b.for_ctrl(
        "pcd",
        "o",
        b.var("dead"),
        b.app2(b.loc("owner_close"), b.loc("o"), b.none()),
    );
    let scan_body = b.block(
        [
            b.var_decl("kept", b.list(vec![])),
            b.var_decl(
                "rem",
                b.app(b.global("List.Core", "length"), b.loc("slots")),
            ),
            b.var_decl("dead", b.list(vec![])),
            b.let_(
                "_pg",
                b.for_ctrl("ppg", "sl", b.loc("slots"), purge_slot_expr(b)),
            ),
            b.rset(
                b.loc("st"),
                b.tuple([b.var("kept"), b.loc("waits"), b.loc("closed")]),
            ),
            b.let_("_cd", close_dead),
        ],
        b.unit(),
    );
    let on_tms = b.block(
        [b.let_("now", now_ms(b))],
        b.match_(b.rget(b.loc("st")), [arm(st_pat(b), scan_body)]),
    );
    let on_ttl = b.match_(
        b.app(b.global("Duration", "to_millis"), b.loc("d")),
        [arm(b.some_pat("tms"), on_tms), arm(Pat::Wild, b.unit())],
    );
    Stmt::Expr(b.match_(
        b.loc("ttl"),
        [arm(b.some_pat("d"), on_ttl), arm(Pat::Wild, b.unit())],
    ))
}

fn lease_free_slot_expr(b: &ExprBuilder) -> Expr {
    let updated = b.tuple([
        b.loc("sv"),
        b.loc("so"),
        b.add_i(b.loc("su"), b.int(1)),
        b.loc("sr"),
        b.loc("si"),
        b.loc("sc"),
        now_ms(b),
    ]);
    let defer = b.app(
        b.global("Scope", "defer"),
        defer_release_expr(b, b.loc("sl")),
    );
    let on_slot = b.block(
        [b.rset(b.loc("sl"), updated), Stmt::Expr(defer)],
        b.break_("pget", b.loc("sv")),
    );
    b.match_(b.rget(b.loc("sl")), [arm(slot_pat(b), on_slot)])
}

fn grow_pool_expr(b: &ExprBuilder) -> Expr {
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.loc("slots2"),
        b.list([b.loc("nsl")]),
    );
    let save_slot = b.block(
        [b.rset(
            b.loc("st"),
            b.tuple([appended, b.loc("waits2"), b.loc("closed2")]),
        )],
        b.unit(),
    );
    let commit = b.match_(
        b.rget(b.loc("st")),
        [arm(b.tup_pat(&["slots2", "waits2", "closed2"]), save_slot)],
    );
    let defer = b.app(
        b.global("Scope", "defer"),
        defer_release_expr(b, b.loc("nsl")),
    );
    let ret = b.match_(
        b.rget(b.loc("nsl")),
        [arm(slot_pat(b), b.break_("pget", b.loc("sv")))],
    );
    b.block(
        [
            b.let_("nsl", acquire_slot_expr(b, 1)),
            Stmt::Expr(commit),
            Stmt::Expr(defer),
        ],
        ret,
    )
}

fn wait_pool_expr(b: &ExprBuilder) -> Expr {
    let waits2 = b.app2(
        b.global("List.Core", "append"),
        b.loc("waits"),
        b.list([b.loc("wd")]),
    );
    b.block(
        [
            b.let_("wd", b.dmake()),
            b.rset(
                b.loc("st"),
                b.tuple([b.loc("slots"), waits2, b.loc("closed")]),
            ),
        ],
        b.dawait(b.loc("wd")),
    )
}

pub(super) fn pool_get_expr(b: &ExprBuilder) -> Expr {
    let has_room = b.bin(
        BinOp::Lt,
        b.app(b.global("List.Core", "length"), b.loc("slots")),
        b.loc("mx"),
    );
    let on_none = b.if_(has_room, grow_pool_expr(b), wait_pool_expr(b));
    let on_open = b.match_(
        find_free_slot(b, b.loc("slots")),
        [
            arm(b.some_pat("sl"), lease_free_slot_expr(b)),
            arm(Pat::Wild, on_none),
        ],
    );
    let on_st = b.if_(
        b.loc("closed"),
        b.app(b.global("Effect.Core", "die"), b.str_("Pool is closed")),
        on_open,
    );
    let step = b.block(
        [purge_expired_stmt(b)],
        b.match_(b.rget(b.loc("st")), [arm(st_pat(b), on_st)]),
    );
    b.lam(
        &["pool"],
        b.match_(b.loc("pool"), [arm(pool_pat(b), b.loop_ctrl("pget", step))]),
    )
}
