//! Metric attribute sorting/merging and global registry lookup helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_observe_refs::upsert_pair_list;
use super::super::{Expr, Pat};

pub(in super::super) fn merge_attrs_expr(b: &ExprBuilder, base: Expr, extra: Expr) -> Expr {
    let step = b.match_(
        b.loc("ap"),
        [arm(
            b.tup_pat(&["ak", "av"]),
            b.block(
                [b.assign(
                    "ma",
                    upsert_pair_list(b, b.var("ma"), b.loc("ak"), b.loc("av")),
                )],
                b.unit(),
            ),
        )],
    );
    b.block(
        [
            b.var_decl("ma", base),
            b.let_("_ml", b.for_ctrl("mal", "ap", extra, step)),
        ],
        b.var("ma"),
    )
}

pub(in super::super) fn sort_pairs_expr(b: &ExprBuilder, pairs: Expr) -> Expr {
    // Insertion sort of `List (String, String)` by first element ascending.
    let should_insert = b.bin(
        BinOp::And,
        b.not(b.var("ins")),
        b.bin(BinOp::Lt, b.loc("pk"), b.loc("sk")),
    );
    let insert_here = b.block(
        [
            b.assign("ins", b.bool_(true)),
            b.assign(
                "nxt",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("nxt"),
                    b.list([b.loc("p"), b.loc("sp")]),
                ),
            ),
        ],
        b.unit(),
    );
    let keep_cur = b.block(
        [b.assign(
            "nxt",
            b.app2(
                b.global("List.Core", "append"),
                b.var("nxt"),
                b.list([b.loc("sp")]),
            ),
        )],
        b.unit(),
    );
    let inner = b.match_(
        b.loc("sp"),
        [arm(
            b.tup_pat(&["sk", "_sv"]),
            b.if_(should_insert, insert_here, keep_cur),
        )],
    );
    let append_tail = b.app2(
        b.global("List.Core", "append"),
        b.var("nxt"),
        b.list([b.loc("p")]),
    );
    let outer_body = b.block(
        [
            b.var_decl("nxt", b.list(vec![])),
            b.var_decl("ins", b.bool_(false)),
            b.let_("_il", b.for_ctrl("spi", "sp", b.var("res"), inner)),
            b.assign("res", b.if_(b.var("ins"), b.var("nxt"), append_tail)),
        ],
        b.unit(),
    );
    let outer = b.match_(b.loc("p"), [arm(b.tup_pat(&["pk", "_pv"]), outer_body)]);
    b.block(
        [
            b.var_decl("res", b.list(vec![])),
            b.let_("_ol", b.for_ctrl("spo", "p", pairs, outer)),
        ],
        b.var("res"),
    )
}

pub(super) fn sort_floats_expr(b: &ExprBuilder, xs: Expr) -> Expr {
    let should_insert = b.bin(
        BinOp::And,
        b.not(b.var("ins")),
        b.bin(BinOp::Lt, b.loc("x"), b.loc("sx")),
    );
    let insert_here = b.block(
        [
            b.assign("ins", b.bool_(true)),
            b.assign(
                "nxt",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("nxt"),
                    b.list([b.loc("x"), b.loc("sx")]),
                ),
            ),
        ],
        b.unit(),
    );
    let keep_cur = b.block(
        [b.assign(
            "nxt",
            b.app2(
                b.global("List.Core", "append"),
                b.var("nxt"),
                b.list([b.loc("sx")]),
            ),
        )],
        b.unit(),
    );
    let inner = b.if_(should_insert, insert_here, keep_cur);
    let append_tail = b.app2(
        b.global("List.Core", "append"),
        b.var("nxt"),
        b.list([b.loc("x")]),
    );
    let outer = b.block(
        [
            b.var_decl("nxt", b.list(vec![])),
            b.var_decl("ins", b.bool_(false)),
            b.let_("_il", b.for_ctrl("sfi", "sx", b.var("res"), inner)),
            b.assign("res", b.if_(b.var("ins"), b.var("nxt"), append_tail)),
        ],
        b.unit(),
    );
    b.block(
        [
            b.var_decl("res", b.list(vec![])),
            b.let_("_ol", b.for_ctrl("sfo", "x", xs, outer)),
        ],
        b.var("res"),
    )
}

fn registry_lookup_hook(b: &ExprBuilder) -> Expr {
    let find_pred = b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(
                b.tup_pat(&["ek", "_1", "_2", "_3", "_4", "_5", "_6", "_7"]),
                b.bin(BinOp::Eq, b.loc("ek"), b.loc("mkey")),
            )],
        ),
    );
    let hit_arm = b.match_(
        b.loc("ent"),
        [arm(
            b.tup_pat(&["_0", "_1", "_2", "_3", "_4", "gh", "uh", "mh"]),
            b.tuple([b.loc("gh"), b.loc("uh"), b.loc("mh")]),
        )],
    );
    let new_entry = b.tuple([
        b.loc("mkey"),
        b.loc("m_id"),
        b.loc("m_kind"),
        b.loc("m_desc"),
        b.loc("stored_attrs"),
        b.loc("gh"),
        b.loc("uh"),
        b.loc("mh"),
    ]);
    let append_entry = b.app2(
        b.global("List.Core", "append"),
        b.loc("ents"),
        b.list([new_entry]),
    );
    let store_hooks = b.match_(
        b.loc("hks"),
        [arm(
            b.tup_pat(&["gh", "uh", "mh"]),
            b.block(
                [b.rset(b.loc("reg"), append_entry)],
                b.tuple([b.loc("gh"), b.loc("uh"), b.loc("mh")]),
            ),
        )],
    );
    let has_extra = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("extra")),
        b.int(0),
    );
    let miss_arm = b.block(
        [
            b.let_("hks", b.app(b.loc("m_create"), b.unit())),
            b.let_(
                "stored_attrs",
                b.if_(has_extra, b.loc("merged"), b.loc("m_attrs")),
            ),
        ],
        store_hooks,
    );
    let mkey = b.tuple([
        b.loc("m_kind"),
        b.loc("m_id"),
        b.loc("m_desc"),
        sort_pairs_expr(b, b.loc("merged")),
    ]);
    let body = b.block(
        [
            b.let_(
                "merged",
                merge_attrs_expr(b, b.loc("m_attrs"), b.loc("extra")),
            ),
            b.let_("mkey", mkey),
            b.let_("reg", b.global("$rt", "Metric.Registry")),
            b.let_("ents", b.rget(b.loc("reg"))),
            b.let_(
                "hit",
                b.app2(b.global("List.Core", "find"), find_pred, b.loc("ents")),
            ),
        ],
        b.match_(
            b.loc("hit"),
            [arm(b.some_pat("ent"), hit_arm), arm(Pat::Wild, miss_arm)],
        ),
    );
    b.lam(&["extra"], b.no_preempt(body))
}

// Metric value: `(get_fn : Extra ->{} s, upd_fn : Extra -> i ->{} Unit, mod_fn : Extra -> i ->{} Unit)`
// Registry entry: `(key, id, kind, desc, attrs, get_h, upd_h, mod_h)`
pub(in super::super) fn make_metric_expr(
    b: &ExprBuilder,
    id: Expr,
    kind: &str,
    desc: Expr,
    attrs: Expr,
    create_hooks: Expr,
) -> Expr {
    let get_fn = b.lam(
        &["ex"],
        b.match_(
            b.app(b.loc("hook"), b.loc("ex")),
            [arm(
                b.tup_pat(&["gh", "_u", "_m"]),
                b.app(b.loc("gh"), b.unit()),
            )],
        ),
    );
    let upd_fn = b.lam(
        &["ex", "inp"],
        b.match_(
            b.app(b.loc("hook"), b.loc("ex")),
            [arm(
                b.tup_pat(&["_g", "uh", "_m"]),
                b.app(b.loc("uh"), b.loc("inp")),
            )],
        ),
    );
    let mod_fn = b.lam(
        &["ex", "inp"],
        b.match_(
            b.app(b.loc("hook"), b.loc("ex")),
            [arm(
                b.tup_pat(&["_g", "_u", "mh"]),
                b.app(b.loc("mh"), b.loc("inp")),
            )],
        ),
    );
    b.block(
        [
            b.let_("m_id", id),
            b.let_("m_kind", b.str_(kind)),
            b.let_("m_desc", desc),
            b.let_("m_attrs", attrs),
            b.let_("m_create", create_hooks),
            b.let_("hook", registry_lookup_hook(b)),
        ],
        b.tuple([get_fn, upd_fn, mod_fn]),
    )
}
