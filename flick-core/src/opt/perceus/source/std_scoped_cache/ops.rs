//! `ScopedCache` access, mutation, invalidation, and enumeration templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_scoped_res::{lease_holder_expr, plain_holder_expr, retire_holder_expr};
use super::super::{Expr, Pat, Stmt};
use super::entry::{
    acquire_entry_expr, compute_ttl, get_or_acquire_holder, retire_entry_expr,
    retire_key_and_evict_expr,
};
use super::{drop_key, find_key, is_expired, sc_pat};

pub(super) fn use_opt_expr(b: &ExprBuilder, only_success: bool) -> Expr {
    let lease_some = lease_holder_expr(
        b,
        b.loc("h"),
        b.lam(&["v"], b.app(b.loc("f"), b.some(b.loc("v")))),
    );
    let on_err = if only_success {
        b.app(b.loc("f"), b.none())
    } else {
        b.app(b.global("Failure", "rethrow"), b.loc("c"))
    };
    let on_live = b.match_(
        b.loc("res"),
        [arm(b.ok_pat("h"), lease_some), arm(b.err_pat("c"), on_err)],
    );
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["res", "exp"]),
            b.if_(
                is_expired(b, b.loc("exp")),
                b.app(b.loc("f"), b.none()),
                on_live,
            ),
        )],
    );
    let resolved_pair = b.match_(
        b.loc("st"),
        [
            arm(b.some_pat("p"), b.loc("p")),
            arm(Pat::Wild, b.dawait(b.loc("wd"))),
        ],
    );
    let on_ent = b.match_(
        b.rget(b.loc("ent")),
        [arm(
            b.tup_pat(&["st", "wd"]),
            b.block([b.let_("pair", resolved_pair)], on_pair),
        )],
    );
    let on_found = b.match_(b.loc("found"), [arm(b.tup_pat(&["_ek", "ent"]), on_ent)]);
    let body = b.match_(
        find_key(b, b.loc("ents"), b.loc("k")),
        [
            arm(b.some_pat("found"), on_found),
            arm(Pat::Wild, b.app(b.loc("f"), b.none())),
        ],
    );
    b.lam(
        &["k", "c", "f"],
        b.match_(b.loc("c"), [arm(sc_pat(b), body)]),
    )
}

pub(super) fn use_expr(b: &ExprBuilder) -> Expr {
    let body = lease_holder_expr(b, get_or_acquire_holder(b), b.loc("f"));
    b.lam(
        &["k", "c", "f"],
        b.match_(b.loc("c"), [arm(sc_pat(b), body)]),
    )
}

pub(super) fn set_expr(b: &ExprBuilder) -> Expr {
    let body = b.block(
        [
            b.let_("res", b.ok(plain_holder_expr(b, b.loc("v")))),
            b.let_("exp", compute_ttl(b, b.loc("res"))),
            b.let_("wd", b.dmake()),
            b.let_("pair", b.tuple([b.loc("res"), b.loc("exp")])),
            b.dsucceed(b.loc("pair"), b.loc("wd")),
            b.let_(
                "ent",
                b.rmake(b.tuple([b.some(b.loc("pair")), b.loc("wd")])),
            ),
        ],
        retire_key_and_evict_expr(b, b.loc("k"), b.loc("ent")),
    );
    b.lam(
        &["k", "v", "c"],
        b.match_(b.loc("c"), [arm(sc_pat(b), body)]),
    )
}

pub(super) fn has_expr(b: &ExprBuilder) -> Expr {
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["_res", "exp"]),
            b.not(is_expired(b, b.loc("exp"))),
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
        find_key(b, b.loc("ents"), b.loc("k")),
        [
            arm(b.some_pat("found"), on_found),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    b.lam(&["k", "c"], b.match_(b.loc("c"), [arm(sc_pat(b), body)]))
}

pub(super) fn invalidate_expr(b: &ExprBuilder) -> Expr {
    let drop_and_retire = b.block(
        [b.rset(
            b.loc("ents"),
            drop_key(b, b.rget(b.loc("ents")), b.loc("k")),
        )],
        retire_entry_expr(b, b.loc("ent")),
    );
    let on_found = b.match_(
        b.loc("found"),
        [arm(b.tup_pat(&["_ek", "ent"]), drop_and_retire)],
    );
    let body = b.match_(
        find_key(b, b.loc("ents"), b.loc("k")),
        [arm(b.some_pat("found"), on_found), arm(Pat::Wild, b.unit())],
    );
    b.lam(&["k", "c"], b.match_(b.loc("c"), [arm(sc_pat(b), body)]))
}

pub(super) fn invalidate_when_expr(b: &ExprBuilder) -> Expr {
    let do_drop = b.block(
        [
            b.rset(
                b.loc("ents"),
                drop_key(b, b.rget(b.loc("ents")), b.loc("k")),
            ),
            Stmt::Expr(retire_holder_expr(b, b.loc("h"))),
        ],
        b.bool_(true),
    );
    let check_holder = b.match_(
        b.loc("h"),
        [arm(
            b.tup_pat(&["hv", "_ho", "_hm"]),
            b.if_(b.app(b.loc("pred"), b.loc("hv")), do_drop, b.bool_(false)),
        )],
    );
    let on_live = b.match_(
        b.loc("res"),
        [
            arm(b.ok_pat("h"), check_holder),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["res", "exp"]),
            b.if_(is_expired(b, b.loc("exp")), b.bool_(false), on_live),
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
        find_key(b, b.loc("ents"), b.loc("k")),
        [
            arm(b.some_pat("found"), on_found),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    b.lam(
        &["k", "pred", "c"],
        b.match_(b.loc("c"), [arm(sc_pat(b), body)]),
    )
}

pub(super) fn refresh_expr(b: &ExprBuilder) -> Expr {
    let body = b.match_(
        acquire_entry_expr(b),
        [
            arm(b.ok_pat("_h"), b.unit()),
            arm(
                b.err_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        ],
    );
    b.lam(&["k", "c"], b.match_(b.loc("c"), [arm(sc_pat(b), body)]))
}

pub(super) fn invalidate_all_expr(b: &ExprBuilder) -> Expr {
    let retire_one = b.match_(
        b.loc("p"),
        [arm(
            b.tup_pat(&["_ek", "ee"]),
            retire_entry_expr(b, b.loc("ee")),
        )],
    );
    let body = b.block(
        [
            b.let_("all", b.rget(b.loc("ents"))),
            b.rset(b.loc("ents"), b.list(vec![])),
            b.let_("_ia", b.for_ctrl("scia", "p", b.loc("all"), retire_one)),
        ],
        b.unit(),
    );
    b.lam(&["c"], b.match_(b.loc("c"), [arm(sc_pat(b), body)]))
}

pub(super) fn size_expr(b: &ExprBuilder) -> Expr {
    b.lam(
        &["c"],
        b.app(
            b.global("List.Core", "length"),
            b.app(b.global("ScopedCache", "keys"), b.loc("c")),
        ),
    )
}

pub(super) fn keys_expr(b: &ExprBuilder) -> Expr {
    let on_pair = b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["_res", "exp"]),
            b.if_(is_expired(b, b.loc("exp")), b.unit(), b.yield_(b.loc("ek"))),
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
    b.lam(&["c"], b.match_(b.loc("c"), [arm(sc_pat(b), body)]))
}
