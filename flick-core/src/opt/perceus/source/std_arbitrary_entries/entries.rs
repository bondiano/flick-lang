//! Record-entries shrink-tree helpers (`arb_entries_sample`, `arb_rebuild_entries`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};

pub(super) fn entries_sample_helper(b: &ExprBuilder) -> Expr {
    let pair_step = b.block(
        [
            b.let_(
                "k",
                b.match_(
                    b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("ks")),
                    [arm(pair_pat(b, "kv", "_"), b.loc("kv"))],
                ),
            ),
            b.let_(
                "v",
                b.match_(
                    b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("vs")),
                    [arm(pair_pat(b, "vv", "_"), b.loc("vv"))],
                ),
            ),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.yield_(b.tuple([b.loc("k"), b.loc("v")])),
    );
    let pairs = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("n")), pair_step));
    let ent_step = b.block(
        [
            b.let_(
                "kr",
                b.app(
                    rt(b, "Helper.arb_retain"),
                    b.app2(rt(b, "Helper.list_at"), b.var("j"), b.loc("ks")),
                ),
            ),
            b.let_(
                "vr",
                b.app(
                    rt(b, "Helper.arb_retain"),
                    b.app2(rt(b, "Helper.list_at"), b.var("j"), b.loc("vs")),
                ),
            ),
            b.let_(
                "kstr",
                b.match_(b.loc("kr"), [arm(pair_pat(b, "kv", "_"), b.loc("kv"))]),
            ),
            b.assign("j", b.add_i(b.var("j"), b.int(1))),
        ],
        b.yield_(b.tuple([b.loc("kstr"), b.loc("kr"), b.loc("vr")])),
    );
    let ents = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("j"), b.loc("n")), ent_step));
    let rebuilt = b.app3(
        rt(b, "Helper.arb_rebuild_entries"),
        b.loc("ents"),
        b.int(0),
        b.int(0),
    );
    let lazy_body = b.block(
        [b.var_decl("j", b.int(0)), b.let_("ents", ents)],
        b.match_(rebuilt, [arm(pair_pat(b, "_v", "sh"), b.loc("sh"))]),
    );
    let shrunk = b.app2(
        rt(b, "Helper.arb_lazy_shrinks"),
        b.loc("rec"),
        b.lam(&["_u"], lazy_body),
    );
    let stmts = [
        b.let_("n", b.app(b.global("List.Core", "length"), b.loc("ks"))),
        b.var_decl("i", b.int(0)),
        b.let_("pairs", pairs),
        b.let_(
            "rec",
            b.app(rt(b, "Schema.RecordFromEntries"), b.loc("pairs")),
        ),
    ];
    let body = b.if_(
        b.not(b.loc("shrinks")),
        b.tuple([b.loc("rec"), b.none()]),
        shrunk,
    );
    b.lam(&["ks", "vs", "shrinks"], b.block(stmts, body))
}

fn rem_pulls_expr(b: &ExprBuilder) -> Expr {
    let idx_step = b.block(
        [
            b.let_("cur", b.var("ri")),
            b.assign("ri", b.add_i(b.var("ri"), b.int(1))),
        ],
        b.yield_(b.loc("cur")),
    );
    let idxs = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("ri"), b.loc("n")), idx_step));
    let without_idx = b.app2(
        b.global("List.Core", "append"),
        b.app2(b.global("List.Core", "take"), b.loc("idx"), b.loc("ents")),
        b.app2(
            b.global("List.Core", "drop"),
            b.add_i(b.loc("idx"), b.int(1)),
            b.loc("ents"),
        ),
    );
    let rebuild_one = b.lam(
        &["idx"],
        b.some(b.app3(
            rt(b, "Helper.arb_rebuild_entries"),
            without_idx,
            b.int(0),
            b.int(0),
        )),
    );
    let pull_list = b.list([b.app2(rt(b, "Helper.arb_pull_each"), b.loc("idxs"), rebuild_one)]);
    let non_empty = b.block(
        [b.var_decl("ri", b.int(0)), b.let_("idxs", idxs)],
        pull_list,
    );
    b.if_(
        b.bin(BinOp::Gt, b.loc("n"), b.int(0)),
        non_empty,
        b.list(vec![]),
    )
}

fn val_pulls_expr(b: &ExprBuilder, ep: Pat) -> Expr {
    let replaced = b.app3(
        rt(b, "Helper.arb_replace_at"),
        b.loc("ents"),
        b.loc("idx"),
        b.tuple([b.loc("ek"), b.loc("ekr"), b.loc("r")]),
    );
    let mapped_r = b.lam(
        &["r"],
        b.app3(
            rt(b, "Helper.arb_rebuild_entries"),
            replaced,
            b.loc("idx"),
            b.int(0),
        ),
    );
    let on_mkc = b.yield_(b.app2(
        rt(b, "Helper.arb_map_retained"),
        b.app(b.loc("mkc"), b.unit()),
        mapped_r,
    ));
    let on_evr = b.match_(
        b.loc("vsh"),
        [arm(b.some_pat("mkc"), on_mkc), arm(Pat::Wild, b.unit())],
    );
    let on_ent = b.match_(b.loc("evr"), [arm(pair_pat(b, "_v", "vsh"), on_evr)]);
    let step = b.block(
        [
            b.let_("idx", b.var("vi")),
            b.assign("vi", b.add_i(b.var("vi"), b.int(1))),
        ],
        b.match_(
            b.app2(rt(b, "Helper.list_at"), b.loc("idx"), b.loc("ents")),
            [arm(ep, on_ent)],
        ),
    );
    b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("vi"), b.loc("n")), step))
}

fn key_unique_pred(b: &ExprBuilder) -> Expr {
    let check_eq = b.if_(
        b.bin(BinOp::Eq, b.loc("ok2"), b.loc("cand")),
        b.assign_e("ok", b.bool_(false)),
        b.unit(),
    );
    let check_other = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.var("ji"), b.loc("ents")),
        [arm(b.tup_pat(&["ok2", "_", "_"]), check_eq)],
    );
    let body = b.block(
        [
            Stmt::Expr(b.if_(
                b.bin(BinOp::NotEq, b.var("ji"), b.loc("idx")),
                check_other,
                b.unit(),
            )),
            b.assign("ji", b.add_i(b.var("ji"), b.int(1))),
        ],
        b.unit(),
    );
    let stmts = [
        b.var_decl("ok", b.bool_(true)),
        b.var_decl("ji", b.int(0)),
        Stmt::Expr(b.while_(b.bin(BinOp::Lt, b.var("ji"), b.loc("n")), body)),
    ];
    b.lam(&["cand"], b.block(stmts, b.var("ok")))
}

fn key_on_some_ksh(b: &ExprBuilder) -> Expr {
    let flt_expr = b.app2(
        rt(b, "Helper.arb_filter_sample"),
        b.app(rt(b, "Helper.arb_from_retained"), b.loc("ekr")),
        key_unique_pred(b),
    );
    let new_ent = b.tuple([
        b.loc("nk"),
        b.app(rt(b, "Helper.arb_retain"), b.loc("s")),
        b.loc("evr"),
    ]);
    let replaced = b.app3(
        rt(b, "Helper.arb_replace_at"),
        b.loc("ents"),
        b.loc("idx"),
        new_ent,
    );
    let rebuilt = b.app3(
        rt(b, "Helper.arb_rebuild_entries"),
        replaced,
        b.loc("n"),
        b.loc("idx"),
    );
    let on_s = b.lam(
        &["s"],
        b.match_(b.loc("s"), [arm(pair_pat(b, "nk", "_"), rebuilt)]),
    );
    let on_fp = b.yield_(b.app2(rt(b, "Helper.arb_map_pull"), b.loc("fp"), on_s));
    let on_fsh = b.match_(
        b.loc("fsh"),
        [arm(b.some_pat("fp"), on_fp), arm(Pat::Wild, b.unit())],
    );
    let match_sample = b.match_(b.loc("fs"), [arm(pair_pat(b, "_fv", "fsh"), on_fsh)]);
    let body = b.match_(
        b.loc("flt"),
        [
            arm(b.some_pat("fs"), match_sample),
            arm(Pat::Wild, b.unit()),
        ],
    );
    b.block([b.let_("flt", flt_expr)], body)
}

fn key_pulls_expr(b: &ExprBuilder, ep: Pat) -> Expr {
    let on_ksh = b.match_(
        b.loc("ksh"),
        [
            arm(b.some_pat("_"), key_on_some_ksh(b)),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let on_ekr = b.match_(b.loc("ekr"), [arm(pair_pat(b, "_kv", "ksh"), on_ksh)]);
    let step = b.block(
        [
            b.let_("idx", b.var("ki")),
            b.assign("ki", b.add_i(b.var("ki"), b.int(1))),
        ],
        b.match_(
            b.app2(rt(b, "Helper.list_at"), b.loc("idx"), b.loc("ents")),
            [arm(ep, on_ekr)],
        ),
    );
    b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("ki"), b.loc("n")), step))
}

pub(super) fn rebuild_entries_helper(b: &ExprBuilder) -> Expr {
    let ep = b.tup_pat(&["ek", "ekr", "evr"]);
    let all_p = b.app2(
        b.global("List.Core", "append"),
        b.loc("rem_pulls"),
        b.app2(
            b.global("List.Core", "append"),
            b.loc("val_pulls"),
            b.loc("key_pulls"),
        ),
    );
    let pair_arm = b.match_(
        b.loc("vr"),
        [arm(
            pair_pat(b, "v", "_"),
            b.yield_(b.tuple([b.loc("k"), b.loc("v")])),
        )],
    );
    let pairs = b.gen_strict(b.each(
        "e",
        b.loc("ents"),
        b.match_(b.loc("e"), [arm(b.tup_pat(&["k", "_", "vr"]), pair_arm)]),
    ));
    let sh = b.if_(
        b.bin(
            BinOp::Gt,
            b.app(b.global("List.Core", "length"), b.loc("all_p")),
            b.int(0),
        ),
        b.some(b.app(rt(b, "Helper.arb_concat_pulls"), b.loc("all_p"))),
        b.none(),
    );
    let stmts = [
        b.let_("n", b.app(b.global("List.Core", "length"), b.loc("ents"))),
        b.let_("rem_pulls", rem_pulls_expr(b)),
        b.var_decl("vi", b.loc("vstart")),
        b.let_("val_pulls", val_pulls_expr(b, ep.clone())),
        b.var_decl("ki", b.loc("kstart")),
        b.let_("key_pulls", key_pulls_expr(b, ep)),
        b.let_("all_p", all_p),
        b.let_("pairs", pairs),
        b.let_("sh", sh),
    ];
    let tail = b.tuple([
        b.app(rt(b, "Schema.RecordFromEntries"), b.loc("pairs")),
        b.loc("sh"),
    ]);
    b.lam(&["ents", "vstart", "kstart"], b.block(stmts, tail))
}
