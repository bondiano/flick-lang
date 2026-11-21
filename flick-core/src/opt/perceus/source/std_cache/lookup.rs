//! `Cache` constructors, lookup (`get`/`getOption`/`getSuccess`), and `set` templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat};
use super::{
    cache_pat, compute_exp_expr, find_key_expr, is_expired_expr, put_entry_stmt, run_lookup_expr,
    unwrap_exit_expr,
};

pub(super) fn make_expr(b: &ExprBuilder) -> Expr {
    let ttl = b.match_(
        b.field(b.loc("opts"), "timeToLive"),
        [
            arm(b.some_pat("d"), b.some(b.lam(&["_ex", "_k"], b.loc("d")))),
            arm(Pat::Wild, b.none()),
        ],
    );
    let tuple = b.tuple([
        b.field(b.loc("opts"), "lookup"),
        b.field(b.loc("opts"), "capacity"),
        b.loc("ttl"),
        b.loc("env"),
        b.rmake(b.list(vec![])),
    ]);
    b.lam(
        &["opts"],
        b.block(
            [
                b.let_("env", b.app(b.loc("env_capture"), b.unit())),
                b.let_("ttl", ttl),
            ],
            tuple,
        ),
    )
}

pub(super) fn make_with_expr(b: &ExprBuilder) -> Expr {
    let tuple = b.tuple([
        b.loc("lkp"),
        b.field(b.loc("opts"), "capacity"),
        b.field(b.loc("opts"), "timeToLive"),
        b.loc("env"),
        b.rmake(b.list(vec![])),
    ]);
    b.lam(
        &["lkp", "opts"],
        b.block(
            [b.let_("env", b.app(b.loc("env_capture"), b.unit()))],
            tuple,
        ),
    )
}

pub(super) fn get_expr(b: &ExprBuilder) -> Expr {
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["ex", "exp"]),
            b.if_(
                is_expired_expr(b, b.loc("exp")),
                run_lookup_expr(b),
                unwrap_exit_expr(b, b.loc("ex")),
            ),
        )],
    );
    let on_inflight = b.match_(
        b.dawait(b.loc("wd")),
        [arm(
            b.tup_pat(&["ex", "_exp"]),
            unwrap_exit_expr(b, b.loc("ex")),
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
    let body = b.match_(
        find_key_expr(b, b.loc("ents"), b.loc("k")),
        [
            arm(b.some_pat("found"), on_found),
            arm(Pat::Wild, run_lookup_expr(b)),
        ],
    );
    b.lam(&["k", "c"], b.match_(b.loc("c"), [arm(cache_pat(b), body)]))
}

pub(super) fn get_option_expr(b: &ExprBuilder) -> Expr {
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["ex", "exp"]),
            b.if_(
                is_expired_expr(b, b.loc("exp")),
                b.none(),
                b.some(unwrap_exit_expr(b, b.loc("ex"))),
            ),
        )],
    );
    let on_inflight = b.match_(
        b.dawait(b.loc("wd")),
        [arm(
            b.tup_pat(&["ex", "_exp"]),
            b.some(unwrap_exit_expr(b, b.loc("ex"))),
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
    let body = b.match_(
        find_key_expr(b, b.loc("ents"), b.loc("k")),
        [arm(b.some_pat("found"), on_found), arm(Pat::Wild, b.none())],
    );
    b.lam(&["k", "c"], b.match_(b.loc("c"), [arm(cache_pat(b), body)]))
}

pub(super) fn get_success_expr(b: &ExprBuilder) -> Expr {
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["ex", "exp"]),
            b.if_(
                is_expired_expr(b, b.loc("exp")),
                b.none(),
                b.app(b.global("Exit", "value"), b.loc("ex")),
            ),
        )],
    );
    let on_state = b.match_(
        b.loc("st"),
        [arm(b.some_pat("pair"), on_pair), arm(Pat::Wild, b.none())],
    );
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(b.tup_pat(&["st", "_wd"]), on_state)],
    );
    let on_found = b.match_(b.loc("found"), [arm(b.tup_pat(&["_ek", "ent"]), on_ent)]);
    let body = b.match_(
        find_key_expr(b, b.loc("ents"), b.loc("k")),
        [arm(b.some_pat("found"), on_found), arm(Pat::Wild, b.none())],
    );
    b.lam(&["k", "c"], b.match_(b.loc("c"), [arm(cache_pat(b), body)]))
}

pub(super) fn set_expr(b: &ExprBuilder) -> Expr {
    let body = b.block(
        [
            b.let_(
                "ex",
                b.app(b.global("Effect.Core", "exit"), b.task(b.loc("v"))),
            ),
            b.let_(
                "exp",
                compute_exp_expr(b, b.loc("ttl"), b.loc("ex"), b.loc("k")),
            ),
            b.let_("wd", b.dmake()),
            b.let_("res", b.tuple([b.loc("ex"), b.loc("exp")])),
            b.dsucceed(b.loc("res"), b.loc("wd")),
            b.let_("ent", b.rmake(b.tuple([b.some(b.loc("res")), b.loc("wd")]))),
            put_entry_stmt(b, b.loc("k"), b.loc("ent")),
        ],
        b.unit(),
    );
    b.lam(
        &["k", "v", "c"],
        b.match_(b.loc("c"), [arm(cache_pat(b), body)]),
    )
}
