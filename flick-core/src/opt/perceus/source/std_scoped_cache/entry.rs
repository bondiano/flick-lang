//! Entry lifecycle, TTL computation, eviction, and construction templates for `ScopedCache`.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_scoped_res::{acquire_holder_expr, retire_holder_expr};
use super::super::{Expr, Pat, Stmt};
use super::{drop_key, find_key, is_expired, now_ms};

pub(super) fn retire_entry_expr(b: &ExprBuilder, ent_expr: Expr) -> Expr {
    let on_res = b.match_(
        b.loc("res"),
        [
            arm(b.ok_pat("h"), retire_holder_expr(b, b.loc("h"))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let on_pair = b.match_(b.loc("pair"), [arm(b.tup_pat(&["res", "_exp"]), on_res)]);
    let on_state = b.match_(
        b.loc("st"),
        [arm(b.some_pat("pair"), on_pair), arm(Pat::Wild, b.unit())],
    );
    b.match_(b.rget(ent_expr), [arm(b.tup_pat(&["st", "_wd"]), on_state)])
}

pub(super) fn retire_key_and_evict_expr(b: &ExprBuilder, key: Expr, new_ent: Expr) -> Expr {
    let next0 = b.app2(
        b.global("List.Core", "append"),
        drop_key(b, b.rget(b.loc("ents")), key.clone()),
        b.list([b.tuple([key.clone(), new_ent])]),
    );
    let over_cond = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("cap"), b.int(0)),
        b.bin(BinOp::Gt, b.loc("n"), b.loc("cap")),
    );
    let over = b.if_(over_cond, b.sub_i(b.loc("n"), b.loc("cap")), b.int(0));
    let retire_old_pair = b.match_(
        b.loc("op"),
        [arm(
            b.tup_pat(&["_ok", "oe"]),
            retire_entry_expr(b, b.loc("oe")),
        )],
    );
    let retire_old = b.match_(
        b.loc("old_opt"),
        [
            arm(b.some_pat("op"), retire_old_pair),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let retire_evicted = b.for_ctrl(
        "scev",
        "p",
        b.loc("evicted"),
        b.match_(
            b.loc("p"),
            [arm(
                b.tup_pat(&["_ek", "ee"]),
                retire_entry_expr(b, b.loc("ee")),
            )],
        ),
    );
    b.block(
        [
            b.let_("old_opt", find_key(b, b.loc("ents"), key)),
            b.let_("next0", next0),
            b.let_("n", b.app(b.global("List.Core", "length"), b.loc("next0"))),
            b.let_("over", over),
            b.let_(
                "evicted",
                b.app2(b.global("List.Core", "take"), b.loc("over"), b.loc("next0")),
            ),
            b.let_(
                "kept",
                b.app2(b.global("List.Core", "drop"), b.loc("over"), b.loc("next0")),
            ),
            b.rset(b.loc("ents"), b.loc("kept")),
            Stmt::Expr(retire_old),
            b.let_("_ev", retire_evicted),
        ],
        b.unit(),
    )
}

pub(super) fn compute_ttl(b: &ExprBuilder, res_expr: Expr) -> Expr {
    let ok_exit = b.match_(
        b.loc("h"),
        [arm(
            b.tup_pat(&["hv", "_ho", "_hm"]),
            b.app(b.global("Effect.Core", "exit"), b.task(b.loc("hv"))),
        )],
    );
    let err_exit = b.app(
        b.global("Effect.Core", "exit"),
        b.task(b.app(b.global("Failure", "rethrow"), b.loc("c"))),
    );
    let exit_for_ttl = b.match_(
        res_expr,
        [arm(b.ok_pat("h"), ok_exit), arm(b.err_pat("c"), err_exit)],
    );
    let deadline = b.bin(BinOp::Arith(Arith::Add, Num::Float), now_ms(b), b.loc("ms"));
    let dur = b.app2(b.loc("tf"), b.loc("exv"), b.loc("k"));
    let millis_match = b.match_(
        b.app(b.global("Duration", "to_millis"), dur),
        [
            arm(b.some_pat("ms"), b.some(deadline)),
            arm(Pat::Wild, b.none()),
        ],
    );
    let on_tf = b.block([b.let_("exv", exit_for_ttl)], millis_match);
    b.match_(
        b.loc("ttl"),
        [arm(b.some_pat("tf"), on_tf), arm(Pat::Wild, b.none())],
    )
}

pub(super) fn acquire_entry_expr(b: &ExprBuilder) -> Expr {
    // Acquires fresh holder, swaps it in (retiring old & evicted), returns Result Cause Holder.
    let acq = acquire_holder_expr(
        b,
        b.task(b.app(b.loc("lkp"), b.loc("k"))),
        Some(b.loc("env")),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [
            arm(b.some_pat("c"), b.err(b.loc("c"))),
            arm(
                Pat::Wild,
                b.err(b.app(b.global("Cause", "die"), b.str_("sc"))),
            ),
        ],
    );
    let res = b.match_(
        b.app(b.global("Exit", "value"), b.loc("ex")),
        [
            arm(b.some_pat("h"), b.ok(b.loc("h"))),
            arm(Pat::Wild, on_fail),
        ],
    );
    b.block(
        [
            b.let_("wd", b.dmake()),
            b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.task(acq))),
            b.let_("res", res),
            b.let_("exp", compute_ttl(b, b.loc("res"))),
            b.let_("pair", b.tuple([b.loc("res"), b.loc("exp")])),
            b.dsucceed(b.loc("pair"), b.loc("wd")),
            b.let_(
                "ent",
                b.rmake(b.tuple([b.some(b.loc("pair")), b.loc("wd")])),
            ),
            Stmt::Expr(retire_key_and_evict_expr(b, b.loc("k"), b.loc("ent"))),
        ],
        b.loc("res"),
    )
}

fn unpack_holder(b: &ExprBuilder, res: Expr) -> Expr {
    b.match_(
        res,
        [
            arm(b.ok_pat("h"), b.loc("h")),
            arm(
                b.err_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        ],
    )
}

pub(super) fn get_or_acquire_holder(b: &ExprBuilder) -> Expr {
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["res", "exp"]),
            b.if_(
                is_expired(b, b.loc("exp")),
                unpack_holder(b, acquire_entry_expr(b)),
                unpack_holder(b, b.loc("res")),
            ),
        )],
    );
    let on_inflight = b.match_(
        b.dawait(b.loc("wd")),
        [arm(
            b.tup_pat(&["res", "_exp"]),
            unpack_holder(b, b.loc("res")),
        )],
    );
    let on_state = b.match_(
        b.loc("st"),
        [
            arm(b.some_pat("pair"), on_pair),
            arm(Pat::Wild, on_inflight),
        ],
    );
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(b.tup_pat(&["st", "wd"]), on_state)],
    );
    let on_found = b.match_(b.loc("found"), [arm(b.tup_pat(&["_ek", "ent"]), on_ent)]);
    b.match_(
        find_key(b, b.loc("ents"), b.loc("k")),
        [
            arm(b.some_pat("found"), on_found),
            arm(Pat::Wild, unpack_holder(b, acquire_entry_expr(b))),
        ],
    )
}

pub(super) fn make_sc_expr(b: &ExprBuilder, ttl_expr: Expr) -> Expr {
    let retire_one = b.match_(
        b.loc("p"),
        [arm(
            b.tup_pat(&["_ek", "ee"]),
            retire_entry_expr(b, b.loc("ee")),
        )],
    );
    let close_all = b.block(
        [
            b.let_("all", b.rget(b.loc("ents"))),
            b.rset(b.loc("ents"), b.list(vec![])),
            b.let_("_cl", b.for_ctrl("sccl", "p", b.loc("all"), retire_one)),
        ],
        b.unit(),
    );
    let cache_tuple = b.tuple([
        b.field(b.loc("opts"), "lookup"),
        b.field(b.loc("opts"), "capacity"),
        ttl_expr,
        b.loc("env"),
        b.loc("ents"),
    ]);
    b.lam(
        &["opts"],
        b.block(
            [
                b.let_("env", b.app(b.loc("env_capture"), b.unit())),
                b.let_("ents", b.rmake(b.list(vec![]))),
                Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(close_all))),
            ],
            cache_tuple,
        ),
    )
}
