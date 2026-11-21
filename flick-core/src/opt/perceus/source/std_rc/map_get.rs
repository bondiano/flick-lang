//! `RcMap` lookup helpers and `flickGet`/`getOption` templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, ExprKind, Pat, Stmt};
use super::rc_entry_release;

pub(super) fn rc_map_find(b: &ExprBuilder, entries: Expr, key: Expr) -> Expr {
    let pred = b.lam(
        &["p"],
        b.match_(
            b.loc("p"),
            [arm(
                b.tup_pat(&["ek", "_ee"]),
                b.bin(BinOp::Eq, b.loc("ek"), key),
            )],
        ),
    );
    b.app2(b.global("List.Core", "find"), pred, entries)
}

pub(super) fn rc_map_remove_entry(b: &ExprBuilder, map_ref: Expr, ent: Expr) -> Expr {
    let pred = b.lam(
        &["p"],
        b.match_(
            b.loc("p"),
            [arm(
                b.tup_pat(&["_ek", "ee"]),
                b.not(b.app2(b.global("Ref", "same"), b.loc("ee"), ent)),
            )],
        ),
    );
    let kept = b.app2(
        b.global("List.Core", "filter"),
        pred,
        b.rget(map_ref.clone()),
    );
    b.block([b.rset(map_ref, kept)], b.unit())
}

fn idle_for_key(b: &ExprBuilder) -> Expr {
    b.match_(
        b.loc("idle"),
        [
            arm(b.some_pat("f"), b.some(b.app(b.loc("f"), b.loc("k")))),
            arm(Pat::Wild, b.none()),
        ],
    )
}

fn rc_map_acquire_new(b: &ExprBuilder, on_evict: Expr) -> Expr {
    let exceeded = b.at(ExprKind::Con(ExprBuilder::con(
        "RcMap",
        "ExceededCapacity",
        "ExceededCapacity",
        1,
    )));
    let cap_full = b.bin(
        BinOp::Ge,
        b.app(b.global("List.Core", "length"), b.rget(b.loc("ents"))),
        b.loc("c"),
    );
    let cap_check = b.match_(
        b.loc("cap"),
        [
            arm(
                b.some_pat("c"),
                b.if_(
                    cap_full,
                    b.app(
                        b.global("Fail", "fail"),
                        b.app(exceeded, b.str_("RcMap exceeded capacity")),
                    ),
                    b.unit(),
                ),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let run_lkp = b.task(b.app2(
        b.loc("env_run"),
        b.loc("env"),
        b.task(b.app(b.loc("lkp"), b.loc("k"))),
    ));
    let run_in_owner = b.task(b.app2(b.loc("owner_run"), b.loc("ho"), run_lkp));
    let new_ent = b.rmake(b.tuple([
        b.loc("hv"),
        b.loc("ho"),
        b.int(1),
        b.bool_(false),
        b.bool_(false),
        b.int(0),
    ]));
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.rget(b.loc("ents")),
        b.list([b.tuple([b.loc("k"), b.loc("ent")])]),
    );
    let rel = rc_entry_release(b, b.loc("ent"), idle_for_key(b), b.loc("pown"), on_evict);
    let on_ok = b.block(
        [
            b.let_("ent", new_ent),
            b.rset(b.loc("ents"), appended),
            Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(rel))),
        ],
        b.loc("hv"),
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
                b.app(b.global("Effect.Core", "die"), b.str_("rcmap")),
            ),
        ],
    );
    let tail = b.match_(
        b.app(b.global("Exit", "value"), b.loc("hex")),
        [arm(b.some_pat("hv"), on_ok), arm(Pat::Wild, on_fail)],
    );
    b.block(
        [
            Stmt::Expr(cap_check),
            b.let_("ho", b.app(b.loc("owner_new"), b.unit())),
            b.let_("hex", b.app(b.global("Effect.Core", "exit"), run_in_owner)),
        ],
        tail,
    )
}

fn reuse_entry_expr(b: &ExprBuilder, on_evict: Expr, ret_val: Expr) -> Expr {
    let updated = b.tuple([
        b.loc("ev"),
        b.loc("eo"),
        b.add_i(b.loc("h"), b.int(1)),
        b.bool_(false),
        b.bool_(false),
        b.add_i(b.loc("ep"), b.int(1)),
    ]);
    let rel = rc_entry_release(b, b.loc("ent"), idle_for_key(b), b.loc("pown"), on_evict);
    b.block(
        [
            b.rset(b.loc("ent"), updated),
            Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(rel))),
        ],
        ret_val,
    )
}

pub(super) fn rc_map_get(b: &ExprBuilder) -> Expr {
    let on_evict = rc_map_remove_entry(b, b.loc("ents"), b.loc("ent"));
    let acq_new = rc_map_acquire_new(b, on_evict.clone());
    let reuse = reuse_entry_expr(b, on_evict, b.loc("ev"));
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(
            b.tup_pat(&["ev", "eo", "h", "ret", "cls", "ep"]),
            b.if_(
                b.bin(BinOp::Or, b.loc("ret"), b.loc("cls")),
                acq_new.clone(),
                reuse,
            ),
        )],
    );
    let on_found = b.match_(b.loc("found"), [arm(b.tup_pat(&["_ek", "ent"]), on_ent)]);
    let body = b.match_(
        rc_map_find(b, b.rget(b.loc("ents")), b.loc("k")),
        [arm(b.some_pat("found"), on_found), arm(Pat::Wild, acq_new)],
    );
    b.lam(
        &["k", "m"],
        b.match_(
            b.loc("m"),
            [arm(
                b.tup_pat(&["lkp", "idle", "cap", "env", "pown", "ents", "lk"]),
                b.app2(
                    b.global("Semaphore", "with_permit"),
                    b.loc("lk"),
                    b.task(body),
                ),
            )],
        ),
    )
}

pub(super) fn rc_map_get_option(b: &ExprBuilder) -> Expr {
    let on_evict = rc_map_remove_entry(b, b.loc("ents"), b.loc("ent"));
    let reuse = reuse_entry_expr(b, on_evict, b.some(b.loc("ev")));
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(
            b.tup_pat(&["ev", "eo", "h", "ret", "cls", "ep"]),
            b.if_(
                b.bin(BinOp::Or, b.loc("ret"), b.loc("cls")),
                b.none(),
                reuse,
            ),
        )],
    );
    let on_found = b.match_(b.loc("found"), [arm(b.tup_pat(&["_ek", "ent"]), on_ent)]);
    let body = b.match_(
        rc_map_find(b, b.rget(b.loc("ents")), b.loc("k")),
        [arm(b.some_pat("found"), on_found), arm(Pat::Wild, b.none())],
    );
    b.lam(
        &["k", "m"],
        b.match_(
            b.loc("m"),
            [arm(
                b.tup_pat(&["_lkp", "idle", "_cap", "_env", "pown", "ents", "lk"]),
                b.app2(
                    b.global("Semaphore", "with_permit"),
                    b.loc("lk"),
                    b.task(body),
                ),
            )],
        ),
    )
}
