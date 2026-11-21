//! Compound `$Kind` compilation arms for `arb_add_schema`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{con_app, pair_pat, rt, str_lit, triple_pat};
use super::super::{Expr, Pat};
use super::{arb_err, c_pat, float_inf, node_con, push_node, push_simple, raw_entry, set_node};

fn c_opt_len_bound(b: &ExprBuilder, field_var: &str, fallback: Expr) -> Expr {
    let inner = b.match_(
        b.loc(field_var),
        [
            arm(b.some_pat("x"), b.loc("x")),
            arm(Pat::Wild, fallback.clone()),
        ],
    );
    b.match_(
        b.loc("c_opt"),
        [
            arm(
                b.some_pat("c"),
                b.match_(b.loc("c"), [arm(c_pat(b), inner)]),
            ),
            arm(Pat::Wild, fallback),
        ],
    )
}

pub(super) fn list_arm(b: &ExprBuilder) -> Expr {
    let add_it = b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.loc("it"),
        b.loc("wire"),
        b.none(),
    );
    let spec = node_con(
        b,
        "$Arrays",
        vec![
            b.list(vec![]),
            b.some(b.loc("iid")),
            b.bool_(false),
            b.loc("alo"),
            b.loc("ahi"),
        ],
    );
    let stmts = [
        b.let_("iid", add_it),
        b.let_("alo", c_opt_len_bound(b, "cminl", b.float(0.0))),
        b.let_("ahi", c_opt_len_bound(b, "cmaxl", float_inf(b))),
    ];
    b.block(stmts, push_simple(b, b.list([b.loc("iid")]), spec))
}

pub(super) fn tuple_arm(b: &ExprBuilder) -> Expr {
    let eids = b.gen_strict(b.each(
        "it",
        b.loc("its"),
        b.yield_(b.app4(
            rt(b, "Helper.arb_add_schema"),
            b.loc("ctx"),
            b.loc("it"),
            b.loc("wire"),
            b.none(),
        )),
    ));
    let spec = node_con(
        b,
        "$Arrays",
        vec![
            b.loc("eids"),
            b.none(),
            b.bool_(true),
            b.float(0.0),
            float_inf(b),
        ],
    );
    b.block([b.let_("eids", eids)], push_simple(b, b.loc("eids"), spec))
}

pub(super) fn rec_arm(b: &ExprBuilder) -> Expr {
    let ks = b.gen_strict(b.each(
        "f",
        b.loc("flds"),
        b.match_(
            b.loc("f"),
            [arm(pair_pat(b, "k", "_"), b.yield_(b.loc("k")))],
        ),
    ));
    let add_fs = b.yield_(b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.loc("fs"),
        b.loc("wire"),
        b.none(),
    ));
    let pids = b.gen_strict(b.each(
        "f",
        b.loc("flds"),
        b.match_(b.loc("f"), [arm(pair_pat(b, "_k", "fs"), add_fs)]),
    ));
    let ent_step = b.block(
        [
            b.let_(
                "k",
                b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("ks")),
            ),
            b.let_(
                "v",
                b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("vs")),
            ),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.yield_(b.tuple([b.loc("k"), b.loc("v")])),
    );
    let ents = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("n")), ent_step));
    let mk = b.lam(
        &["vs"],
        b.block(
            [
                b.let_("n", b.app(b.global("List.Core", "length"), b.loc("ks"))),
                b.var_decl("i", b.int(0)),
                b.let_("ents", ents),
            ],
            b.app(rt(b, "Schema.RecordFromSortedEntries"), b.loc("ents")),
        ),
    );
    let stmts = [b.let_("ks", ks), b.let_("pids", pids), b.let_("mk", mk)];
    let spec = node_con(b, "$Objects", vec![b.loc("pids"), b.loc("mk")]);
    b.block(stmts, push_simple(b, b.loc("pids"), spec))
}

pub(super) fn adt_arm(b: &ExprBuilder) -> Expr {
    let fids = b.gen_strict(b.each(
        "it",
        b.loc("its"),
        b.yield_(b.app4(
            rt(b, "Helper.arb_add_schema"),
            b.loc("ctx"),
            b.loc("it"),
            b.loc("wire"),
            b.none(),
        )),
    ));
    let mk_adt = b.lam(
        &["vs"],
        b.app2(rt(b, "Schema.ConstructAdt"), b.loc("ftag"), b.loc("vs")),
    );
    let case_node = push_simple(
        b,
        b.loc("fids"),
        node_con(b, "$Objects", vec![b.loc("fids"), mk_adt]),
    );
    let on_case = b.block([b.let_("fids", fids)], b.yield_(case_node));
    let mids = b.gen_strict(b.each(
        "cs",
        b.loc("cases"),
        b.match_(
            b.loc("cs"),
            [arm(triple_pat(b, "ftag", "_sn", "its"), on_case)],
        ),
    ));
    let spec = node_con(b, "$Union", vec![b.loc("mids")]);
    b.block([b.let_("mids", mids)], push_simple(b, b.loc("mids"), spec))
}

pub(super) fn opt_arm(b: &ExprBuilder) -> Expr {
    let add_it = b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.loc("it"),
        b.loc("wire"),
        b.none(),
    );
    let mk_some = b.lam(
        &["vs"],
        b.some(b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("vs"))),
    );
    let some_node = push_simple(
        b,
        b.list([b.loc("iid")]),
        node_con(b, "$Objects", vec![b.list([b.loc("iid")]), mk_some]),
    );
    let none_node = push_simple(b, b.list(vec![]), node_con(b, "$Literal", vec![b.none()]));
    let stmts = [
        b.let_("iid", add_it),
        b.let_("some_id", some_node),
        b.let_("none_id", none_node),
        b.let_("mids", b.list([b.loc("some_id"), b.loc("none_id")])),
    ];
    b.block(
        stmts,
        push_simple(b, b.loc("mids"), node_con(b, "$Union", vec![b.loc("mids")])),
    )
}

pub(super) fn res_arm(b: &ExprBuilder) -> Expr {
    let add_ok = b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.loc("os"),
        b.bool_(false),
        b.none(),
    );
    let add_err = b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.loc("es"),
        b.bool_(false),
        b.none(),
    );
    let mk_ok = b.lam(
        &["vs"],
        b.ok(b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("vs"))),
    );
    let mk_err = b.lam(
        &["vs"],
        b.err(b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("vs"))),
    );
    let ok_node = push_simple(
        b,
        b.list([b.loc("oid")]),
        node_con(b, "$Objects", vec![b.list([b.loc("oid")]), mk_ok]),
    );
    let err_node = push_simple(
        b,
        b.list([b.loc("eid")]),
        node_con(b, "$Objects", vec![b.list([b.loc("eid")]), mk_err]),
    );
    let stmts = [
        b.let_("oid", add_ok),
        b.let_("eid", add_err),
        b.let_("ok_id", ok_node),
        b.let_("err_id", err_node),
        b.let_("mids", b.list([b.loc("ok_id"), b.loc("err_id")])),
    ];
    let ok_branch = b.block(
        stmts,
        push_simple(b, b.loc("mids"), node_con(b, "$Union", vec![b.loc("mids")])),
    );
    b.if_(
        b.loc("wire"),
        arb_err(b, str_lit(b, "an unsupported Declaration")),
        ok_branch,
    )
}

pub(super) fn lazy_arm(b: &ExprBuilder) -> Expr {
    let find_pred = b.lam(
        &["p"],
        b.match_(
            b.loc("p"),
            [arm(
                pair_pat(b, "k", "_"),
                b.bin(BinOp::Eq, b.loc("k"), b.loc("lname")),
            )],
        ),
    );
    let found = b.app2(
        b.global("List.Core", "find"),
        find_pred,
        b.rget(b.loc("lcr")),
    );
    let placeholder = push_node(
        b,
        b.loc("ctx"),
        raw_entry(
            b,
            b.list(vec![]),
            node_con(b, "$Never", vec![]),
            b.list(vec![]),
            b.none(),
            true,
        ),
    );
    let append_cache = b.app2(
        b.global("List.Core", "append"),
        b.rget(b.loc("lcr")),
        b.list([b.tuple([b.loc("lname"), b.loc("sid")])]),
    );
    let add_body = b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.app(b.loc("lth"), b.unit()),
        b.loc("wire"),
        b.none(),
    );
    let patch_sus = set_node(
        b,
        b.loc("ctx"),
        b.loc("sid"),
        raw_entry(
            b,
            b.list([b.loc("bid")]),
            node_con(b, "$Suspend", vec![b.loc("bid")]),
            b.list(vec![]),
            b.none(),
            true,
        ),
    );
    let on_miss = b.block(
        [
            b.let_("sid", placeholder),
            b.rset(b.loc("lcr"), append_cache),
            b.let_("bid", add_body),
            patch_sus,
        ],
        b.loc("sid"),
    );
    let on_hit = b.match_(b.loc("fp"), [arm(pair_pat(b, "_", "id"), b.loc("id"))]);
    let body = b.block(
        [b.let_("found", found)],
        b.match_(
            b.loc("found"),
            [arm(b.some_pat("fp"), on_hit), arm(Pat::Wild, on_miss)],
        ),
    );
    b.match_(b.loc("ctx"), [arm(pair_pat(b, "_nr", "lcr"), body)])
}

pub(super) fn chunk_arm(b: &ExprBuilder) -> Expr {
    let add_it = b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.loc("it"),
        b.bool_(false),
        b.none(),
    );
    let arr_spec = node_con(
        b,
        "$Arrays",
        vec![
            b.list(vec![]),
            b.some(b.loc("iid")),
            b.bool_(false),
            b.float(0.0),
            float_inf(b),
        ],
    );
    let wrap_chunk = b.lam(
        &["xs"],
        con_app(b, "Chunk.Core", "Chunk", "$Chunk", vec![b.loc("xs")]),
    );
    let map_spec = node_con(b, "$Map", vec![b.loc("lid"), wrap_chunk]);
    b.block(
        [
            b.let_("iid", add_it),
            b.let_("lid", push_simple(b, b.list([b.loc("iid")]), arr_spec)),
        ],
        push_simple(b, b.list([b.loc("lid")]), map_spec),
    )
}
