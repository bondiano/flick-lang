//! `Cache` membership, invalidation, refresh, and enumeration templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat};
use super::{cache_pat, drop_key_expr, find_key_expr, is_expired_expr, run_lookup_expr};

pub(super) fn has_expr(b: &ExprBuilder) -> Expr {
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["_ex", "exp"]),
            b.not(is_expired_expr(b, b.loc("exp"))),
        )],
    );
    let on_state = b.match_(
        b.loc("st"),
        [
            arm(b.some_pat("pair"), on_pair),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(b.tup_pat(&["st", "_wd"]), on_state)],
    );
    let on_found = b.match_(b.loc("found"), [arm(b.tup_pat(&["_ek", "ent"]), on_ent)]);
    let body = b.match_(
        find_key_expr(b, b.loc("ents"), b.loc("k")),
        [
            arm(b.some_pat("found"), on_found),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    b.lam(&["k", "c"], b.match_(b.loc("c"), [arm(cache_pat(b), body)]))
}

pub(super) fn invalidate_expr(b: &ExprBuilder) -> Expr {
    let body = b.block(
        [b.rset(
            b.loc("ents"),
            drop_key_expr(b, b.rget(b.loc("ents")), b.loc("k")),
        )],
        b.unit(),
    );
    b.lam(&["k", "c"], b.match_(b.loc("c"), [arm(cache_pat(b), body)]))
}

pub(super) fn invalidate_when_expr(b: &ExprBuilder) -> Expr {
    let do_drop = b.block(
        [b.rset(
            b.loc("ents"),
            drop_key_expr(b, b.rget(b.loc("ents")), b.loc("k")),
        )],
        b.bool_(true),
    );
    let check_val = b.if_(b.app(b.loc("pred"), b.loc("v")), do_drop, b.bool_(false));
    let on_live = b.match_(
        b.app(b.global("Exit", "value"), b.loc("ex")),
        [
            arm(b.some_pat("v"), check_val),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["ex", "exp"]),
            b.if_(is_expired_expr(b, b.loc("exp")), b.bool_(false), on_live),
        )],
    );
    let on_state = b.match_(
        b.loc("st"),
        [
            arm(b.some_pat("pair"), on_pair),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(b.tup_pat(&["st", "_wd"]), on_state)],
    );
    let on_found = b.match_(b.loc("found"), [arm(b.tup_pat(&["_ek", "ent"]), on_ent)]);
    let body = b.match_(
        find_key_expr(b, b.loc("ents"), b.loc("k")),
        [
            arm(b.some_pat("found"), on_found),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    b.lam(
        &["k", "pred", "c"],
        b.match_(b.loc("c"), [arm(cache_pat(b), body)]),
    )
}

pub(super) fn refresh_expr(b: &ExprBuilder) -> Expr {
    b.lam(
        &["k", "c"],
        b.match_(b.loc("c"), [arm(cache_pat(b), run_lookup_expr(b))]),
    )
}

pub(super) fn invalidate_all_expr(b: &ExprBuilder) -> Expr {
    let body = b.block([b.rset(b.loc("ents"), b.list(vec![]))], b.unit());
    b.lam(&["c"], b.match_(b.loc("c"), [arm(cache_pat(b), body)]))
}

pub(super) fn size_expr(b: &ExprBuilder) -> Expr {
    b.lam(
        &["c"],
        b.app(
            b.global("List.Core", "length"),
            b.app(b.global("Cache", "keys"), b.loc("c")),
        ),
    )
}

pub(super) fn keys_expr(b: &ExprBuilder) -> Expr {
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["_ex", "exp"]),
            b.if_(
                is_expired_expr(b, b.loc("exp")),
                b.unit(),
                b.yield_(b.loc("ek")),
            ),
        )],
    );
    let on_state = b.match_(
        b.loc("st"),
        [
            arm(b.some_pat("pair"), on_pair),
            arm(Pat::Wild, b.yield_(b.loc("ek"))),
        ],
    );
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(b.tup_pat(&["st", "_wd"]), on_state)],
    );
    let on_item = b.match_(b.loc("p"), [arm(b.tup_pat(&["ek", "ent"]), on_ent)]);
    let body = b.gen_strict(b.each("p", b.rget(b.loc("ents")), on_item));
    b.lam(&["c"], b.match_(b.loc("c"), [arm(cache_pat(b), body)]))
}

pub(super) fn values_expr(b: &ExprBuilder) -> Expr {
    let snd = b.lam(
        &["p"],
        b.match_(b.loc("p"), [arm(b.tup_pat(&["_k", "v"]), b.loc("v"))]),
    );
    b.lam(
        &["c"],
        b.app2(
            b.global("List", "map"),
            snd,
            b.app(b.global("Cache", "entries"), b.loc("c")),
        ),
    )
}

pub(super) fn entries_expr(b: &ExprBuilder) -> Expr {
    let on_live = b.match_(
        b.app(b.global("Exit", "value"), b.loc("ex")),
        [
            arm(
                b.some_pat("v"),
                b.yield_(b.tuple([b.loc("ek"), b.loc("v")])),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["ex", "exp"]),
            b.if_(is_expired_expr(b, b.loc("exp")), b.unit(), on_live),
        )],
    );
    let on_state = b.match_(
        b.loc("st"),
        [arm(b.some_pat("pair"), on_pair), arm(Pat::Wild, b.unit())],
    );
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(b.tup_pat(&["st", "_wd"]), on_state)],
    );
    let on_item = b.match_(b.loc("p"), [arm(b.tup_pat(&["ek", "ent"]), on_ent)]);
    let body = b.gen_strict(b.each("p", b.rget(b.loc("ents")), on_item));
    b.lam(&["c"], b.match_(b.loc("c"), [arm(cache_pat(b), body)]))
}
